//! Mods: analysis, install pipeline, removal (PLAN.md §9.2, §9.4, §9.5).
//!
//! Installing is two steps so the user can confirm what was detected:
//! [`inspect`] looks at a folder or ZIP, [`install`] stores the mod and creates
//! its play profile.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backups::{self, Contents, Reason};
use crate::catalog::{self, Game, GameVersion};
use crate::ckpatch;
use crate::db::Db;
use crate::detect::{detect, sha256_hex};
use crate::error::{Error, Result};
use crate::library::{self, Install, now};
use crate::paths::AppPaths;
use crate::recipe::{self, Recipe, VersionRecipes, has_ext, patcher_game};
use crate::source::{Source, split_dir};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ModSource {
    Index,
    Keenwiki,
    Manual,
}

impl ModSource {
    fn as_str(self) -> &'static str {
        match self {
            ModSource::Index => "index",
            ModSource::Keenwiki => "keenwiki",
            ModSource::Manual => "manual",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "index" => ModSource::Index,
            "keenwiki" => ModSource::Keenwiki,
            "manual" => ModSource::Manual,
            _ => return None,
        })
    }
}

/// What [`inspect`] found, for the user to confirm.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    pub name: String,
    pub authors: Vec<String>,
    pub version: Option<String>,
    /// `YYYY-MM-DD`, when known.
    pub released: Option<String>,
    pub game_id: String,
    /// Ways to start the mod, best first.
    pub recipes: Vec<RecipeOption>,
    /// False when the user should pick from `recipes`.
    pub confident: bool,
    pub file_count: u32,
    /// The index entry whose download this is, byte for byte.
    pub index_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeOption {
    pub recipe: Recipe,
    /// The file that suggested this recipe, e.g. `PLAY.BAT`.
    pub origin: String,
    /// Versions the patch file names, e.g. `1.4`.
    pub game_versions: Vec<String>,
    /// Catalog versions matching `game_versions`. Empty when any version
    /// may do, or the catalog doesn't know the named versions.
    pub version_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InstallModRequest {
    /// The folder or ZIP passed to [`inspect`].
    pub path: String,
    pub name: String,
    /// One of the recipes [`inspect`] offered.
    pub recipe: Recipe,
    /// The install the mod is played on; `None` to install it without its
    /// game, which it then waits for.
    pub install_id: Option<String>,
    /// Set when the mod came from a KeenWiki page.
    #[serde(default)]
    pub keenwiki: Option<KeenwikiOrigin>,
    /// An installed mod of the same game this one replaces, e.g. an older
    /// version. Its id and saves stay; `None` installs alongside.
    #[serde(default)]
    pub replace: Option<String>,
}

/// Where a KeenWiki mod came from, and what its page says.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeenwikiOrigin {
    pub page_url: String,
    pub download_url: String,
    /// The page's "Modded episode", for archives that don't tell.
    pub game_id: Option<String>,
    /// Replace the guessed ones when not empty.
    pub authors: Vec<String>,
    /// Author name → their page.
    #[serde(default)]
    pub author_links: BTreeMap<String, String>,
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// `YYYY-MM-DD`, when known.
    #[serde(default)]
    pub released: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Mod {
    pub id: String,
    pub game_id: String,
    pub name: String,
    pub authors: Vec<String>,
    /// Author name → their page, for the authors that have one.
    pub author_links: BTreeMap<String, String>,
    pub version: Option<String>,
    pub description: Option<String>,
    /// `YYYY-MM-DD`, when known.
    pub released: Option<String>,
    pub recipe: Recipe,
    pub source: ModSource,
    /// The index entry it was installed from.
    pub index_id: Option<String>,
    pub keenwiki_url: Option<String>,
    /// Unix epoch seconds.
    #[specta(type = u32)]
    pub added_at: i64,
    /// The play profile created for the mod.
    pub profile_id: String,
}

/// The mod inside a source.
struct Analysis {
    /// Directory of the mod inside the source, `""` for the root.
    root: String,
    /// Files to install, relative to the source.
    files: Vec<String>,
    /// More places for some of them: source path → path in the mod.
    copies: Vec<(String, String)>,
    info: ModInfo,
}

/// `game_hint` is used when neither the files nor the archive name tell.
pub fn inspect(path: &Path, game_hint: Option<&str>) -> Result<ModInfo> {
    Ok(analyze(&Source::open(path)?, path, game_hint)?.info)
}

/// What a folder or ZIP dropped on the window holds.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Dropped {
    Games,
    Mod { info: ModInfo },
    Nothing,
}

/// Tells games from a mod. Game detection goes by file names, and many mods
/// ship the game's files, often with their own executable. So whatever starts
/// through a patch or a batch file is a mod, even where a game is detected.
pub fn classify(path: &Path) -> Result<Dropped> {
    let source = Source::open(path)?;
    let game = !detect(&source)?.is_empty();
    let analysis = match analyze(&source, path, None) {
        Ok(a) => Some(a.info),
        Err(Error::NoModFound) => None,
        Err(e) => return Err(e),
    };
    Ok(decide(game, analysis))
}

fn decide(game: bool, info: Option<ModInfo>) -> Dropped {
    let started_by_mod = info.as_ref().is_some_and(|i| {
        i.recipes
            .first()
            .is_some_and(|o| matches!(o.recipe, Recipe::Patch { .. } | Recipe::Batch { .. }))
    });
    match info {
        _ if game && !started_by_mod => Dropped::Games,
        Some(info) => Dropped::Mod { info },
        None => Dropped::Nothing,
    }
}

fn analyze(source: &Source, path: &Path, game_hint: Option<&str>) -> Result<Analysis> {
    let files: Vec<String> = source
        .files()?
        .into_iter()
        .filter(|f| !is_junk(split_dir(f).1))
        .collect();
    // Patchers by name, or by the usage line of renamed ones.
    let patchers: BTreeMap<&str, &'static str> = files
        .iter()
        .filter_map(|f| {
            let name = split_dir(f).1;
            if !(has_ext(name, "EXE") || has_ext(name, "COM")) {
                return None;
            }
            let game = patcher_game(name).or_else(|| {
                let exe = source.read(f).ok()?;
                recipe::patcher_game_of(&exe)
            })?;
            Some((f.as_str(), game))
        })
        .collect();
    let root = find_root(&files, |f| patchers.contains_key(f)).ok_or(Error::NoModFound)?;
    let in_root = |f: &&String| root.is_empty() || f.starts_with(&format!("{root}/"));
    let mut files: Vec<String> = files.iter().filter(in_root).cloned().collect();
    // Bare names in the mod root, where recipes have to live.
    let names: Vec<String> = files
        .iter()
        .filter(|f| split_dir(f).0 == root)
        .map(|f| split_dir(f).1.to_string())
        .collect();
    let rel = |name: &str| {
        if root.is_empty() {
            name.to_string()
        } else {
            format!("{root}/{name}")
        }
    };
    let read = |name: &str| {
        source
            .read(&rel(name))
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };

    let meta = guess_metadata(path, &root);
    let is_patcher = |name: &str| patchers.contains_key(rel(name).as_str());
    let by_patcher = names
        .iter()
        .find_map(|n| patchers.get(rel(n).as_str()).copied());
    let game = find_game(&names, &read, by_patcher, meta.episode.or(game_hint))
        .ok_or(Error::NoModFound)?;

    // Pristine game executables would hide the install's own version.
    let mut pristine = Vec::new();
    for name in &names {
        if game
            .executables
            .iter()
            .any(|e| e.eq_ignore_ascii_case(name))
        {
            let hash = sha256_hex(&source.read(&rel(name))?);
            if game
                .versions
                .iter()
                .any(|v| v.files.values().any(|h| *h == hash))
            {
                pristine.push(rel(name));
            }
        }
    }
    files.retain(|f| !pristine.contains(f));
    // Keen 1-3's settings: a mod's are its author's keys and sound, which the
    // game's own menus change, so the game starts with its defaults instead.
    files.retain(|f| {
        let (dir, name) = split_dir(f);
        let settings = name
            .rsplit_once('.')
            .is_some_and(|(stem, _)| stem.eq_ignore_ascii_case("CTLPANEL"));
        !(dir == root && settings && episode_of_ext(name) == Some(game.id.as_str()))
    });

    // Without a patcher of its own, a patch file runs with BloogBase's.
    let bundled = ckpatch::needed(
        &game.id,
        names.iter().map(|n| (n.as_str(), read(n))),
        is_patcher,
    )
    .then(|| ckpatch::bundled(&game.id))
    .flatten()
    .map(|(name, _)| name.to_string());
    let with_bundled: Vec<String> = names.iter().cloned().chain(bundled.clone()).collect();
    let is_patcher = |name: &str| {
        is_patcher(name)
            || bundled
                .as_deref()
                .is_some_and(|b| b.eq_ignore_ascii_case(name))
    };
    let guess = recipe::guess(&with_bundled, read, &game.executables, is_patcher);
    let recipes = guess
        .candidates
        .into_iter()
        .map(|c| {
            let game_versions = c
                .recipe
                .patch_file()
                .and_then(read)
                .map(|t| recipe::patch_versions(&t))
                .unwrap_or_default();
            RecipeOption {
                version_ids: version_ids(game, &game_versions),
                game_versions,
                recipe: c.recipe,
                origin: c.origin,
            }
        })
        .collect();

    Ok(Analysis {
        info: ModInfo {
            name: meta.name,
            authors: meta.authors,
            version: meta.version,
            released: meta.released,
            game_id: game.id.clone(),
            recipes,
            confident: guess.confident,
            file_count: files.len() as u32,
            index_id: None,
        },
        root,
        files,
        copies: Vec::new(),
    })
}

fn is_junk(name: &str) -> bool {
    name.starts_with("._")
        || ["THUMBS.DB", ".DS_STORE", "DESKTOP.INI"]
            .iter()
            .any(|j| j.eq_ignore_ascii_case(name))
}

/// Whether a file name says "Commander Keen data": episode data, patches,
/// patchers, episode executables.
fn is_keen_file(name: &str) -> bool {
    episode_of_ext(name).is_some()
        || has_ext(name, "PAT")
        || patcher_game(name).is_some()
        || catalog::games()
            .iter()
            .any(|g| g.executables.iter().any(|e| e.eq_ignore_ascii_case(name)))
}

fn episode_of_ext(name: &str) -> Option<&'static str> {
    let (_, ext) = name.rsplit_once('.')?;
    Some(match ext.to_ascii_uppercase().as_str() {
        "CK1" => "keen1",
        "CK2" => "keen2",
        "CK3" => "keen3",
        "CK4" => "keen4",
        "CK5" => "keen5",
        "CK6" => "keen6",
        "KDR" => "kdreams",
        _ => return None,
    })
}

/// The directory with the most Keen files directly inside it; the shallowest
/// one on a tie. Mods are often zipped with a folder around them. Folders the
/// mod can start from (a patcher, a patch or a game executable) come first:
/// Keen Rescue keeps more levels in `levels/` than files next to its patcher.
/// `is_patcher` takes a path and also knows renamed patchers.
fn find_root(files: &[String], is_patcher: impl Fn(&str) -> bool) -> Option<String> {
    let mut score: BTreeMap<&str, (bool, usize)> = BTreeMap::new();
    for f in files {
        let (dir, name) = split_dir(f);
        if is_keen_file(name) || is_patcher(f) {
            let s = score.entry(dir).or_default();
            s.0 |= episode_of_ext(name).is_none();
            s.1 += 1;
        }
    }
    score
        .into_iter()
        .max_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| depth(b.0).cmp(&depth(a.0)))
                .then_with(|| b.0.cmp(a.0))
        })
        .map(|(dir, _)| dir.to_string())
}

fn depth(dir: &str) -> usize {
    if dir.is_empty() {
        0
    } else {
        dir.matches('/').count() + 1
    }
}

/// The target episode: the patcher says it best, then a patch file's `%ext`,
/// then the data files, then the archive name.
fn find_game(
    names: &[String],
    read: &impl Fn(&str) -> Option<String>,
    by_patcher: Option<&'static str>,
    hint: Option<&str>,
) -> Option<&'static Game> {
    let by_ext = || {
        names
            .iter()
            .filter_map(|n| {
                let text = read(n)?;
                (has_ext(n, "PAT") || recipe::is_patch_text(n, &text)).then_some(text)
            })
            .filter_map(|t| recipe::patch_ext(&t))
            .find_map(|ext| episode_of_ext(&format!("x.{ext}")))
    };
    let by_data = || {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for n in names {
            if let Some(g) = episode_of_ext(n) {
                *counts.entry(g).or_default() += 1;
            }
        }
        counts.into_iter().max_by_key(|(_, c)| *c).map(|(g, _)| g)
    };
    let by_exe = || {
        catalog::games()
            .iter()
            .find(|g| {
                g.executables
                    .iter()
                    .any(|e| names.iter().any(|n| n.eq_ignore_ascii_case(e)))
            })
            .map(|g| g.id.as_str())
    };
    let id = by_patcher
        .or_else(by_ext)
        .or_else(by_data)
        .or_else(by_exe)
        .or(hint)?;
    catalog::game(id)
}

/// Catalog versions of `game` matching patch file versions like `1.4`,
/// plus third-party builds of those.
fn version_ids(game: &Game, versions: &[String]) -> Vec<String> {
    let originals: Vec<&str> = game
        .versions
        .iter()
        .filter(|v| {
            v.patch_version
                .as_ref()
                .is_some_and(|p| versions.contains(p))
        })
        .map(|v| v.id.as_str())
        .collect();
    game.versions
        .iter()
        .filter(|v| originals.contains(&v.original.as_deref().unwrap_or(&v.id)))
        .map(|v| v.id.clone())
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct Metadata {
    name: String,
    authors: Vec<String>,
    version: Option<String>,
    released: Option<String>,
    episode: Option<&'static str>,
}

/// Name and details from the file name. Understands the archive naming
/// scheme `m.<author>.<name>[_v<version>].<episode>.<date>`
/// (e.g. `m.dave216.aurora_bliss_v.1.2.k4.2023-08-23.zip`); otherwise uses
/// the mod's folder or the file name.
fn guess_metadata(path: &Path, root: &str) -> Metadata {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = if path.is_dir() {
        file_name.as_str()
    } else {
        file_name
            .rsplit_once('.')
            .map_or(file_name.as_str(), |(s, _)| s)
    };

    let parts: Vec<&str> = stem.split('.').collect();
    // Level packs without a name: `l.<author>.<episode>.<date>`.
    if parts.len() == 4
        && parts[0].eq_ignore_ascii_case("l")
        && is_date(parts[3])
        && let Some(episode) = episode_code(parts[2])
    {
        return Metadata {
            name: format!("{} Level Pack", title_case(&parts[1].replace('_', " "))),
            authors: vec![parts[1].to_string()],
            version: None,
            released: Some(parts[3].to_string()),
            episode: Some(episode),
        };
    }
    if parts.len() >= 5
        && parts[0].len() == 1
        && is_date(parts[parts.len() - 1])
        && let Some(episode) = episode_code(parts[parts.len() - 2])
    {
        let raw = parts[2..parts.len() - 2].join(".");
        let (name, version) = split_version(&raw);
        return Metadata {
            name: title_case(&name.replace('_', " ")),
            authors: vec![parts[1].to_string()],
            version,
            released: Some(parts[parts.len() - 1].to_string()),
            episode: Some(episode),
        };
    }

    let folder = split_dir(root).1;
    let name = if folder.is_empty() { stem } else { folder };
    let name = name.replace('_', " ").trim().to_string();
    let name = if name.chars().any(|c| c.is_ascii_uppercase()) {
        name
    } else {
        title_case(&name)
    };
    Metadata {
        name: if name.is_empty() { "Mod".into() } else { name },
        authors: Vec::new(),
        version: None,
        released: None,
        episode: None,
    }
}

fn episode_code(code: &str) -> Option<&'static str> {
    Some(match code.to_ascii_lowercase().as_str() {
        "k1" => "keen1",
        "k2" => "keen2",
        "k3" => "keen3",
        "k4" => "keen4",
        "k5" => "keen5",
        "k6" => "keen6",
        "kd" => "kdreams",
        _ => return None,
    })
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

/// `aurora_bliss_v.1.2` → (`aurora_bliss`, `1.2`); also `simulator_keen.v.1.1`.
fn split_version(raw: &str) -> (String, Option<String>) {
    for sep in ["_v", ".v"] {
        if let Some((name, v)) = raw.rsplit_once(sep) {
            let v = v.trim_start_matches('.');
            if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return (name.to_string(), Some(v.to_string()));
            }
        }
    }
    (raw.to_string(), None)
}

fn title_case(s: &str) -> String {
    const SMALL: &[&str] = &["a", "an", "and", "in", "of", "on", "the", "to"];
    s.split_whitespace()
        .enumerate()
        .map(|(i, w)| {
            if i > 0 && SMALL.contains(&w) {
                return w.to_string();
            }
            if is_roman_numeral(w) {
                return w.to_ascii_uppercase();
            }
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether `w` is a sequel number from I to XXXIX, as in "Patchwork Mod III".
fn is_roman_numeral(w: &str) -> bool {
    let w = w.to_ascii_uppercase();
    let rest = w.trim_start_matches('X');
    if w.len() - rest.len() > 3 {
        return false;
    }
    !w.is_empty() && ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX"].contains(&rest)
}

/// What gets stored for a mod, whatever its source.
struct Stored<'a> {
    name: String,
    authors: Vec<String>,
    author_links: BTreeMap<String, String>,
    version: Option<String>,
    description: Option<String>,
    released: Option<String>,
    recipe: Recipe,
    version_recipes: VersionRecipes,
    /// Catalog versions it plays on; `None` for any.
    requires: Option<Vec<String>>,
    source: ModSource,
    index_id: Option<&'a str>,
    keenwiki_url: Option<&'a str>,
    /// The picked folder or ZIP, or the download URL.
    source_url: &'a str,
    /// Name for the kept copy of the archive; default: its own.
    archive_name: Option<&'a str>,
}

/// Installs a mod and creates its play profile on `install_id`, or without
/// a game.
pub fn install(paths: &AppPaths, db: &Db, req: &InstallModRequest) -> Result<Mod> {
    let path = Path::new(&req.path);
    let source = Source::open(path)?;
    let origin = req.keenwiki.as_ref();
    let analysis = analyze(&source, path, origin.and_then(|o| o.game_id.as_deref()))?;
    let option = analysis
        .info
        .recipes
        .iter()
        .find(|o| o.recipe == req.recipe)
        .ok_or_else(|| Error::IncompatibleInstall("unknown recipe".into()))?;
    let target = match &req.install_id {
        Some(id) => {
            let target = library::list(db)?
                .into_iter()
                .find(|i| i.id == *id)
                .ok_or_else(|| Error::NotFound(format!("install {id}")))?;
            check_install(&analysis.info.game_id, option, &target)?;
            Some(target)
        }
        None => None,
    };
    // Like `check_install`: a patch that names versions plays on those only.
    let requires = (!option.game_versions.is_empty()).then(|| option.version_ids.clone());

    let name = match req.name.trim() {
        "" => analysis.info.name.clone(),
        n => n.to_string(),
    };
    let replaced = match &req.replace {
        Some(id) => {
            let old = list(db)?
                .into_iter()
                .find(|m| m.id == *id)
                .ok_or_else(|| Error::NotFound(format!("mod {id}")))?;
            if old.game_id != analysis.info.game_id {
                return Err(Error::IncompatibleInstall(format!(
                    "{} is for {}, not {}",
                    old.name, old.game_id, analysis.info.game_id
                )));
            }
            Some(old)
        }
        None => None,
    };
    let mut stored = match origin {
        Some(o) => Stored {
            name,
            authors: if o.authors.is_empty() {
                analysis.info.authors.clone()
            } else {
                o.authors.clone()
            },
            author_links: o.author_links.clone(),
            version: o.version.clone().or_else(|| analysis.info.version.clone()),
            description: o.description.clone(),
            released: o
                .released
                .clone()
                .or_else(|| analysis.info.released.clone()),
            recipe: option.recipe.clone(),
            version_recipes: VersionRecipes::new(),
            requires: requires.clone(),
            source: ModSource::Keenwiki,
            index_id: None,
            keenwiki_url: Some(&o.page_url),
            source_url: &o.download_url,
            archive_name: None,
        },
        None => Stored {
            name,
            authors: analysis.info.authors.clone(),
            author_links: BTreeMap::new(),
            version: analysis.info.version.clone(),
            description: None,
            released: analysis.info.released.clone(),
            recipe: option.recipe.clone(),
            version_recipes: VersionRecipes::new(),
            requires,
            source: ModSource::Manual,
            index_id: None,
            keenwiki_url: None,
            source_url: &req.path,
            archive_name: None,
        },
    };
    let Some(old) = &replaced else {
        return store(paths, db, &source, &analysis, &stored, target.as_ref());
    };
    // What the files can't tell is still true of the new version.
    if stored.author_links.is_empty() {
        stored.author_links = old.author_links.clone();
    }
    stored.description = stored.description.or_else(|| old.description.clone());
    stored.keenwiki_url = stored.keenwiki_url.or(old.keenwiki_url.as_deref());
    update(
        paths,
        db,
        &old.id,
        &source,
        &analysis,
        &stored,
        target.as_ref(),
    )
}

/// A mod as the index describes it. Its recipe is used as is: a curated
/// recipe always beats the heuristics (§9.2 step 4).
#[derive(Debug, Clone)]
pub struct Curated {
    pub index_id: String,
    pub game_id: String,
    pub name: String,
    pub authors: Vec<String>,
    pub author_links: BTreeMap<String, String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub released: Option<String>,
    pub recipe: Recipe,
    /// Instead of `recipe` on these game versions.
    pub version_recipes: VersionRecipes,
    /// Folders in the mod's root whose files also go into the root, as the
    /// readme asks the player to copy them.
    pub copy_to_root: Vec<String>,
    /// Catalog versions it works with; empty: any.
    pub requires: Vec<String>,
    pub keenwiki_url: Option<String>,
    pub url: String,
}

/// Installs a downloaded index mod, or updates it when it's installed
/// already. Without `install_id` the best fitting game is used; when there
/// is none, the mod waits for one if `without_game`, else it's an error.
pub fn install_curated(
    paths: &AppPaths,
    db: &Db,
    archive: &Path,
    c: &Curated,
    install_id: Option<&str>,
    without_game: bool,
) -> Result<Mod> {
    let source = Source::open(archive)?;
    let mut analysis = analyze(&source, archive, Some(&c.game_id))?;
    if analysis.info.game_id != c.game_id {
        return Err(Error::UnexpectedContent(format!(
            "the download looks like a mod for {}, not {}",
            analysis.info.game_id, c.game_id
        )));
    }
    let in_root: Vec<&str> = analysis
        .files
        .iter()
        .filter(|f| split_dir(f).0 == analysis.root)
        .map(|f| split_dir(f).1)
        .collect();
    let recipes = std::iter::once(&c.recipe).chain(c.version_recipes.values());
    for file in recipes.flat_map(Recipe::files) {
        if !in_root.iter().any(|n| n.eq_ignore_ascii_case(file)) {
            return Err(Error::UnexpectedContent(format!("{file} is missing")));
        }
    }
    analysis.copies = copies_to_root(&analysis, &c.copy_to_root)?;

    let stored = Stored {
        name: c.name.clone(),
        authors: c.authors.clone(),
        author_links: c.author_links.clone(),
        version: c.version.clone(),
        description: c.description.clone(),
        released: c.released.clone(),
        recipe: c.recipe.clone(),
        version_recipes: c.version_recipes.clone(),
        requires: requires(&c.requires),
        source: ModSource::Index,
        index_id: Some(&c.index_id),
        keenwiki_url: c.keenwiki_url.as_deref(),
        source_url: &c.url,
        archive_name: c.url.rsplit('/').next().filter(|n| !n.is_empty()),
    };
    let game =
        catalog::game(&c.game_id).ok_or_else(|| Error::NotFound(format!("game {}", c.game_id)))?;
    let requires = stored.requires.as_deref();
    if let Some(mod_id) = find_by_index(db, &c.index_id)? {
        let install_id: Option<String> = db.conn().query_row(
            "SELECT install_id FROM profiles WHERE mod_id = ?1",
            [&mod_id],
            |r| r.get(0),
        )?;
        // A mod still waiting for its game has nothing to fit yet.
        let target = match install_id {
            Some(id) => Some(
                library::list(db)?
                    .into_iter()
                    .find(|i| i.id == id && fits(game, requires, i))
                    .ok_or_else(|| {
                        Error::IncompatibleInstall(format!(
                            "the new version needs {}",
                            needed_game(game, requires)
                        ))
                    })?,
            ),
            None => None,
        };
        return update(
            paths,
            db,
            &mod_id,
            &source,
            &analysis,
            &stored,
            target.as_ref(),
        );
    }

    let target = match install_id {
        Some(id) => Some(
            library::list(db)?
                .into_iter()
                .find(|i| i.id == id && fits(game, requires, i))
                .ok_or_else(|| Error::IncompatibleInstall(format!("{id} can't play {}", c.name)))?,
        ),
        None => match best_install(db, game, requires)? {
            None if !without_game => {
                return Err(Error::MissingGame(needed_game(game, requires)));
            }
            target => target,
        },
    };
    store(paths, db, &source, &analysis, &stored, target.as_ref())
}

/// The installed mod made from an index entry.
pub fn find_by_index(db: &Db, index_id: &str) -> Result<Option<String>> {
    Ok(db
        .conn()
        .query_row(
            "SELECT id FROM mods WHERE index_id = ?1 ORDER BY added_at LIMIT 1",
            [index_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// An index entry's `requires` as stored: empty means any version.
fn requires(versions: &[String]) -> Option<Vec<String>> {
    (!versions.is_empty()).then(|| versions.to_vec())
}

/// Whether a mod that plays on the `requires` versions (`None`: any) can be
/// played on `install`. Like [`check_install`], unknown versions get the
/// benefit of the doubt.
pub fn fits(game: &Game, requires: Option<&[String]>, install: &Install) -> bool {
    install.game_id == game.id
        && match (requires, &install.version_id) {
            (None, _) | (_, None) => true,
            (Some(r), Some(v)) => fits_version(game, r, v),
        }
}

fn version<'a>(game: &'a Game, version_id: &str) -> Option<&'a GameVersion> {
    game.versions.iter().find(|v| v.id == version_id)
}

/// `version_id` is one of `requires`, or a build of one.
fn fits_version(game: &Game, requires: &[String], version_id: &str) -> bool {
    requires.iter().any(|r| r == version_id)
        || version(game, version_id)
            .and_then(|v| v.original.as_ref())
            .is_some_and(|o| requires.contains(o))
}

/// Lower is better, among installs a mod [`fits`]: a release, a
/// third-party build, an unknown version; then the game's preferred
/// versions. `bestFit` in catalog.ts mirrors it.
fn fit_rank(game: &Game, install: &Install) -> (u8, usize) {
    match &install.version_id {
        Some(v) => (
            u8::from(version(game, v).is_some_and(GameVersion::modified)),
            game.preference(v),
        ),
        None => (2, 0),
    }
}

/// The install a mod fits best, if any; on a tie the one added first.
fn best_install(db: &Db, game: &Game, requires: Option<&[String]>) -> Result<Option<Install>> {
    Ok(library::list(db)?
        .into_iter()
        .filter(|i| fits(game, requires, i))
        .min_by_key(|i| fit_rank(game, i)))
}

/// e.g. `Commander Keen 4: Secret of the Oracle (v1.4 EGA)`.
pub fn needed_game(game: &Game, requires: Option<&[String]>) -> String {
    let labels: Vec<&str> = game
        .versions
        .iter()
        .filter(|v| requires.is_some_and(|r| r.contains(&v.id)))
        .map(|v| v.label.as_str())
        .collect();
    if labels.is_empty() {
        game.title.clone()
    } else {
        format!("{} ({})", game.title, labels.join(", "))
    }
}

/// Reads `mods.requires_json`.
pub fn parse_requires(json: Option<&str>) -> Option<Vec<String>> {
    json.and_then(|j| serde_json::from_str(j).ok())
}

/// Puts mods without a game, or with `except` gone, on the install they
/// fit best; those without one wait for it. Returns the profiles that moved.
pub fn rehome(db: &Db, except: Option<&str>) -> Result<Vec<String>> {
    let waiting: Vec<(String, String, Option<String>)> = {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT p.id, m.game_id, m.requires_json
             FROM profiles p JOIN mods m ON m.id = p.mod_id
             WHERE p.install_id IS NULL OR p.install_id IS ?1",
        )?;
        stmt.query_map([except], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let installs: Vec<Install> = library::list(db)?
        .into_iter()
        .filter(|i| Some(i.id.as_str()) != except)
        .collect();
    let mut moved = Vec::new();
    for (profile_id, game_id, requires) in waiting {
        let Some(game) = catalog::game(&game_id) else {
            continue;
        };
        let requires = parse_requires(requires.as_deref());
        let target = installs
            .iter()
            .filter(|i| fits(game, requires.as_deref(), i))
            .min_by_key(|i| fit_rank(game, i));
        let target = target.map(|i| i.id.as_str());
        let changed = db.conn().execute(
            "UPDATE profiles SET install_id = ?2, composed_hash = NULL
             WHERE id = ?1 AND install_id IS NOT ?2",
            params![profile_id, target],
        )?;
        if changed > 0 {
            tracing::info!("mod profile {profile_id} now plays on {target:?}");
            moved.push(profile_id);
        }
    }
    Ok(moved)
}

/// Gives mods whose patch names versions the catalog didn't know the
/// catalog versions they now match, e.g. after an update taught the catalog
/// what CKPatch calls a release.
pub fn identify_requirements(paths: &AppPaths, db: &Db) -> Result<()> {
    let unknown: Vec<(String, String, String)> = {
        let conn = db.conn();
        let mut stmt =
            conn.prepare("SELECT id, game_id, recipe_json FROM mods WHERE requires_json = '[]'")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut found = false;
    for (mod_id, game_id, recipe) in unknown {
        let Some(game) = catalog::game(&game_id) else {
            continue;
        };
        let Some(text) = serde_json::from_str::<Recipe>(&recipe)
            .ok()
            .and_then(|r| r.patch_file().map(str::to_string))
            .and_then(|p| fs::read(paths.mods.join(&mod_id).join("files").join(p)).ok())
        else {
            continue;
        };
        let versions = recipe::patch_versions(&String::from_utf8_lossy(&text));
        let ids = version_ids(game, &versions);
        if ids.is_empty() {
            continue;
        }
        db.conn().execute(
            "UPDATE mods SET requires_json = ?2 WHERE id = ?1",
            params![
                mod_id,
                serde_json::to_string(&ids).expect("strings serialize")
            ],
        )?;
        tracing::info!("{mod_id} plays on {ids:?}");
        found = true;
    }
    if found {
        rehome(db, None)?;
    }
    Ok(())
}

/// The installs `mod_id` can be played on, in library order.
pub fn fitting_installs(db: &Db, mod_id: &str) -> Result<Vec<Install>> {
    let (game_id, requires): (String, Option<String>) = db
        .conn()
        .query_row(
            "SELECT game_id, requires_json FROM mods WHERE id = ?1",
            [mod_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))?;
    let Some(game) = catalog::game(&game_id) else {
        return Ok(Vec::new());
    };
    let requires = parse_requires(requires.as_deref());
    Ok(library::list(db)?
        .into_iter()
        .filter(|i| fits(game, requires.as_deref(), i))
        .collect())
}

/// Plays `mod_id` on `install_id` from now on. Saves stay with the mod;
/// the composed layer is rebuilt on the next launch.
pub fn set_install(db: &Db, mod_id: &str, install_id: &str) -> Result<()> {
    if !fitting_installs(db, mod_id)?
        .iter()
        .any(|i| i.id == install_id)
    {
        return Err(Error::IncompatibleInstall(format!(
            "{mod_id} can't be played on {install_id}"
        )));
    }
    db.conn().execute(
        "UPDATE profiles SET install_id = ?2, composed_hash = NULL
         WHERE mod_id = ?1 AND install_id IS NOT ?2",
        params![mod_id, install_id],
    )?;
    tracing::info!("mod {mod_id} now plays on {install_id}");
    Ok(())
}

/// Copies the mod's files (and the archive) into `staging`.
fn stage(
    source: &Source,
    analysis: &Analysis,
    staging: &Path,
    archive_name: Option<&str>,
) -> Result<()> {
    let _ = fs::remove_dir_all(staging);
    fs::create_dir_all(staging.join("files"))?;
    for rel in &analysis.files {
        let inner = if analysis.root.is_empty() {
            rel.as_str()
        } else {
            &rel[analysis.root.len() + 1..]
        };
        source.copy_to(rel, &staging.join("files").join(inner))?;
    }
    for (rel, inner) in &analysis.copies {
        source.copy_to(rel, &staging.join("files").join(inner))?;
    }
    if let Source::Zip(zip) = source {
        fs::create_dir_all(staging.join("source"))?;
        let name = archive_name
            .map(std::ffi::OsStr::new)
            .unwrap_or_else(|| zip.file_name().expect("a file"));
        fs::copy(zip, staging.join("source").join(name))?;
    }
    Ok(())
}

fn archive_sha256(source: &Source) -> Result<Option<String>> {
    Ok(match source {
        Source::Zip(zip) => Some(sha256_hex(&fs::read(zip)?)),
        Source::Folder(_) => None,
    })
}

fn version_recipes_json(s: &Stored) -> Option<String> {
    (!s.version_recipes.is_empty())
        .then(|| serde_json::to_string(&s.version_recipes).expect("recipes serialize"))
}

/// Where the files of `folders` (in the mod's root) go when also copied into
/// the root: source path → path in the mod.
fn copies_to_root(analysis: &Analysis, folders: &[String]) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for folder in folders {
        let dir = if analysis.root.is_empty() {
            folder.clone()
        } else {
            format!("{}/{folder}", analysis.root)
        };
        let before = out.len();
        out.extend(
            analysis
                .files
                .iter()
                .filter(|f| split_dir(f).0.eq_ignore_ascii_case(&dir))
                .map(|f| (f.clone(), split_dir(f).1.to_string())),
        );
        if out.len() == before {
            return Err(Error::UnexpectedContent(format!(
                "folder {folder} is missing"
            )));
        }
    }
    Ok(out)
}

fn requires_json(s: &Stored) -> Option<String> {
    s.requires
        .as_ref()
        .map(|r| serde_json::to_string(r).expect("strings serialize"))
}

/// Stores a new mod with its play profile on `target`, or waiting for a game.
fn store(
    paths: &AppPaths,
    db: &Db,
    source: &Source,
    analysis: &Analysis,
    s: &Stored,
    target: Option<&Install>,
) -> Result<Mod> {
    let id = reserve_id(paths, &s.name)?;
    let mod_dir = paths.mods.join(&id);
    let profile_dir = paths.profiles.join(&id);
    let staging = paths.cache.join(format!("mod-{id}"));

    let result = (|| {
        stage(source, analysis, &staging, s.archive_name)?;
        fs::create_dir_all(profile_dir.join("state"))?;
        for entry in fs::read_dir(&staging)? {
            let entry = entry?;
            fs::rename(entry.path(), mod_dir.join(entry.file_name()))?;
        }

        let m = Mod {
            id: id.clone(),
            game_id: analysis.info.game_id.clone(),
            name: s.name.clone(),
            authors: s.authors.clone(),
            author_links: s.author_links.clone(),
            version: s.version.clone(),
            description: s.description.clone(),
            released: s.released.clone(),
            recipe: s.recipe.clone(),
            source: s.source,
            index_id: s.index_id.map(str::to_string),
            keenwiki_url: s.keenwiki_url.map(str::to_string),
            added_at: now(),
            profile_id: id.clone(),
        };
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO mods (id, game_id, index_id, keenwiki_url, name, authors, version,
                               recipe_json, path, source, source_url, archive_sha256, added_at,
                               description, released, author_links, requires_json,
                               version_recipes_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18)",
            params![
                m.id,
                m.game_id,
                s.index_id,
                s.keenwiki_url,
                m.name,
                serde_json::to_string(&m.authors).expect("strings serialize"),
                m.version,
                serde_json::to_string(&m.recipe).expect("recipes serialize"),
                mod_dir.display().to_string(),
                m.source.as_str(),
                s.source_url,
                archive_sha256(source)?,
                m.added_at,
                m.description,
                m.released,
                serde_json::to_string(&m.author_links).expect("strings serialize"),
                requires_json(s),
                version_recipes_json(s)
            ],
        )?;
        tx.execute(
            "INSERT INTO profiles (id, install_id, mod_id, state_path, created_at)
             VALUES (?1, ?2, ?1, ?3, ?4)",
            params![
                m.id,
                target.map(|t| &t.id),
                profile_dir.join("state").display().to_string(),
                m.added_at
            ],
        )?;
        tx.commit()?;
        Ok(m)
    })();

    let _ = fs::remove_dir_all(&staging);
    match &result {
        Ok(m) => tracing::info!(
            "installed mod {} for {} on {:?}: {} files, recipe {:?}",
            m.id,
            m.game_id,
            target.map(|t| &t.id),
            analysis.files.len(),
            m.recipe
        ),
        Err(err) => {
            tracing::warn!("install of mod {id} failed, cleaning up: {err}");
            let _ = fs::remove_dir_all(&mod_dir);
            let _ = fs::remove_dir_all(&profile_dir);
        }
    }
    result
}

/// Replaces an installed mod's files with another version and plays it on
/// `target`, which the caller checked. Saves stay untouched; the composed
/// layer is rebuilt on the next launch.
fn update(
    paths: &AppPaths,
    db: &Db,
    mod_id: &str,
    source: &Source,
    analysis: &Analysis,
    s: &Stored,
    target: Option<&Install>,
) -> Result<Mod> {
    let staging = paths.cache.join(format!("mod-{mod_id}-update"));
    let mod_dir = paths.mods.join(mod_id);
    let result: Result<()> = (|| {
        stage(source, analysis, &staging, s.archive_name)?;
        for part in ["files", "source"] {
            let live = mod_dir.join(part);
            let old = staging.join(format!("old-{part}"));
            let new = staging.join(part);
            if live.exists() {
                fs::rename(&live, &old)?;
            }
            if new.exists()
                && let Err(e) = fs::rename(&new, &live)
            {
                let _ = fs::rename(&old, &live);
                return Err(e.into());
            }
        }
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE mods SET authors = ?2, version = ?3, recipe_json = ?4, keenwiki_url = ?5,
                             source_url = ?6, archive_sha256 = ?7, description = ?8,
                             released = ?9, author_links = ?10, requires_json = ?11,
                             name = ?12, source = ?13, index_id = ?14,
                             version_recipes_json = ?15
             WHERE id = ?1",
            params![
                mod_id,
                serde_json::to_string(&s.authors).expect("strings serialize"),
                s.version,
                serde_json::to_string(&s.recipe).expect("recipes serialize"),
                s.keenwiki_url,
                s.source_url,
                archive_sha256(source)?,
                s.description,
                s.released,
                serde_json::to_string(&s.author_links).expect("strings serialize"),
                requires_json(s),
                s.name,
                s.source.as_str(),
                s.index_id,
                version_recipes_json(s)
            ],
        )?;
        tx.execute(
            "UPDATE profiles SET install_id = ?2, composed_hash = NULL WHERE mod_id = ?1",
            params![mod_id, target.map(|t| &t.id)],
        )?;
        tx.commit()?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    result?;
    tracing::info!("updated mod {mod_id} to {:?}", s.version);
    if target.is_none() {
        rehome(db, None)?;
    }
    list(db)?
        .into_iter()
        .find(|m| m.id == mod_id)
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))
}

/// Whether the mod can be played on `install`. Installs of an unknown
/// version are allowed: the catalog doesn't know every release. A known
/// version the patch file doesn't name is refused, even when the catalog
/// doesn't know the named ones (e.g. a CGA-only patch on the EGA release).
fn check_install(game_id: &str, option: &RecipeOption, install: &Install) -> Result<()> {
    if install.game_id != game_id {
        return Err(Error::IncompatibleInstall(format!(
            "the mod is for {game_id}, not {}",
            install.game_id
        )));
    }
    if let Some(version) = &install.version_id
        && !option.game_versions.is_empty()
        && !option.version_ids.contains(version)
    {
        return Err(Error::IncompatibleInstall(format!(
            "the mod needs version {}, not {version}",
            option.game_versions.join(", ")
        )));
    }
    Ok(())
}

/// A readable, unique id derived from the name, claimed by creating the
/// mod's directory. Never one that is also a profile or install id.
fn reserve_id(paths: &AppPaths, name: &str) -> Result<String> {
    let base = slug(name);
    for n in 1.. {
        let id = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        if paths.profiles.join(&id).exists() || paths.library.join(&id).exists() {
            continue;
        }
        match fs::create_dir(paths.mods.join(&id)) {
            Ok(()) => return Ok(id),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!()
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(40).collect();
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "mod".into()
    } else {
        out.into()
    }
}

pub fn list(db: &Db) -> Result<Vec<Mod>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT m.id, m.game_id, m.name, m.authors, m.version, m.recipe_json, m.source,
                m.added_at, p.id, m.index_id, m.keenwiki_url, m.description, m.released,
                m.author_links, m.version_recipes_json, i.version_id
         FROM mods m JOIN profiles p ON p.mod_id = m.id
         LEFT JOIN installs i ON i.id = p.install_id
         ORDER BY m.added_at, m.id",
    )?;
    let rows = stmt.query_map([], |r| {
        let authors: String = r.get(3)?;
        let recipe: String = r.get(5)?;
        let source: String = r.get(6)?;
        let game_id: String = r.get(1)?;
        let recipe = serde_json::from_str(&recipe).unwrap_or(Recipe::Episode);
        let by_version: VersionRecipes = r
            .get::<_, Option<String>>(14)?
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        let version_id: Option<String> = r.get(15)?;
        // The one it starts with on its game.
        let recipe =
            recipe::for_version(&recipe, &by_version, &game_id, version_id.as_deref()).clone();
        Ok(Mod {
            id: r.get(0)?,
            game_id,
            name: r.get(2)?,
            authors: serde_json::from_str(&authors).unwrap_or_default(),
            author_links: r
                .get::<_, Option<String>>(13)?
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default(),
            version: r.get(4)?,
            recipe,
            source: ModSource::parse(&source).expect("valid mod source"),
            added_at: r.get(7)?,
            profile_id: r.get(8)?,
            index_id: r.get(9)?,
            keenwiki_url: r.get(10)?,
            description: r.get(11)?,
            released: r.get(12)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Where an installed mod came from: its KeenWiki page, the download (or
/// picked file) and its version.
pub fn origin(db: &Db, mod_id: &str) -> Result<(Option<String>, String, Option<String>)> {
    db.conn()
        .query_row(
            "SELECT keenwiki_url, source_url, version FROM mods WHERE id = ?1",
            [mod_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))
}

/// A link to a prefilled GitHub issue suggesting a KeenWiki mod for the
/// index (§9.3), with a draft entry built from what was installed.
pub fn index_suggestion(db: &Db, mod_id: &str) -> Result<String> {
    let m = list(db)?
        .into_iter()
        .find(|m| m.id == mod_id)
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))?;
    let (source_url, sha256, path): (Option<String>, Option<String>, String) =
        db.conn().query_row(
            "SELECT source_url, archive_sha256, path FROM mods WHERE id = ?1",
            [mod_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
    let Mod {
        name,
        game_id,
        authors,
        author_links,
        version,
        recipe,
        keenwiki_url,
        ..
    } = m;
    let size = fs::read_dir(Path::new(&path).join("source"))
        .ok()
        .and_then(|mut d| d.next())
        .and_then(|e| e.ok()?.metadata().ok())
        .map(|m| m.len());

    // JSON scalars and arrays are valid YAML.
    fn json(v: &impl Serialize) -> String {
        serde_json::to_string(v).expect("plain values serialize")
    }
    let recipe = serde_json::to_value(&recipe).expect("recipes serialize");
    let mut yaml = format!(
        "id: {}\nname: {}\ngame: {game_id}\nrequires: []\nauthors: {}\n",
        slug(&name),
        json(&name),
        json(&authors)
    );
    if !author_links.is_empty() {
        yaml += "authorLinks:\n";
        for (name, url) in &author_links {
            yaml += &format!("  {}: {url}\n", json(name));
        }
    }
    if let Some(v) = &version {
        yaml += &format!("version: {}\n", json(v));
    }
    yaml += "type: episode  # episode | levelpack | conversion\nlinks:\n";
    if let Some(url) = &keenwiki_url {
        yaml += &format!("  keenwiki: {url}\n");
    }
    yaml += "downloads:\n";
    yaml += &format!(
        "  - url: {}\n    sha256: {}\n",
        source_url.as_deref().unwrap_or(""),
        sha256.as_deref().unwrap_or("")
    );
    if let Some(v) = &version {
        yaml += &format!("    version: {}\n", json(v));
    }
    if let Some(size) = size {
        yaml += &format!("    size: {size}\n");
    }
    yaml += "recipe:\n";
    if let serde_json::Value::Object(fields) = &recipe {
        for (key, value) in fields {
            yaml += &format!("  {key}: {value}\n");
        }
    }

    let body = format!(
        "BloogBase installed this mod from KeenWiki and it could go into the index.\n\n\
         Page: {}\n\nDraft entry (please check the recipe, `requires`, `type` and the dates):\n\n\
         ```yaml\n{yaml}```\n",
        keenwiki_url.as_deref().unwrap_or("-"),
    );
    let url = reqwest::Url::parse_with_params(
        concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new"),
        [
            ("title", format!("Index suggestion: {name}")),
            ("body", body),
        ],
    )
    .expect("valid URL");
    Ok(url.to_string())
}

/// Removes a mod with its profile. With `keep_saves`, what the game wrote is
/// archived into `backups/` first (§9.5).
pub fn delete(paths: &AppPaths, db: &Db, mod_id: &str, keep_saves: bool) -> Result<()> {
    let profile_id: String = db
        .conn()
        .query_row("SELECT id FROM profiles WHERE mod_id = ?1", [mod_id], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))?;
    let backup = if keep_saves {
        backups::create(paths, db, &profile_id, Contents::State, Reason::ModRemoved)?
    } else {
        None
    };
    {
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM profiles WHERE id = ?1", [&profile_id])?;
        tx.execute("DELETE FROM mods WHERE id = ?1", [mod_id])?;
        tx.commit()?;
    }
    for dir in [
        paths.mods.join(mod_id),
        paths.profiles.join(&profile_id),
        paths.composed(&profile_id),
    ] {
        if let Err(e) = fs::remove_dir_all(&dir)
            && e.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!("could not remove {}: {e}", dir.display());
        }
    }
    crate::covers::remove(paths, &profile_id);
    tracing::info!("removed mod {mod_id} (saves kept: {})", backup.is_some());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles;
    use crate::source::testutil::*;

    fn setup() -> (tempfile::TempDir, AppPaths, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        let db = Db::open_in_memory().unwrap();
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        library::import(&paths, &db, &src).unwrap();
        (tmp, paths, db)
    }

    const AURORA: &[(&str, &[u8])] = &[
        (
            "Aurora/play.bat",
            b"@echo off\r\nCK4PATCH.EXE abiathar.pat\r\n",
        ),
        ("Aurora/CK4PATCH.EXE", b"patcher"),
        ("Aurora/abiathar.pat", b"%ext ck4\n%version 1.4\n"),
        ("Aurora/GAMEMAPS.CK4", b"new maps"),
        ("Aurora/READ ME.txt", b"hi"),
        ("Aurora/Thumbs.db", b""),
        ("__MACOSX/Aurora/._play.bat", b""),
        ("info.txt", b"outside the mod"),
    ];

    fn request(path: &Path, recipe: Recipe) -> InstallModRequest {
        InstallModRequest {
            path: path.display().to_string(),
            name: String::new(),
            recipe,
            install_id: Some("keen4-1".into()),
            keenwiki: None,
            replace: None,
        }
    }

    #[test]
    fn inspects_zip_with_folder_and_metadata_from_name() {
        let tmp = tempfile::tempdir().unwrap();
        let zip = tmp
            .path()
            .join("m.dave216.aurora_bliss_v.1.2.k4.2023-08-23.zip");
        write_zip(&zip, AURORA);
        let info = inspect(&zip, None).unwrap();
        assert_eq!(info.name, "Aurora Bliss");
        assert_eq!(info.authors, ["dave216"]);
        assert_eq!(info.version.as_deref(), Some("1.2"));
        assert_eq!(info.released.as_deref(), Some("2023-08-23"));
        assert_eq!(info.game_id, "keen4");
        assert!(info.confident);
        assert_eq!(info.file_count, 5);
        assert_eq!(
            info.recipes[0],
            RecipeOption {
                recipe: Recipe::Patch {
                    patcher: "CK4PATCH.EXE".into(),
                    patch: "abiathar.pat".into(),
                    args: vec![]
                },
                origin: "play.bat".into(),
                game_versions: vec!["1.4".into()],
                version_ids: vec!["keen4-1.4-ega".into()],
            }
        );
    }

    #[test]
    fn installs_mod_with_profile_and_removes_it_keeping_saves() {
        let (tmp, paths, db) = setup();
        let zip = tmp.path().join("aurora.zip");
        write_zip(&zip, AURORA);
        let info = inspect(&zip, None).unwrap();
        assert_eq!(info.name, "Aurora");

        // The fake Keen 4 has an unknown version, which is allowed.
        let m = install(&paths, &db, &request(&zip, info.recipes[0].recipe.clone())).unwrap();
        assert_eq!((m.id.as_str(), m.profile_id.as_str()), ("aurora", "aurora"));
        let files = paths.mods.join("aurora/files");
        assert!(files.join("GAMEMAPS.CK4").is_file());
        assert!(files.join("READ ME.txt").is_file());
        assert!(!files.join("Thumbs.db").exists());
        assert!(!paths.mods.join("aurora/files/info.txt").exists());
        assert!(paths.mods.join("aurora/source/aurora.zip").is_file());
        assert!(paths.profiles.join("aurora/state").is_dir());
        assert_eq!(list(&db).unwrap().len(), 1);
        assert_eq!(fs::read_dir(&paths.cache).unwrap().count(), 0);

        // Same name again gets its own id.
        let second = install(&paths, &db, &request(&zip, info.recipes[0].recipe.clone())).unwrap();
        assert_eq!(second.id, "aurora-2");

        fs::write(paths.profiles.join("aurora/state/SAVEGAM0.CK4"), b"save").unwrap();
        delete(&paths, &db, "aurora", true).unwrap();
        assert!(!paths.mods.join("aurora").exists());
        assert!(!paths.profiles.join("aurora").exists());
        let (archive, files): (String, String) = db
            .conn()
            .query_row(
                "SELECT archive_path, files_json FROM save_backups WHERE profile_id = 'aurora'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(files, r#"["SAVEGAM0.CK4"]"#);
        let mut backup = zip::ZipArchive::new(fs::File::open(archive).unwrap()).unwrap();
        assert_eq!(backup.by_index(0).unwrap().name().unwrap(), "SAVEGAM0.CK4");
        assert_eq!(list(&db).unwrap().len(), 1);

        delete(&paths, &db, "aurora-2", false).unwrap();
        assert!(list(&db).unwrap().is_empty());
        assert!(matches!(
            delete(&paths, &db, "aurora-2", false),
            Err(Error::NotFound(_))
        ));

        // Installed again, it gets its id back and is offered the saves.
        let again = install(&paths, &db, &request(&zip, info.recipes[0].recipe.clone())).unwrap();
        assert_eq!(again.profile_id, "aurora");
        let offer = backups::earlier(&db, "aurora").unwrap().unwrap();
        assert_eq!(offer.backup.reason, Some(backups::Reason::ModRemoved));
        assert!(backups::earlier(&db, "keen4-1").unwrap().is_none());
    }

    #[test]
    fn keenwiki_install_keeps_the_page_and_can_be_suggested() {
        let (tmp, paths, db) = setup();
        let zip = tmp.path().join("aurora.zip");
        // A patch without `%ext` and no patcher: nothing names the episode.
        write_zip(
            &zip,
            &[(
                "Aurora/MOD.PAT",
                b"%version 1.4
",
            )],
        );
        assert!(matches!(inspect(&zip, None), Err(Error::NoModFound)));
        let info = inspect(&zip, Some("keen4")).unwrap();
        assert_eq!(info.game_id, "keen4");

        let page = "https://keenwiki.shikadi.net/wiki/Aurora_Bliss";
        let download = "https://files.shikadi.net/keenwiki/mods/Keen4/aurora.zip";
        let mut req = request(&zip, info.recipes[0].recipe.clone());
        req.name = "Aurora Bliss".into();
        req.keenwiki = Some(KeenwikiOrigin {
            page_url: page.into(),
            download_url: download.into(),
            game_id: Some("keen4".into()),
            authors: vec!["Dave216".into()],
            author_links: BTreeMap::from([(
                "Dave216".into(),
                "https://keenwiki.shikadi.net/wiki/User:Dave216".into(),
            )]),
            version: Some("1.2".into()),
            description: Some("Blissful.".into()),
            released: Some("2012-05-01".into()),
        });
        let m = install(&paths, &db, &req).unwrap();
        assert_eq!(m.source, ModSource::Keenwiki);
        assert_eq!(m.authors, ["Dave216"]);
        assert_eq!(m.version.as_deref(), Some("1.2"));
        assert_eq!(m.keenwiki_url.as_deref(), Some(page));
        let listed = &list(&db).unwrap()[0];
        assert_eq!(listed.keenwiki_url.as_deref(), Some(page));
        assert_eq!(listed.description.as_deref(), Some("Blissful."));
        assert_eq!(listed.released.as_deref(), Some("2012-05-01"));
        assert_eq!(
            listed.author_links["Dave216"],
            "https://keenwiki.shikadi.net/wiki/User:Dave216"
        );

        let url = reqwest::Url::parse(&index_suggestion(&db, &m.id).unwrap()).unwrap();
        assert_eq!(
            url.as_str().split('?').next(),
            Some("https://github.com/transporthummel/BloogBase/issues/new")
        );
        let query: BTreeMap<_, _> = url.query_pairs().collect();
        assert_eq!(query["title"], "Index suggestion: Aurora Bliss");
        let body = &query["body"];
        let sha = sha256_hex(&fs::read(&zip).unwrap());
        for line in [
            "id: aurora-bliss\n",
            "name: \"Aurora Bliss\"\n",
            "game: keen4\n",
            "authors: [\"Dave216\"]\n",
            "authorLinks:\n  \"Dave216\": https://keenwiki.shikadi.net/wiki/User:Dave216\n",
            &format!("  keenwiki: {page}\n"),
            &format!("  - url: {download}\n    sha256: {sha}\n    version: \"1.2\"\n"),
            "recipe:\n  type: \"episode\"\n",
        ] {
            assert!(body.contains(line), "{line} missing in\n{body}");
        }
        assert!(matches!(
            index_suggestion(&db, "nope"),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn replaces_another_version_keeping_saves_and_page() {
        let (tmp, paths, db) = setup();
        let zip = tmp.path().join("aurora.zip");
        write_zip(&zip, AURORA);
        let recipe = inspect(&zip, None).unwrap().recipes[0].recipe.clone();
        let mut req = request(&zip, recipe.clone());
        req.keenwiki = Some(KeenwikiOrigin {
            page_url: "https://keenwiki.shikadi.net/wiki/Aurora_Bliss".into(),
            download_url: "https://files.shikadi.net/aurora.zip".into(),
            game_id: Some("keen4".into()),
            authors: vec!["Dave216".into()],
            author_links: BTreeMap::new(),
            version: Some("1.1".into()),
            description: Some("Blissful.".into()),
            released: None,
        });
        let old = install(&paths, &db, &req).unwrap();
        fs::write(paths.profiles.join("aurora/state/SAVEGAM0.CK4"), b"save").unwrap();

        let newer = tmp.path().join("m.dave216.aurora_v.1.2.k4.2023-08-23.zip");
        let mut files = AURORA.to_vec();
        files[3] = ("Aurora/GAMEMAPS.CK4", b"newer maps");
        write_zip(&newer, &files);
        let m = install(
            &paths,
            &db,
            &InstallModRequest {
                replace: Some(old.id.clone()),
                ..request(&newer, recipe.clone())
            },
        )
        .unwrap();
        assert_eq!((m.id.as_str(), m.profile_id.as_str()), ("aurora", "aurora"));
        assert_eq!(m.source, ModSource::Manual);
        assert_eq!(m.version.as_deref(), Some("1.2"));
        assert_eq!(m.description.as_deref(), Some("Blissful."));
        assert_eq!(m.keenwiki_url, old.keenwiki_url);
        assert_eq!(list(&db).unwrap().len(), 1);
        let files = paths.mods.join("aurora/files");
        assert_eq!(fs::read(files.join("GAMEMAPS.CK4")).unwrap(), b"newer maps");
        assert!(
            paths
                .mods
                .join("aurora/source")
                .join(newer.file_name().unwrap())
                .is_file()
        );
        assert!(!paths.mods.join("aurora/source/aurora.zip").exists());
        assert!(paths.profiles.join("aurora/state/SAVEGAM0.CK4").is_file());
        assert_eq!(fs::read_dir(&paths.cache).unwrap().count(), 0);

        let k5 = tmp.path().join("k5mod");
        write_folder(&k5, &[("LEVEL01.CK5", b"x")]);
        let k5_recipe = inspect(&k5, None).unwrap().recipes[0].recipe.clone();
        let other_game = InstallModRequest {
            replace: Some("aurora".into()),
            install_id: None,
            ..request(&k5, k5_recipe)
        };
        assert!(matches!(
            install(&paths, &db, &other_game),
            Err(Error::IncompatibleInstall(_))
        ));
        let missing = InstallModRequest {
            replace: Some("nope".into()),
            ..request(&newer, recipe)
        };
        assert!(matches!(
            install(&paths, &db, &missing),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn refuses_other_games_and_versions() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("k5mod");
        write_folder(&src, &[("LEVEL.CK5", b""), ("GAMEMAPS.CK5", b"")]);
        let info = inspect(&src, None).unwrap();
        assert_eq!(info.game_id, "keen5");
        let err = install(&paths, &db, &request(&src, Recipe::Episode)).unwrap_err();
        assert!(matches!(err, Error::IncompatibleInstall(_)));
        // Nothing left behind.
        assert_eq!(fs::read_dir(&paths.mods).unwrap().count(), 0);

        let option = RecipeOption {
            recipe: Recipe::Episode,
            origin: String::new(),
            game_versions: vec!["1.4".into()],
            version_ids: vec!["keen4-1.4-ega".into()],
        };
        let mut i = library::list(&db).unwrap().remove(0);
        check_install("keen4", &option, &i).unwrap();
        i.version_id = Some("keen4-1.0".into());
        assert!(check_install("keen4", &option, &i).is_err());
        i.version_id = Some("keen4-1.4-ega".into());
        check_install("keen4", &option, &i).unwrap();

        // A patch for a version the catalog doesn't know: the EGA one isn't it.
        let cga = RecipeOption {
            game_versions: vec!["cga-2.0".into()],
            version_ids: vec![],
            ..option
        };
        assert!(check_install("keen4", &cga, &i).is_err());
        i.version_id = None;
        check_install("keen4", &cga, &i).unwrap();
    }

    #[test]
    fn identifies_versions_the_catalog_learned_for_installed_mods() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("cga");
        write_folder(
            &src,
            &[
                (
                    "play.bat",
                    b"CK4PATCH.EXE KEEN4C16.PAT
",
                ),
                ("CK4PATCH.EXE", b"patcher"),
                (
                    "KEEN4C16.PAT",
                    b"%ext ck4
%version cga-1.4
",
                ),
            ],
        );
        let info = inspect(&src, None).unwrap();
        assert_eq!(info.recipes[0].version_ids, ["keen4-1.4-cga"]);
        let m = install(&paths, &db, &request(&src, info.recipes[0].recipe.clone())).unwrap();
        // As installed before the catalog knew the version, its game since
        // identified as that one.
        db.conn()
            .execute_batch(
                "UPDATE mods SET requires_json = '[]';
                 UPDATE profiles SET install_id = NULL WHERE mod_id IS NOT NULL;
                 UPDATE installs SET version_id = 'keen4-1.4-cga';",
            )
            .unwrap();
        assert!(fitting_installs(&db, &m.id).unwrap().is_empty());

        identify_requirements(&paths, &db).unwrap();
        assert_eq!(fitting_installs(&db, &m.id).unwrap()[0].id, "keen4-1");
        let install_id: Option<String> = db
            .conn()
            .query_row(
                "SELECT install_id FROM profiles WHERE mod_id = ?1",
                [&m.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(install_id.as_deref(), Some("keen4-1"));
    }

    fn curated(recipe: Recipe) -> Curated {
        Curated {
            index_id: "aurora-bliss".into(),
            game_id: "keen4".into(),
            name: "Aurora Bliss".into(),
            authors: vec!["Dave216".into()],
            author_links: BTreeMap::new(),
            version: Some("1.2".into()),
            description: Some("A Keen 4 mod.".into()),
            released: None,
            recipe,
            version_recipes: VersionRecipes::new(),
            copy_to_root: Vec::new(),
            requires: vec!["keen4-1.4-ega".into()],
            keenwiki_url: Some("https://keenwiki.shikadi.net/wiki/Aurora_Bliss".into()),
            url: "https://example.com/m.dave216.aurora.zip".into(),
        }
    }

    fn aurora_patch(args: &[&str]) -> Recipe {
        Recipe::Patch {
            patcher: "CK4PATCH.EXE".into(),
            patch: "ABIATHAR.PAT".into(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }

    #[test]
    fn curated_install_uses_the_index_recipe_and_updates_in_place() {
        let (tmp, paths, db) = setup();
        let zip = tmp.path().join("index-aurora-bliss.zip");
        write_zip(&zip, AURORA);
        let c = curated(aurora_patch(&["/nocheck"]));
        let m = install_curated(&paths, &db, &zip, &c, None, false).unwrap();
        assert_eq!(m.id, "aurora-bliss");
        assert_eq!(m.recipe, c.recipe);
        assert_eq!(m.source, ModSource::Index);
        assert_eq!(m.index_id.as_deref(), Some("aurora-bliss"));
        assert!(
            paths
                .mods
                .join("aurora-bliss/source/m.dave216.aurora.zip")
                .is_file()
        );
        let (url, wiki): (String, String) = db
            .conn()
            .query_row("SELECT source_url, keenwiki_url FROM mods", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(url, c.url);
        assert!(wiki.ends_with("Aurora_Bliss"));

        // A new version: same mod and profile, new files, saves kept.
        let state = paths.profiles.join("aurora-bliss/state/SAVEGAM0.CK4");
        fs::write(&state, b"save").unwrap();
        let mut v2: Vec<(&str, &[u8])> = AURORA.to_vec();
        v2[3] = ("Aurora/GAMEMAPS.CK4", b"better maps");
        write_zip(&zip, &v2);
        let c2 = Curated {
            version: Some("1.3".into()),
            ..c.clone()
        };
        let updated = install_curated(&paths, &db, &zip, &c2, None, false).unwrap();
        assert_eq!(updated.id, "aurora-bliss");
        assert_eq!(updated.version.as_deref(), Some("1.3"));
        assert_eq!(list(&db).unwrap().len(), 1);
        assert_eq!(
            fs::read(paths.mods.join("aurora-bliss/files/GAMEMAPS.CK4")).unwrap(),
            b"better maps"
        );
        assert_eq!(fs::read(&state).unwrap(), b"save");
        let (sha, composed): (String, Option<String>) = db
            .conn()
            .query_row(
                "SELECT m.archive_sha256, p.composed_hash FROM mods m JOIN profiles p ON p.mod_id = m.id",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(sha, sha256_hex(&fs::read(&zip).unwrap()));
        assert_eq!(composed, None);
        assert!(!paths.cache.join("mod-aurora-bliss-update").exists());
    }

    #[test]
    fn curated_install_checks_the_download_and_the_library() {
        let (tmp, paths, db) = setup();
        let zip = tmp.path().join("index-x.zip");
        write_zip(&zip, AURORA);
        let missing = curated(Recipe::Batch {
            file: "START.BAT".into(),
        });
        assert!(matches!(
            install_curated(&paths, &db, &zip, &missing, None, false),
            Err(Error::UnexpectedContent(_))
        ));
        let other_game = Curated {
            game_id: "keen5".into(),
            ..curated(aurora_patch(&[]))
        };
        assert!(matches!(
            install_curated(&paths, &db, &zip, &other_game, None, false),
            Err(Error::UnexpectedContent(_))
        ));

        let k5 = tmp.path().join("k5.zip");
        write_zip(&k5, &[("GAMEMAPS.CK5", b""), ("LEVEL.CK5", b"")]);
        let needs_k5 = Curated {
            game_id: "keen5".into(),
            requires: vec!["keen5-1.4-ega".into()],
            ..curated(Recipe::Episode)
        };
        match install_curated(&paths, &db, &k5, &needs_k5, None, false) {
            Err(Error::MissingGame(what)) => assert!(what.ends_with("(v1.4 EGA)"), "{what}"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            install_curated(
                &paths,
                &db,
                &zip,
                &curated(aurora_patch(&[])),
                Some("nope"),
                false
            ),
            Err(Error::IncompatibleInstall(_))
        ));
        assert_eq!(fs::read_dir(&paths.mods).unwrap().count(), 0);
    }

    #[test]
    fn mods_wait_for_their_game_and_move_when_it_goes() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        let db = Db::open_in_memory().unwrap();
        let zip = tmp.path().join("aurora.zip");
        write_zip(&zip, AURORA);
        let info = inspect(&zip, None).unwrap();
        let req = InstallModRequest {
            install_id: None,
            ..request(&zip, info.recipes[0].recipe.clone())
        };
        let m = install(&paths, &db, &req).unwrap();
        let (p, layers) = profiles::get(&db, &m.profile_id).unwrap();
        assert_eq!((p.install_id, p.game_id.as_str()), (None, "keen4"));
        assert_eq!(
            p.missing_game.as_deref(),
            Some("Commander Keen 4: Secret of the Oracle (v1.4 EGA)")
        );
        assert_eq!(layers.lower, None);
        let k5 = tmp.path().join("k5.zip");
        write_zip(&k5, &[("GAMEMAPS.CK5", b""), ("LEVEL.CK5", b"")]);
        let needs_k5 = Curated {
            game_id: "keen5".into(),
            requires: vec![],
            ..curated(Recipe::Episode)
        };
        let k5_mod = install_curated(&paths, &db, &k5, &needs_k5, None, true).unwrap();
        let install_of = |id: &str| profiles::get(&db, id).unwrap().0.install_id;
        assert_eq!(install_of(&k5_mod.profile_id), None);

        // The game arrives, of an unknown version, which may be the right one.
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        library::import(&paths, &db, &src).unwrap();
        assert_eq!(install_of("aurora").as_deref(), Some("keen4-1"));
        assert_eq!(profiles::get(&db, "aurora").unwrap().0.missing_game, None);
        assert_eq!(install_of(&k5_mod.profile_id), None);

        // Removing it moves the mod to another release, else it waits again.
        let other = tmp.path().join("k4b");
        write_folder(&other, KEEN4);
        fs::write(other.join("KEEN4E.EXE"), b"other").unwrap();
        library::import(&paths, &db, &other).unwrap();
        library::remove(&paths, &db, "keen4-1", false, false).unwrap();
        assert_eq!(install_of("aurora").as_deref(), Some("keen4-2"));
        library::remove(&paths, &db, "keen4-2", false, false).unwrap();
        assert_eq!(install_of("aurora"), None);
        assert!(paths.mods.join("aurora/files/GAMEMAPS.CK4").is_file());
        assert_eq!(list(&db).unwrap().len(), 2);
    }

    #[test]
    fn mods_move_to_another_fitting_copy() {
        let (tmp, paths, db) = setup();
        let other = tmp.path().join("k4b");
        write_folder(&other, KEEN4);
        fs::write(other.join("KEEN4E.EXE"), b"other").unwrap();
        library::import(&paths, &db, &other).unwrap();
        let zip = tmp.path().join("aurora.zip");
        write_zip(&zip, AURORA);
        let info = inspect(&zip, None).unwrap();
        let m = install(&paths, &db, &request(&zip, info.recipes[0].recipe.clone())).unwrap();
        let ids = |v: Vec<Install>| v.into_iter().map(|i| i.id).collect::<Vec<_>>();
        assert_eq!(
            ids(fitting_installs(&db, &m.id).unwrap()),
            ["keen4-1", "keen4-2"]
        );

        set_install(&db, &m.id, "keen4-2").unwrap();
        let p = profiles::get(&db, &m.profile_id).unwrap().0;
        assert_eq!(p.install_id.as_deref(), Some("keen4-2"));
        assert!(matches!(
            set_install(&db, &m.id, "keen5-1"),
            Err(Error::IncompatibleInstall(_))
        ));
        assert_eq!(
            profiles::get(&db, &m.profile_id)
                .unwrap()
                .0
                .install_id
                .as_deref(),
            Some("keen4-2")
        );
        assert!(set_install(&db, "nope", "keen4-1").is_err());
    }

    #[test]
    fn a_known_version_the_mod_doesnt_name_isnt_used() {
        let k6 = catalog::game("keen6").unwrap();
        let install = Install {
            id: "keen6-1".into(),
            game_id: "keen6".into(),
            version_id: Some("keen6-1.4-ega".into()),
            label: String::new(),
            source: library::InstallSource::Folder,
            path: String::new(),
            added_at: 0,
        };
        let requires = parse_requires(Some(r#"["keen6-1.0"]"#));
        assert!(!fits(k6, requires.as_deref(), &install));
        assert!(fits(k6, parse_requires(None).as_deref(), &install));
        assert_eq!(
            needed_game(k6, requires.as_deref()),
            format!("{} (v1.0 EGA)", k6.title)
        );
    }

    #[test]
    fn curated_versions_and_ranking() {
        let k6 = catalog::game("keen6").unwrap();
        let install = |v: Option<&str>| Install {
            id: "i".into(),
            game_id: "keen6".into(),
            version_id: v.map(str::to_string),
            label: String::new(),
            source: library::InstallSource::Folder,
            path: String::new(),
            added_at: 0,
        };
        let v10 = vec!["keen6-1.0".to_string()];
        let v14 = vec!["keen6-1.4-ega".to_string()];
        assert!(fits(k6, Some(&v10), &install(Some("keen6-1.0-nocp"))));
        assert!(!fits(k6, Some(&v14), &install(Some("keen6-1.0-nocp"))));
        assert!(fits(k6, Some(&v14), &install(None)));
        assert!(fits(k6, None, &install(Some("keen6-1.0"))));
        // A patch naming versions the catalog doesn't know.
        assert!(!fits(k6, Some(&[]), &install(Some("keen6-1.0"))));
        assert!(!fits(catalog::game("keen4").unwrap(), None, &install(None)));
        let rank = |v| fit_rank(k6, &install(v));
        assert!(rank(Some("keen6-1.0")) < rank(Some("keen6-1.0-nocp")));
        assert!(rank(Some("keen6-1.0-nocp")) < rank(None));
        // Releases by the game's preferred versions, builds after all of them.
        assert!(rank(Some("keen6-1.4-ega")) < rank(Some("keen6-1.5-ega")));
        assert!(rank(Some("keen6-1.5-ega")) < rank(Some("keen6-1.0")));
        assert!(rank(Some("keen6-demo")) < rank(Some("keen6-1.5-ega-nocp")));
        assert!(rank(Some("keen6-1.5-ega-nocp")) < rank(Some("keen6-1.0-nocp")));
        let k1 = catalog::game("keen1").unwrap();
        let k1_rank = |v: &str| {
            let i = Install {
                game_id: "keen1".into(),
                ..install(Some(v))
            };
            fit_rank(k1, &i)
        };
        assert!(k1_rank("keen1-1.1") < k1_rank("keen1-beta-dec"));
        // Same game files as the original: as good as it.
        let kdreams = catalog::game("kdreams").unwrap();
        let v193 = vec!["kdreams-1.93".to_string()];
        let anthology = Install {
            game_id: "kdreams".into(),
            ..install(Some("kdreams-1.93-anthology"))
        };
        assert!(fits(kdreams, Some(&v193), &anthology));
        assert_eq!(fit_rank(kdreams, &anthology), (0, 0));
    }

    #[test]
    fn unknown_recipe_is_rejected() {
        let (tmp, paths, db) = setup();
        let zip = tmp.path().join("aurora.zip");
        write_zip(&zip, AURORA);
        let bad = Recipe::Batch {
            file: "EVIL.BAT".into(),
        };
        assert!(install(&paths, &db, &request(&zip, bad)).is_err());
    }

    #[test]
    fn nothing_moddy() {
        let tmp = tempfile::tempdir().unwrap();
        write_folder(tmp.path(), &[("README.TXT", b"")]);
        assert!(matches!(inspect(tmp.path(), None), Err(Error::NoModFound)));
    }

    #[test]
    fn dropped_things_are_told_apart() {
        let tmp = tempfile::tempdir().unwrap();
        let zip = tmp.path().join("aurora.zip");
        write_zip(&zip, AURORA);
        let Dropped::Mod { info: patched } = classify(&zip).unwrap() else {
            panic!("not a mod");
        };
        // A game of a version the catalog doesn't know is still a game…
        let game = tmp.path().join("k4");
        write_folder(&game, KEEN4);
        assert!(matches!(classify(&game).unwrap(), Dropped::Games));
        // …but the game's files with a patch are a mod.
        let modded = tmp.path().join("k4mod");
        write_folder(&modded, KEEN4);
        write_folder(
            &modded,
            &[
                ("CK4PATCH.EXE", b"patcher"),
                ("MOD.PAT", b"%ext ck4\n%version 1.4\n"),
            ],
        );
        assert!(matches!(classify(&modded).unwrap(), Dropped::Mod { .. }));
        let empty = tmp.path().join("empty");
        write_folder(&empty, &[("README.TXT", b"")]);
        assert!(matches!(classify(&empty).unwrap(), Dropped::Nothing));
        let text = tmp.path().join("notes.txt");
        fs::write(&text, "").unwrap();
        assert!(matches!(classify(&text), Err(Error::UnsupportedSource(_))));

        let with = |recipe| ModInfo {
            recipes: vec![RecipeOption {
                recipe,
                origin: "x".into(),
                game_versions: vec![],
                version_ids: vec![],
            }],
            ..patched.clone()
        };
        let batch = with(Recipe::Batch {
            file: "PLAY.BAT".into(),
        });
        assert!(matches!(decide(true, Some(batch)), Dropped::Mod { .. }));
        assert!(matches!(
            decide(true, Some(with(Recipe::Episode))),
            Dropped::Games
        ));
        assert!(matches!(decide(true, None), Dropped::Games));
        assert!(matches!(
            decide(false, Some(with(Recipe::Episode))),
            Dropped::Mod { .. }
        ));
        assert!(matches!(decide(false, None), Dropped::Nothing));
    }

    #[test]
    fn leaves_out_keen_1_3_settings() {
        let tmp = tempfile::tempdir().unwrap();
        write_folder(
            tmp.path(),
            &[
                ("Ctlpanel.ck1", b"keys"),
                ("LEVEL01.CK1", b""),
                ("KEEN1.EXE", b"modded"),
                ("EXTRA/CTLPANEL.CK1", b"keys"),
            ],
        );
        let a = analyze(&Source::open(tmp.path()).unwrap(), tmp.path(), None).unwrap();
        assert_eq!(a.info.game_id, "keen1");
        assert!(
            !a.files
                .iter()
                .any(|f| f.eq_ignore_ascii_case("CTLPANEL.CK1"))
        );
        assert!(a.files.contains(&"LEVEL01.CK1".to_string()));
        assert!(a.files.contains(&"EXTRA/CTLPANEL.CK1".to_string()));
    }

    #[test]
    fn patch_files_without_a_patcher_use_the_bundled_one() {
        // As Robot Rebellion: its menu runs `ck1patch ckrr.pat`, which the
        // player was to have already.
        let tmp = tempfile::tempdir().unwrap();
        write_folder(
            tmp.path(),
            &[
                ("ROBOKEEN.EXE", b"MZ"),
                (
                    "ckrr.pat",
                    b"%ext ck1\n%patchfile 0x12087 CKSCREEN.CK1\n%end\n",
                ),
                ("LEVEL01.CK1", b""),
            ],
        );
        let a = analyze(&Source::open(tmp.path()).unwrap(), tmp.path(), None).unwrap();
        assert_eq!(a.info.game_id, "keen1");
        let recipes: Vec<&Recipe> = a.info.recipes.iter().map(|o| &o.recipe).collect();
        let program = Recipe::Program {
            program: "ROBOKEEN.EXE".into(),
            args: Vec::new(),
        };
        let patch = Recipe::Patch {
            patcher: "CK1PATCH.EXE".into(),
            patch: "ckrr.pat".into(),
            args: Vec::new(),
        };
        assert_eq!(recipes, [&program, &patch]);
        // The patcher isn't one of the mod's files.
        assert!(
            !a.files
                .iter()
                .any(|f| f.eq_ignore_ascii_case("CK1PATCH.EXE"))
        );
    }

    #[test]
    fn keeps_unknown_executables() {
        let tmp = tempfile::tempdir().unwrap();
        write_folder(
            tmp.path(),
            &[
                ("KDREAMS.EXE", b"modded"),
                ("KDREAMS.MAP", b""),
                ("KDRPATCH.EXE", b""),
            ],
        );
        let a = analyze(&Source::open(tmp.path()).unwrap(), tmp.path(), None).unwrap();
        assert!(a.files.contains(&"KDREAMS.EXE".to_string()));
        assert_eq!(a.info.game_id, "kdreams");
    }

    #[test]
    fn renamed_patcher_and_patch_files_are_found() {
        // As Norp the Yorp 3: CK3PATCH as NORPER.EXE, one `.NY3` patch per
        // game version, each run by its own batch file.
        let tmp = tempfile::tempdir().unwrap();
        let norper = [b"MZ".as_slice(), &[0; 0x1E], b"Usage: CK3PATCH {patchfile}"].concat();
        let patch = b"#Text Files
%patchfile 0x17b00 DRAGON.NY3
%patch 0x1ba4c \"N3MAP\"
";
        write_folder(
            tmp.path(),
            &[
                ("NORPER.EXE", &norper),
                ("MAGMA.NY3", patch),
                ("PLASMA.NY3", patch),
                ("NORP3v131.bat", b"norper magma.ny3"),
                ("NORP3v10.bat", b"norper plasma.ny3"),
                ("N3MAP01.NY3", b""),
            ],
        );
        let a = analyze(&Source::open(tmp.path()).unwrap(), tmp.path(), None).unwrap();
        assert_eq!(a.info.game_id, "keen3");
        let recipes: Vec<&Recipe> = a.info.recipes.iter().map(|o| &o.recipe).collect();
        let patch = |p: &str| Recipe::Patch {
            patcher: "NORPER.EXE".into(),
            patch: p.into(),
            args: Vec::new(),
        };
        assert_eq!(recipes, [&patch("PLASMA.NY3"), &patch("MAGMA.NY3")]);
        assert!(!a.info.confident);
    }

    #[test]
    fn curated_version_recipes_and_copies_to_root() {
        let (tmp, paths, db) = setup();
        let zip = tmp.path().join("aurora.zip");
        let mut files: Vec<(&str, &[u8])> = AURORA.to_vec();
        files.push(("Aurora/other.pat", b"%ext ck4\n"));
        files.push(("Aurora/1/CREATURE.CK4", b"big sprites"));
        write_zip(&zip, &files);
        let other = Recipe::Patch {
            patcher: "CK4PATCH.EXE".into(),
            patch: "other.pat".into(),
            args: Vec::new(),
        };
        let c = Curated {
            version_recipes: [("keen4-1.4-ega".to_string(), other.clone())].into(),
            copy_to_root: vec!["1".into()],
            ..curated(aurora_patch(&[]))
        };
        let m = install_curated(&paths, &db, &zip, &c, None, false).unwrap();
        let files_dir = paths.mods.join(&m.id).join("files");
        assert_eq!(
            fs::read(files_dir.join("CREATURE.CK4")).unwrap(),
            b"big sprites"
        );
        assert!(files_dir.join("1/CREATURE.CK4").exists());
        // Played on 1.4 EGA, the mod starts with that version's recipe.
        assert_eq!(list(&db).unwrap()[0].recipe, aurora_patch(&[]));
        db.conn()
            .execute("UPDATE installs SET version_id = 'keen4-1.4-ega'", [])
            .unwrap();
        assert_eq!(list(&db).unwrap()[0].recipe, other);
        let (_, layers) = profiles::get(&db, &m.profile_id).unwrap();
        assert_eq!(layers.mod_layer.unwrap().recipe, other);

        // Elsewhere the default one.
        let by_version = c.version_recipes.clone();
        let default = aurora_patch(&[]);
        assert_eq!(
            recipe::for_version(&default, &by_version, "keen4", Some("keen4-1.4-cga")),
            &default
        );
        assert_eq!(
            recipe::for_version(&default, &by_version, "keen4", None),
            &default
        );

        let missing = Curated {
            copy_to_root: vec!["2".into()],
            ..c
        };
        assert!(matches!(
            install_curated(&paths, &db, &zip, &missing, None, false),
            Err(Error::UnexpectedContent(_))
        ));
    }

    #[test]
    fn root_is_the_folder_with_most_keen_files() {
        let files: Vec<String> = [
            "Mod/LEVEL01.CK1",
            "Mod/LEVEL02.CK1",
            "Mod/extras/LEVEL01.CK1",
            "readme.txt",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(find_root(&files, |_| false).as_deref(), Some("Mod"));
        assert_eq!(find_root(&["a.txt".to_string()], |_| false), None);

        // As Keen Rescue: more levels in a subfolder than files next to the
        // patcher.
        let mut files = vec![
            "krescue/CK3PATCH.EXE".to_string(),
            "krescue/martakr.pat".into(),
        ];
        files.extend((1..=5).map(|n| format!("krescue/levels/LEVEL0{n}.CK3")));
        assert_eq!(find_root(&files, |_| false).as_deref(), Some("krescue"));
    }

    #[test]
    fn metadata_from_names() {
        let m = guess_metadata(
            Path::new("m.88vega.fireblaze_curse_of_the_dragonlords.k1.2018-11-18.zip"),
            "Fireblaze_Curse_of_the_Dragonlords",
        );
        assert_eq!(m.name, "Fireblaze Curse of the Dragonlords");
        assert_eq!(m.episode, Some("keen1"));
        assert_eq!(m.version, None);

        let m = guess_metadata(
            Path::new("l.alopex.back_to_fribbulus_xax_v1.0.k6.2024-10-03.zip"),
            "",
        );
        assert_eq!(
            (m.name.as_str(), m.version.as_deref()),
            ("Back to Fribbulus Xax", Some("1.0"))
        );

        let m = guess_metadata(
            Path::new("m.shikadi.simulator_keen.v.1.1.k3.2023-03-25.zip"),
            "",
        );
        assert_eq!(
            (m.name.as_str(), m.version.as_deref()),
            ("Simulator Keen", Some("1.1"))
        );
        let m = guess_metadata(Path::new("l.shikadi.k1.2008-02-14.zip"), "");
        assert_eq!(
            (m.name.as_str(), m.episode),
            ("Shikadi Level Pack", Some("keen1"))
        );

        assert_eq!(
            guess_metadata(Path::new("x/btower.zip"), "BTOWER").name,
            "BTOWER"
        );
        assert_eq!(guess_metadata(Path::new("x/my_mod.zip"), "").name, "My Mod");
        assert_eq!(
            guess_metadata(
                Path::new("m.keen_community.patchwork_mod_iii.k3.2008-10-29.zip"),
                ""
            )
            .name,
            "Patchwork Mod III"
        );
        assert_eq!(title_case("vi xiv mix ivy x"), "VI XIV Mix Ivy X");
        assert_eq!(
            guess_metadata(Path::new("x/magnet.zip"), "stuff/magnet").name,
            "Magnet"
        );
    }

    #[test]
    fn third_party_builds_count_as_their_original() {
        let k6 = catalog::game("keen6").unwrap();
        assert_eq!(
            version_ids(k6, &["1.0".into()]),
            ["keen6-1.0", "keen6-1.0-nocp"]
        );
        // Not the CGA release of the same number.
        assert_eq!(version_ids(k6, &["1.4".into()]), ["keen6-1.4-ega"]);
        let kdreams = catalog::game("kdreams").unwrap();
        assert_eq!(
            version_ids(kdreams, &["1.93".into()]),
            ["kdreams-1.93-anthology", "kdreams-1.93"]
        );
    }

    #[test]
    fn slugs() {
        assert_eq!(
            slug("The Grand Intelligence: Gold!"),
            "the-grand-intelligence-gold"
        );
        assert_eq!(slug("  ***  "), "mod");
        assert_eq!(slug("Ünïcode"), "n-code");
    }
}

/// Checks the heuristics against real mods listed in
/// `tests/mod-corpus.json`: download URL, SHA-256 and the expected result.
/// The archives are never committed; missing ones are downloaded once into
/// `BLOOGBASE_MOD_CACHE` (default `.local/mods`).
///
/// `cargo test corpus -- --ignored --nocapture`
/// `BLOOGBASE_CORPUS_UPDATE=1 cargo test corpus -- --ignored` rewrites the
/// expectations from the current results (review the diff!).
#[cfg(test)]
mod corpus {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::downloads;

    const MANIFEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/mod-corpus.json");

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Manifest {
        #[serde(rename = "_comment")]
        comment: String,
        mods: Vec<Entry>,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Entry {
        url: String,
        #[serde(default)]
        sha256: String,
        #[serde(flatten)]
        expect: Option<Expected>,
    }

    /// What `inspect` should report: the game, the name, and the default recipe.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Expected {
        game_id: String,
        name: String,
        recipe: Recipe,
        version_ids: Vec<String>,
        confident: bool,
    }

    impl From<&ModInfo> for Expected {
        fn from(info: &ModInfo) -> Self {
            Expected {
                game_id: info.game_id.clone(),
                name: info.name.clone(),
                recipe: info.recipes[0].recipe.clone(),
                version_ids: info.recipes[0].version_ids.clone(),
                confident: info.confident,
            }
        }
    }

    fn cache_dir() -> PathBuf {
        std::env::var_os("BLOOGBASE_MOD_CACHE")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.local/mods"))
    }

    /// The cached archive, downloaded first if missing. Downloads are spaced
    /// out so the community's file server isn't hammered.
    async fn archive(entry: &Entry) -> std::result::Result<PathBuf, String> {
        let file = entry.url.rsplit('/').next().unwrap_or_default();
        let path = cache_dir().join(file);
        if !path.is_file() {
            if entry.sha256.is_empty() {
                return Err("not cached and no sha256 to verify a download".into());
            }
            eprintln!("downloading {}", entry.url);
            downloads::fetch(&entry.url, &entry.sha256, 0, &path, &|_, _| {})
                .await
                .map_err(|e| e.to_string())?;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Ok(path)
    }

    #[tokio::test]
    #[ignore = "downloads real mods on first run"]
    async fn corpus_matches_expectations() {
        let update = std::env::var_os("BLOOGBASE_CORPUS_UPDATE").is_some();
        let mut manifest: Manifest =
            serde_json::from_str(&fs::read_to_string(MANIFEST).unwrap()).unwrap();
        let mut failures = Vec::new();

        for entry in &mut manifest.mods {
            let path = match archive(entry).await {
                Ok(p) => p,
                Err(e) => {
                    failures.push(format!("{}: {e}", entry.url));
                    continue;
                }
            };
            let hash = sha256_hex(&fs::read(&path).unwrap());
            if entry.sha256.is_empty() && update {
                entry.sha256 = hash.clone();
            }
            if !hash.eq_ignore_ascii_case(&entry.sha256) {
                failures.push(format!("{}: cached file has sha256 {hash}", path.display()));
                continue;
            }
            let actual = match inspect(&path, None) {
                Ok(info) => Expected::from(&info),
                Err(e) => {
                    failures.push(format!("{}: {e}", entry.url));
                    continue;
                }
            };
            if update {
                entry.expect = Some(actual);
            } else if entry.expect.as_ref() != Some(&actual) {
                failures.push(format!(
                    "{}\n  expected {:?}\n  actual   {actual:?}",
                    entry.url, entry.expect
                ));
            }
        }

        if update {
            fs::write(
                MANIFEST,
                serde_json::to_string_pretty(&manifest).unwrap() + "\n",
            )
            .unwrap();
            eprintln!("updated {MANIFEST}");
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        eprintln!("{} mods as expected", manifest.mods.len());
    }
}

//! Storing a mod: from a confirmed inspection or an index entry, as a new
//! mod or an update in place.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use crate::catalog::{self, Game};
use crate::db::Db;
use crate::detect::sha256_hex;
use crate::error::{Error, Result};
use crate::launcher::Running;
use crate::library::{self, Install, now};
use crate::mod_index::ModStatus;
use crate::mod_index::{CoverFrom, IndexEntry, Part, SaveDir, Variant};
use crate::paths::AppPaths;
use crate::recipe::{Recipe, VersionRecipes};
use crate::source::{Source, split_dir};
use crate::worlds::Worlds;

use super::analyze::{Analysis, analyze, analyze_in};
use super::fit::{best_install, fits, needed_game, parse_requires, rehome, requires};
use super::queries::list;
use super::variant_files::{self, VariantFiles};
use super::{InstallModRequest, Mod, ModInfo, ModSource, RecipeOption};

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
    /// The installed mod it's played over.
    extends: Option<String>,
    /// The program whose title screen is the cover; default: the recipe's.
    cover: Option<String>,
    /// Plays without a copy of its game: index entries say so, or the mod
    /// brings all of a game whose shareware is free.
    standalone: bool,
    /// Composed from its own files alone: index entries say so, or the mod
    /// brings all of the game's files.
    own_files: bool,
    keenwiki_url: Option<&'a str>,
    homepage: Option<String>,
    pckf: Option<String>,
    keenmodding: Option<String>,
    status: Option<ModStatus>,
    /// The picked folder or ZIP, or the download URL.
    source_url: &'a str,
    /// Name for the kept copy of the archive; default: its own.
    archive_name: Option<&'a str>,
    /// The download the files are; default: the hash of the picked ZIP.
    archive_sha256: Option<&'a str>,
    /// The folder of the archive the files are from, as the index entry
    /// names it; `None` where the app found it.
    root: Option<&'a str>,
    /// The ways to play it, each in `variants/<id>/` instead of `files/`.
    variants: &'a [Variant],
}

impl<'a> Stored<'a> {
    /// A mod as the index describes it, played over `extends`.
    fn curated(c: &'a Curated, extends: Option<String>) -> Self {
        Stored {
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
            extends,
            cover: c.cover.clone(),
            standalone: c.standalone,
            own_files: c.own_files,
            keenwiki_url: c.keenwiki_url.as_deref(),
            // The index entry links it.
            homepage: None,
            pckf: None,
            keenmodding: None,
            status: None,
            source_url: &c.url,
            archive_name: Some(crate::downloads::url_file_name(&c.url)).filter(|n| !n.is_empty()),
            archive_sha256: Some(&c.sha256),
            root: c.root.as_deref(),
            variants: &c.variants,
        }
    }

    /// A mod the player picked, named `name`, with what its KeenWiki page
    /// says when it came from there.
    fn picked(
        req: &'a InstallModRequest,
        info: &ModInfo,
        name: String,
        recipe: Recipe,
        requires: Option<Vec<String>>,
    ) -> Self {
        let page = req.keenwiki.as_ref();
        Stored {
            name,
            authors: page
                .map(|o| o.authors.clone())
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| info.authors.clone()),
            author_links: page.map(|o| o.author_links.clone()).unwrap_or_default(),
            version: page
                .and_then(|o| o.version.clone())
                .or_else(|| info.version.clone()),
            description: page.and_then(|o| o.description.clone()),
            released: page
                .and_then(|o| o.released.clone())
                .or_else(|| info.released.clone()),
            recipe,
            version_recipes: VersionRecipes::new(),
            requires,
            source: if page.is_some() {
                ModSource::Keenwiki
            } else {
                ModSource::Manual
            },
            index_id: None,
            extends: req.extends.clone(),
            cover: None,
            standalone: false,
            own_files: false,
            keenwiki_url: page.map(|o| o.page_url.as_str()),
            homepage: page.and_then(|o| o.homepage.clone()),
            pckf: page.and_then(|o| o.pckf.clone()),
            keenmodding: page.and_then(|o| o.keenmodding.clone()),
            status: ModStatus::stored(page.and_then(|o| o.status.as_deref())),
            source_url: page.map_or(&req.path, |o| &o.download_url),
            archive_name: None,
            archive_sha256: None,
            root: None,
            variants: &[],
        }
    }

    /// Fills in from `old`, the version it replaces, what the files can't
    /// tell but is still true of the new version.
    fn keeping(mut self, old: &'a Mod) -> Self {
        if self.author_links.is_empty() {
            self.author_links = old.author_links.clone();
        }
        self.description = self.description.or_else(|| old.description.clone());
        self.keenwiki_url = self.keenwiki_url.or(old.keenwiki_url.as_deref());
        self.homepage = self.homepage.or_else(|| old.homepage.clone());
        self.pckf = self.pckf.or_else(|| old.pckf.clone());
        self.keenmodding = self.keenmodding.or_else(|| old.keenmodding.clone());
        self.status = self.status.or(old.status);
        self
    }
}

/// Installs a mod and creates its play profile on `install_id`, or without
/// a game.
pub fn install(paths: &AppPaths, db: &Db, req: &InstallModRequest) -> Result<Mod> {
    let path = Path::new(&req.path);
    let source = Source::open(path)?;
    let hint = req.keenwiki.as_ref().and_then(|o| o.game_id.as_deref());
    let analysis = analyze(&source, path, hint)?;
    let game_id = &analysis.info.game_id;
    let base = match &req.extends {
        Some(id) => Some(base_of(db, id, game_id)?),
        None => None,
    };
    let option = picked_option(&analysis.info, base.as_ref(), &req.recipe)?;
    // Like `check_install`: a patch that names versions plays on those only;
    // else where the mod below plays. A complete mod plays anywhere.
    let requires = if analysis.info.complete && base.is_none() {
        None
    } else if option.game_versions.is_empty() {
        base.as_ref().and_then(|b| b.requires.clone())
    } else {
        Some(option.version_ids.clone())
    };
    // A mod over a standalone one plays without the game too.
    let on_its_own = match &base {
        Some(b) => b.standalone,
        None => analysis.info.standalone,
    };
    let target = match &req.install_id {
        _ if on_its_own => None,
        Some(id) => Some(picked_install(
            db,
            id,
            game_id,
            &option,
            requires.as_deref(),
        )?),
        None => None,
    };
    let replaced = match &req.replace {
        Some(id) => Some(replaced_mod(db, id, game_id)?),
        None => None,
    };
    let name = match req.name.trim() {
        "" => analysis.info.name.clone(),
        n => n.to_string(),
    };
    let mut stored = Stored::picked(req, &analysis.info, name, option.recipe, requires);
    stored.standalone = on_its_own;
    if base.is_none() {
        stored.own_files = analysis.info.complete;
    }
    let layout = Layout {
        analysis: &analysis,
        variants: &[],
        parts: Vec::new(),
    };
    match &replaced {
        None => store(paths, db, &source, &layout, &stored, target.as_ref()),
        Some(old) => update(
            paths,
            db,
            &old.id,
            &source,
            &layout,
            &stored.keeping(old),
            target.as_ref(),
        ),
    }
}

/// The recipe the player picked among `info`'s, or the one of `base`, the
/// mod it's played over.
fn picked_option(info: &ModInfo, base: Option<&Base>, recipe: &Recipe) -> Result<RecipeOption> {
    let own = info.recipes.iter().find(|o| o.recipe == *recipe).cloned();
    // Levels alone start with the mod below's recipe, which it may also pick.
    Ok(match (base, own) {
        (Some(b), Some(o)) if o.recipe == Recipe::Episode => b.option(),
        (_, Some(o)) => o,
        (Some(b), None) if b.recipe == *recipe => b.option(),
        _ => return Err(Error::IncompatibleInstall("unknown recipe".into())),
    })
}

/// The copy of the game the player picked, if the mod plays on it.
fn picked_install(
    db: &Db,
    id: &str,
    game_id: &str,
    option: &RecipeOption,
    requires: Option<&[String]>,
) -> Result<Install> {
    let target =
        library_install(db, id)?.ok_or_else(|| Error::NotFound(format!("install {id}")))?;
    check_install(game_id, option, &target)?;
    if !fits(game(&target.game_id)?, requires, &target) {
        return Err(Error::IncompatibleInstall(format!(
            "the mod it's played over doesn't fit {}",
            target.id
        )));
    }
    Ok(target)
}

/// The mod a new version replaces, which must be for the same game and not
/// from the index: those update from it, and other files go alongside.
fn replaced_mod(db: &Db, id: &str, game_id: &str) -> Result<Mod> {
    let old = list(db)?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| Error::NotFound(format!("mod {id}")))?;
    if old.game_id != game_id {
        return Err(Error::IncompatibleInstall(format!(
            "{} is for {}, not {game_id}",
            old.name, old.game_id
        )));
    }
    if let Some(entry) = &old.index_id {
        return Err(Error::IncompatibleInstall(format!(
            "{} updates from the mod index ({entry})",
            old.name
        )));
    }
    Ok(old)
}

fn library_install(db: &Db, id: &str) -> Result<Option<Install>> {
    Ok(library::list(db)?.into_iter().find(|i| i.id == id))
}

fn game(id: &str) -> Result<&'static Game> {
    catalog::game(id).ok_or_else(|| Error::NotFound(format!("game {id}")))
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
    /// The index mod it's played over, which has to be installed first.
    pub extends: Option<String>,
    /// The program whose title screen is the cover.
    pub cover: Option<String>,
    /// Plays without a copy of `game_id`.
    pub standalone: bool,
    /// Composed from its own files alone, without the game's below them;
    /// set for every standalone mod, and for source mods that need their
    /// game all the same (`requiresGame`).
    pub own_files: bool,
    pub keenwiki_url: Option<String>,
    pub url: String,
    /// The download's SHA-256.
    pub sha256: String,
    /// How it keeps the levels it generates.
    pub worlds: Option<Worlds>,
    /// The folder of the archive it's installed from, `""` for its root;
    /// `None`: the one found.
    pub root: Option<String>,
    /// Folders right in its root the game keeps saves and settings in.
    pub save_dirs: Vec<SaveDir>,
    /// The folder and patch whose title screen is the cover.
    pub cover_from: Option<CoverFrom>,
    /// The ways to play it, with the newest download; empty for most mods.
    pub variants: Vec<Variant>,
    /// Key → further archives of the download, which variants come from.
    pub parts: BTreeMap<String, Part>,
    /// Key → where each of `parts` lies, downloaded by the caller.
    pub part_files: BTreeMap<String, PathBuf>,
}

impl Curated {
    /// The entry with its `download`th download. Its version is the
    /// download's; the entry's is the newest download's.
    pub fn new(entry: &IndexEntry, download: usize) -> Self {
        let d = &entry.downloads[download];
        Curated {
            index_id: entry.id.clone(),
            game_id: entry.game.clone(),
            name: entry.name.clone(),
            authors: entry.authors.clone(),
            author_links: entry.author_links.clone(),
            version: d
                .version
                .clone()
                .or_else(|| entry.version.clone().filter(|_| download == 0)),
            description: entry.description.clone(),
            released: entry.released.clone(),
            // An older download may start unlike the newest.
            recipe: d.recipe.clone().unwrap_or_else(|| entry.recipe.clone()),
            version_recipes: match d.recipe {
                Some(_) => VersionRecipes::new(),
                None => entry.version_recipes.clone(),
            },
            copy_to_root: entry.copy_to_root.clone(),
            extends: entry.extends.clone(),
            cover: entry.cover.clone(),
            standalone: entry.needs_no_game(),
            own_files: entry.standalone,
            requires: entry.requires.clone(),
            keenwiki_url: entry.links.keenwiki.clone(),
            url: d.url.clone(),
            sha256: d.sha256.clone(),
            worlds: entry.worlds.clone(),
            root: entry.root.clone(),
            save_dirs: entry.save_dirs.clone(),
            cover_from: entry.cover_from.clone(),
            // They describe the newest version; an older one gets them
            // with its update.
            variants: match download {
                0 => entry.variants.clone(),
                _ => Vec::new(),
            },
            parts: d.parts.clone(),
            part_files: BTreeMap::new(),
        }
    }

    /// The same without its variants: installed as it was before them.
    pub fn without_variants(&self) -> Self {
        Curated {
            variants: Vec::new(),
            parts: BTreeMap::new(),
            part_files: BTreeMap::new(),
            ..self.clone()
        }
    }
}

/// Ties an installed mod to the index entry whose download it was
/// installed from, and takes the entry's details over what the mod brought
/// along, as an install from the index would store them, and the download
/// it is, so a newer one shows as an update. How it's played follows at
/// the same index check ([`follow_entry`]), once its files are checked.
pub fn tie_to_index(db: &Db, mod_id: &str, c: &Curated) -> Result<()> {
    // Every field gets a decision, so new ones aren't forgotten here.
    let Stored {
        name,
        authors,
        author_links,
        version,
        description,
        released,
        recipe: _,
        version_recipes: _,
        requires: _,
        source,
        index_id,
        extends: _,
        cover,
        // Follow the entry for tied mods too: `set_standalone`.
        standalone: _,
        own_files: _,
        keenwiki_url,
        homepage,
        pckf,
        keenmodding,
        status: _,
        source_url: _,
        archive_name: _,
        archive_sha256,
        // Its files stay as installed; `follow_root` moves them.
        root: _,
        // Its files become variants at the next index check (`follow_variants`).
        variants: _,
    } = Stored::curated(c, None);
    db.conn().execute(
        "UPDATE mods SET name = ?2, authors = ?3, author_links = ?4,
                         version = COALESCE(?5, version), description = ?6, released = ?7,
                         source = ?8, index_id = ?9, keenwiki_url = ?10, homepage = ?11,
                         pckf = ?12, keenmodding = ?13, cover = ?14, archive_sha256 = ?15
         WHERE id = ?1",
        params![
            mod_id,
            name,
            serde_json::to_string(&authors).expect("strings serialize"),
            serde_json::to_string(&author_links).expect("strings serialize"),
            version,
            description,
            released,
            source.as_str(),
            index_id,
            keenwiki_url,
            homepage,
            pckf,
            keenmodding,
            cover,
            archive_sha256,
        ],
    )?;
    tracing::info!("tied mod {mod_id} to index entry {}", c.index_id);
    Ok(())
}

/// Installs a downloaded index mod, or updates it when it's installed
/// already. Without `install_id` the best fitting game is used; when there
/// is none, the mod waits for one if `without_game`, else it's an error.
/// A standalone mod goes on no game.
pub fn install_curated(
    paths: &AppPaths,
    db: &Db,
    archive: &Path,
    c: &Curated,
    install_id: Option<&str>,
    without_game: bool,
) -> Result<Mod> {
    let source = Source::open(archive)?;
    let variants = match c.variants.is_empty() {
        true => Vec::new(),
        false => variant_files::analyze_variants(c, archive)?,
    };
    let mut analysis = match variants.iter().zip(&c.variants).find(|(_, v)| v.default) {
        // What the mod is, as the default variant tells.
        Some((d, _)) => analyze_in(&d.source, archive, Some(&c.game_id), Some(&d.analysis.root))?,
        None => analyze_in(&source, archive, Some(&c.game_id), c.root.as_deref())?,
    };
    if analysis.info.game_id != c.game_id {
        return Err(Error::UnexpectedContent(format!(
            "the download looks like a mod for {}, not {}",
            analysis.info.game_id, c.game_id
        )));
    }
    let base_id = match &c.extends {
        Some(base) => {
            Some(find_by_index(db, base)?.ok_or_else(|| Error::MissingGame(base.clone()))?)
        }
        None => None,
    };
    if variants.is_empty() {
        check_recipe_files(paths, &analysis, c, base_id.as_deref())?;
        analysis.copies = copies_to_root(&analysis, &c.copy_to_root)?;
    } else {
        variant_files::check_recipes(c, &variants)?;
    }
    let layout = Layout {
        analysis: &analysis,
        variants: &variants,
        parts: c
            .part_files
            .iter()
            .filter_map(|(key, path)| {
                let part = c.parts.get(key)?;
                Some((
                    path.clone(),
                    crate::downloads::url_file_name(&part.url).to_string(),
                ))
            })
            .collect(),
    };

    // A mod over a standalone one plays without the game too.
    let on_its_own = match &base_id {
        Some(id) => standalone(db, id)?,
        None => c.standalone,
    };
    let mut stored = Stored::curated(c, base_id);
    stored.standalone = on_its_own;
    let game = game(&c.game_id)?;
    let requires = stored.requires.as_deref();
    if let Some(mod_id) = find_by_index(db, &c.index_id)? {
        let target = match on_its_own {
            true => None,
            false => updated_install(db, &mod_id, game, requires)?,
        };
        let m = update(
            paths,
            db,
            &mod_id,
            &source,
            &layout,
            &stored,
            target.as_ref(),
        )?;
        set_worlds(db, &c.index_id, c.worlds.as_ref())?;
        set_save_dirs(db, &c.index_id, &c.save_dirs)?;
        set_cover_from(db, &c.index_id, c.cover_from.as_ref())?;
        return Ok(m);
    }
    let target = match install_id {
        _ if on_its_own => None,
        Some(id) => Some(
            library_install(db, id)?
                .filter(|i| fits(game, requires, i))
                .ok_or_else(|| Error::IncompatibleInstall(format!("{id} can't play {}", c.name)))?,
        ),
        None => match best_install(db, game, requires)? {
            None if !without_game => {
                return Err(Error::MissingGame(needed_game(game, requires)));
            }
            target => target,
        },
    };
    let m = store(paths, db, &source, &layout, &stored, target.as_ref())?;
    set_worlds(db, &c.index_id, c.worlds.as_ref())?;
    set_save_dirs(db, &c.index_id, &c.save_dirs)?;
    set_cover_from(db, &c.index_id, c.cover_from.as_ref())?;
    Ok(m)
}

/// Reinstalls the index mod `mod_id` from the archive it keeps, from the
/// folder its entry now names (`c.root`), as an update to the download `c`
/// would: installed before the entry named it, it holds one folder's files.
/// What the game wrote into its state's root moves into `old_root`, the
/// folder those files came from, where the game now writes it; the
/// overlay's markers stay. Never while the mod runs: then it's `false`, to
/// try again at the next index check. A failure changes nothing.
pub fn follow_root(
    paths: &AppPaths,
    db: &Db,
    running: &Running,
    mod_id: &str,
    c: &Curated,
    old_root: Option<&str>,
) -> Result<bool> {
    let profile_id: String =
        db.conn()
            .query_row("SELECT id FROM profiles WHERE mod_id = ?1", [mod_id], |r| {
                r.get(0)
            })?;
    let Ok(_claim) = running.claim(&profile_id) else {
        return Ok(false);
    };
    let kept = kept_archive(paths, mod_id, &c.url)
        .ok_or_else(|| Error::NotFound(format!("the archive of mod {mod_id}")))?;
    // Variants come once it's in its root (`follow_variants`).
    let c = &c.without_variants();
    // A copy: the folder it's in gets replaced while it's read.
    let archive = paths.cache.join(format!("mod-{mod_id}-root.zip"));
    fs::copy(&kept, &archive)?;
    let result = (|| {
        let state = crate::profiles::get(db, &profile_id)?.1.upper;
        let moved = match old_root {
            Some(dir) => move_state_into(&state, dir)?,
            None => Vec::new(),
        };
        let installed = install_curated(paths, db, &archive, c, None, true);
        Ok::<_, Error>((state, moved, installed))
    })();
    let _ = fs::remove_file(&archive);
    let (state, moved, installed) = result?;
    match installed {
        Ok(_) => {
            tracing::info!(
                "reinstalled mod {mod_id} from {:?}, moved {} files of its state into {old_root:?}",
                c.root,
                moved.len()
            );
            Ok(true)
        }
        Err(e) => {
            for (from, to) in moved.iter().rev() {
                let _ = fs::rename(to, from);
            }
            if let Some(dir) = old_root {
                // Only if it's empty: it was made for the move.
                let _ = fs::remove_dir(state.join(dir));
            }
            Err(e)
        }
    }
}

/// The archive mod `mod_id` keeps of the download at `url`: named after it,
/// else the only one it keeps.
pub fn kept_archive(paths: &AppPaths, mod_id: &str, url: &str) -> Option<PathBuf> {
    let dir = paths.mods.join(mod_id).join("source");
    let named = dir.join(crate::downloads::url_file_name(url));
    if named.is_file() && !crate::downloads::url_file_name(url).is_empty() {
        return Some(named);
    }
    let files: Vec<PathBuf> = fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    match files.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

/// Moves the files right in `state` into its folder `dir` (made if need
/// be), but for the overlay's markers and names `dir` has already: from →
/// to.
fn move_state_into(state: &Path, dir: &str) -> Result<Vec<(PathBuf, PathBuf)>> {
    let Ok(entries) = fs::read_dir(state) else {
        return Ok(Vec::new());
    };
    let mut files = Vec::new();
    let mut target = state.join(dir);
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_dir() {
            // DOSBox may have spelled the folder otherwise.
            if name.eq_ignore_ascii_case(dir) {
                target = entry.path();
            }
            continue;
        }
        if !crate::overlay_view::is_marker(&name) {
            files.push((name, entry.path()));
        }
    }
    if files.is_empty() {
        return Ok(Vec::new());
    }
    fs::create_dir_all(&target)?;
    let there: Vec<String> = fs::read_dir(&target)?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let mut moved = Vec::new();
    for (name, from) in files {
        if there.iter().any(|t| t.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let to = target.join(&name);
        if let Err(e) = fs::rename(&from, &to) {
            for (from, to) in moved.iter().rev() {
                let _ = fs::rename(to, from);
            }
            return Err(e.into());
        }
        moved.push((from, to));
    }
    Ok(moved)
}

/// Makes the installed index mod `mod_id` follow its entry `c`, the
/// download it is (decided 2026-11-19): its details (name, authors and
/// their links, description, release date, links, cover), and its recipe
/// and version recipes, if its files have what they run and it isn't
/// running; then its composed layer is made anew. The version and the
/// download stay those installed. Whether anything changed.
pub fn follow_entry(
    paths: &AppPaths,
    db: &Db,
    running: &Running,
    mod_id: &str,
    c: &Curated,
) -> Result<bool> {
    let details = follow_details(db, mod_id, c)?;
    let s = Stored::curated(c, None);
    let recipe = serde_json::to_string(&s.recipe).expect("recipes serialize");
    let by_version = version_recipes_json(&s);
    let (stored, stored_by_version, base, profile_id): (
        String,
        Option<String>,
        Option<String>,
        String,
    ) = db.conn().query_row(
        "SELECT m.recipe_json, m.version_recipes_json, m.extends, p.id
         FROM mods m JOIN profiles p ON p.mod_id = m.id WHERE m.id = ?1",
        [mod_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    if stored == recipe && stored_by_version == by_version {
        return Ok(details);
    }
    let Ok(_claim) = running.claim(&profile_id) else {
        tracing::info!("mod {mod_id} runs; it takes its entry's recipe later");
        return Ok(details);
    };
    // The entry's recipe is the default variant's.
    let default: Option<String> = db.conn().query_row(
        "SELECT variants_json FROM mods WHERE id = ?1",
        [mod_id],
        |r| r.get(0),
    )?;
    let default = default
        .and_then(|j| serde_json::from_str::<Vec<Variant>>(&j).ok())
        .and_then(|vs| vs.into_iter().find(|v| v.default))
        .map(|v| v.id);
    let in_root: Vec<String> = fs::read_dir(paths.mod_files(mod_id, default.as_deref()))?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    if let Err(e) = check_recipe_names(paths, in_root, c, base.as_deref()) {
        tracing::warn!("mod {mod_id} keeps its recipe, its entry's doesn't fit: {e}");
        return Ok(details);
    }
    let mut conn = db.conn();
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE mods SET recipe_json = ?2, version_recipes_json = ?3 WHERE id = ?1",
        params![mod_id, recipe, by_version],
    )?;
    tx.execute(
        "UPDATE profiles SET composed_hash = NULL WHERE mod_id = ?1",
        [mod_id],
    )?;
    tx.commit()?;
    tracing::info!("mod {mod_id} takes its entry's recipe {:?}", s.recipe);
    Ok(true)
}

/// Takes the details of the entry `c` over those stored for `mod_id`, as
/// [`tie_to_index`] does, but for the download and its version. Whether
/// they changed.
fn follow_details(db: &Db, mod_id: &str, c: &Curated) -> Result<bool> {
    // Every field gets a decision, so new ones aren't forgotten here.
    let Stored {
        name,
        authors,
        author_links,
        version: _,
        description,
        released,
        // `follow_entry`, once the files are checked.
        recipe: _,
        version_recipes: _,
        requires: _,
        source: _,
        index_id: _,
        extends: _,
        cover,
        standalone: _,
        own_files: _,
        keenwiki_url,
        homepage,
        pckf,
        keenmodding,
        status: _,
        source_url: _,
        archive_name: _,
        archive_sha256: _,
        root: _,
        variants: _,
    } = Stored::curated(c, None);
    let changed = db.conn().execute(
        "UPDATE mods SET name = ?2, authors = ?3, author_links = ?4, description = ?5,
                         released = ?6, keenwiki_url = ?7, homepage = ?8, pckf = ?9,
                         keenmodding = ?10, cover = ?11
         WHERE id = ?1
           AND NOT (name IS ?2 AND authors IS ?3 AND author_links IS ?4 AND description IS ?5
                    AND released IS ?6 AND keenwiki_url IS ?7 AND homepage IS ?8 AND pckf IS ?9
                    AND keenmodding IS ?10 AND cover IS ?11)",
        params![
            mod_id,
            name,
            serde_json::to_string(&authors).expect("strings serialize"),
            serde_json::to_string(&author_links).expect("strings serialize"),
            description,
            released,
            keenwiki_url,
            homepage,
            pckf,
            keenmodding,
            cover,
        ],
    )? > 0;
    if changed {
        tracing::info!(
            "mod {mod_id} takes the details of index entry {}",
            c.index_id
        );
    }
    Ok(changed)
}

/// Makes the mods installed from the index entry `index_id` keep their
/// worlds as it says (§9.6); the entry may gain or change them later.
pub fn set_worlds(db: &Db, index_id: &str, worlds: Option<&Worlds>) -> Result<()> {
    let json = worlds.map(|w| serde_json::to_string(w).expect("worlds serialize"));
    db.conn().execute(
        "UPDATE mods SET worlds_json = ?2 WHERE index_id = ?1 AND worlds_json IS NOT ?2",
        params![index_id, json],
    )?;
    Ok(())
}

/// Makes the mods installed from the index entry `index_id` keep saves and
/// settings in the folders it lists (`saveDirs`) too.
pub fn set_save_dirs(db: &Db, index_id: &str, dirs: &[SaveDir]) -> Result<()> {
    let json = (!dirs.is_empty()).then(|| serde_json::to_string(dirs).expect("dirs serialize"));
    db.conn().execute(
        "UPDATE mods SET save_dirs_json = ?2 WHERE index_id = ?1 AND save_dirs_json IS NOT ?2",
        params![index_id, json],
    )?;
    Ok(())
}

/// Makes the mods installed from the index entry `index_id` take their
/// cover from the folder it names (`coverFrom`).
pub fn set_cover_from(db: &Db, index_id: &str, cover_from: Option<&CoverFrom>) -> Result<()> {
    let json = cover_from.map(|c| serde_json::to_string(c).expect("cover serializes"));
    db.conn().execute(
        "UPDATE mods SET cover_from_json = ?2 WHERE index_id = ?1 AND cover_from_json IS NOT ?2",
        params![index_id, json],
    )?;
    Ok(())
}

/// Installs an index mod from the player's own folder or ZIP, whose files
/// are those of the entry's download `c` (see
/// [`crate::mod_index::ModIndex::entry_for_files`]): the entry's recipe and
/// details, the player's files. Their saves and settings go to the
/// profile's state, where the game writes them, so an update keeps them.
pub fn install_players_files(
    paths: &AppPaths,
    db: &Db,
    path: &Path,
    c: &Curated,
    install_id: Option<&str>,
    without_game: bool,
) -> Result<Mod> {
    let m = install_curated(paths, db, path, c, install_id, without_game)?;
    keep_players_state(paths, db, &m)?;
    Ok(m)
}

/// Moves the files the game writes (saves, settings) from the mod's root to
/// its profile's state, unless the state has one of that name already.
fn keep_players_state(paths: &AppPaths, db: &Db, m: &Mod) -> Result<()> {
    let (_, layers) = crate::profiles::get(db, &m.profile_id)?;
    let game = crate::saves::profile_game(&m.game_id, &layers)?;
    let files = super::active_files(paths, db, &m.id)?;
    let kept: Vec<String> = fs::read_dir(&layers.upper)?
        .filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    let mut moved = 0;
    for entry in fs::read_dir(&files)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type()?.is_file()
            || !game.is_state_file(&name)
            || kept.iter().any(|k| k.eq_ignore_ascii_case(&name))
        {
            continue;
        }
        fs::rename(entry.path(), layers.upper.join(&name))?;
        moved += 1;
    }
    if moved > 0 {
        tracing::info!(
            "kept {moved} saves and settings of mod {} in its state",
            m.id
        );
        db.conn().execute(
            "UPDATE profiles SET composed_hash = NULL WHERE id = ?1",
            [&m.profile_id],
        )?;
    }
    Ok(())
}

/// Checks that the files the index recipes run are in the mod's root, or in
/// the one of `base_id`, the mod it's played over.
fn check_recipe_files(
    paths: &AppPaths,
    analysis: &Analysis,
    c: &Curated,
    base_id: Option<&str>,
) -> Result<()> {
    let in_root: Vec<String> = analysis
        .files
        .iter()
        .filter(|f| split_dir(f).0 == analysis.root)
        .map(|f| split_dir(f).1.to_string())
        .collect();
    check_recipe_names(paths, in_root, c, base_id)
}

/// Checks that the files the index recipes run are among `in_root`, the
/// names in the mod's root, or in the root of `base_id`.
fn check_recipe_names(
    paths: &AppPaths,
    mut in_root: Vec<String>,
    c: &Curated,
    base_id: Option<&str>,
) -> Result<()> {
    if let Some(base_id) = base_id {
        for entry in fs::read_dir(paths.mods.join(base_id).join("files"))? {
            in_root.push(entry?.file_name().to_string_lossy().into_owned());
        }
    }
    let recipes = std::iter::once(&c.recipe).chain(c.version_recipes.values());
    for file in recipes.flat_map(Recipe::files) {
        if !in_root.iter().any(|n| n.eq_ignore_ascii_case(file)) {
            return Err(Error::UnexpectedContent(format!("{file} is missing")));
        }
    }
    Ok(())
}

/// The copy an installed index mod plays on, which its new version must fit
/// too; `None` while it waits for its game.
fn updated_install(
    db: &Db,
    mod_id: &str,
    game: &Game,
    requires: Option<&[String]>,
) -> Result<Option<Install>> {
    let install_id: Option<String> = db.conn().query_row(
        "SELECT install_id FROM profiles WHERE mod_id = ?1",
        [mod_id],
        |r| r.get(0),
    )?;
    let Some(id) = install_id else {
        return Ok(None);
    };
    match library_install(db, &id)?.filter(|i| fits(game, requires, i)) {
        Some(target) => Ok(Some(target)),
        None => Err(Error::IncompatibleInstall(format!(
            "the new version needs {}",
            needed_game(game, requires)
        ))),
    }
}

/// Makes the mods installed from the index entry `index_id` follow its
/// `standalone` and `requiresGame`: one that needs no game (`standalone`)
/// leaves its copy of the game, one that does again goes on the copy that
/// fits best, or waits for one; `own_files` says whether it's composed from
/// its own files alone. Mods played over them follow. Saves stay.
pub fn set_standalone(db: &Db, index_id: &str, standalone: bool, own_files: bool) -> Result<()> {
    let mut left = false;
    let mut taken_off = Vec::new();
    for (mod_id, lacking) in installed_of(db, index_id)? {
        // Stays on its copy until an update brings the executable along.
        if lacking && own_files {
            continue;
        }
        db.conn().execute(
            "UPDATE mods SET own_files = ?2 WHERE id = ?1 AND own_files IS NOT ?2",
            params![mod_id, own_files],
        )?;
        // The mods played over it follow, also ones an earlier change left behind.
        let over: Vec<String> = db
            .conn()
            .prepare("SELECT id FROM mods WHERE extends = ?1")?
            .query_map([&mod_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for id in std::iter::once(mod_id).chain(over) {
            let changed = db.conn().execute(
                "UPDATE mods SET standalone = ?2 WHERE id = ?1 AND standalone IS NOT ?2",
                params![id, standalone],
            )?;
            if changed == 0 {
                continue;
            }
            tracing::info!("mod {id} (index entry {index_id}) now standalone: {standalone}");
            match standalone {
                true => taken_off.push(id),
                false => left = true,
            }
        }
    }
    for mod_id in taken_off {
        db.conn().execute(
            "UPDATE profiles SET install_id = NULL, composed_hash = NULL WHERE mod_id = ?1",
            [mod_id],
        )?;
    }
    if left {
        rehome(db, None)?;
    }
    Ok(())
}

/// The mods installed from index entry `index_id`, and whether each lacks
/// its executable but has all of the game's other files: apps before 1.3.0
/// left out the game's own executable, unchanged, as the copy below brings
/// it (Bounty Hunting Brouhaha), so it can't play from its own files.
/// Source mods run a program of their own instead.
fn installed_of(db: &Db, index_id: &str) -> Result<Vec<(String, bool)>> {
    let rows: Vec<(String, String, String, String, Option<String>)> = db
        .conn()
        .prepare(
            "SELECT m.id, m.game_id, m.path, m.recipe_json,
                    CASE WHEN m.variants_json IS NOT NULL THEN p.active_variant END
             FROM mods m JOIN profiles p ON p.mod_id = m.id WHERE m.index_id = ?1",
        )?
        .query_map([index_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows
        .into_iter()
        .map(|(id, game_id, path, recipe, variant)| {
            let own_program = matches!(serde_json::from_str(&recipe), Ok(Recipe::Program { .. }));
            let dir = crate::paths::mod_files_in(&db.resolve(&path), variant.as_deref());
            let names: Vec<String> = fs::read_dir(dir)
                .map(|d| {
                    d.filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned()))
                        .collect()
                })
                .unwrap_or_default();
            let lacking = !own_program
                && catalog::game(&game_id).is_some_and(|g| {
                    let has = |n: &str| names.iter().any(|m| m.eq_ignore_ascii_case(n));
                    !g.data_files.is_empty()
                        && g.data_files.iter().all(|d| d.split('|').any(has))
                        && !g.executables.iter().any(|e| has(e))
                });
            (id, lacking)
        })
        .collect())
}

/// Whether mod `mod_id` is composed from its own files alone.
/// Whether the installed mod plays without a copy of its game.
pub fn standalone(db: &Db, mod_id: &str) -> Result<bool> {
    Ok(db.conn().query_row(
        "SELECT COALESCE(standalone, 0) FROM mods WHERE id = ?1",
        [mod_id],
        |r| r.get(0),
    )?)
}

pub fn own_files(db: &Db, mod_id: &str) -> Result<bool> {
    Ok(db
        .conn()
        .query_row("SELECT own_files FROM mods WHERE id = ?1", [mod_id], |r| {
            r.get(0)
        })?)
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

/// What a mod's folder gets: its files, or its variants' (then
/// `analysis` is the default's, for what the mod is), and the further
/// archives to keep beside its own: path → name.
struct Layout<'a> {
    analysis: &'a Analysis,
    variants: &'a [VariantFiles],
    parts: Vec<(PathBuf, String)>,
}

/// Copies the mod's files (and the archives) into `staging`.
fn stage(
    source: &Source,
    layout: &Layout,
    staging: &Path,
    archive_name: Option<&str>,
) -> Result<()> {
    let _ = fs::remove_dir_all(staging);
    fs::create_dir_all(staging)?;
    if layout.variants.is_empty() {
        stage_files(source, layout.analysis, staging)?;
    } else {
        variant_files::stage(layout.variants, staging)?;
    }
    if let Source::Zip(zip, _) = source {
        fs::create_dir_all(staging.join("source"))?;
        // The name ends the download's URL; only its last part, so a `\..\`
        // in it can't leave the folder on Windows.
        let name = archive_name
            .and_then(|n| Path::new(n).file_name())
            .unwrap_or_else(|| zip.file_name().expect("a file"));
        fs::copy(zip, staging.join("source").join(name))?;
    }
    for (path, name) in &layout.parts {
        let name = Path::new(name)
            .file_name()
            .ok_or_else(|| Error::NotFound(format!("a name for {}", path.display())))?;
        fs::create_dir_all(staging.join("source"))?;
        fs::copy(path, staging.join("source").join(name))?;
    }
    Ok(())
}

/// Copies the files of a mod without variants into `staging/files/`.
fn stage_files(source: &Source, analysis: &Analysis, staging: &Path) -> Result<()> {
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
    Ok(())
}

fn stored_sha256(s: &Stored, source: &Source) -> Result<Option<String>> {
    match s.archive_sha256 {
        Some(h) => Ok(Some(h.to_string())),
        None => archive_sha256(source),
    }
}

fn archive_sha256(source: &Source) -> Result<Option<String>> {
    Ok(match source {
        Source::Zip(zip, _) => Some(sha256_hex(&fs::read(zip)?)),
        Source::Folder(_) => None,
    })
}

fn version_recipes_json(s: &Stored) -> Option<String> {
    (!s.version_recipes.is_empty())
        .then(|| serde_json::to_string(&s.version_recipes).expect("recipes serialize"))
}

/// Where the files of `folders` (in the mod's root) go when also copied into
/// the root: source path → path in the mod.
pub(super) fn copies_to_root(
    analysis: &Analysis,
    folders: &[String],
) -> Result<Vec<(String, String)>> {
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

/// The variants as stored with the mod, `None` for a mod without.
fn variants_json(s: &Stored) -> Option<String> {
    (!s.variants.is_empty()).then(|| serde_json::to_string(s.variants).expect("variants serialize"))
}

/// The default variant's id.
fn default_variant<'a>(s: &Stored<'a>) -> Option<&'a str> {
    s.variants
        .iter()
        .find(|v| v.default)
        .or(s.variants.first())
        .map(|v| v.id.as_str())
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
    layout: &Layout,
    s: &Stored,
    target: Option<&Install>,
) -> Result<Mod> {
    let analysis = layout.analysis;
    let id = reserve_id(paths, &s.name)?;
    let mod_dir = paths.mods.join(&id);
    let profile_dir = paths.profiles.join(&id);
    let staging = paths.cache.join(format!("mod-{id}"));

    let result = (|| {
        stage(source, layout, &staging, s.archive_name)?;
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
            extends: s.extends.clone(),
            keenwiki_url: s.keenwiki_url.map(str::to_string),
            homepage: s.homepage.clone(),
            pckf: s.pckf.clone(),
            keenmodding: s.keenmodding.clone(),
            status: s.status,
            added_at: now(),
            profile_id: id.clone(),
        };
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO mods (id, game_id, index_id, keenwiki_url, name, authors, version,
                               recipe_json, path, source, source_url, archive_sha256, added_at,
                               description, released, author_links, requires_json,
                               version_recipes_json, extends, homepage, pckf,
                               keenmodding, cover, standalone, own_files, status, root,
                               variants_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)",
            params![
                m.id,
                m.game_id,
                s.index_id,
                s.keenwiki_url,
                m.name,
                serde_json::to_string(&m.authors).expect("strings serialize"),
                m.version,
                serde_json::to_string(&m.recipe).expect("recipes serialize"),
                db.store(&mod_dir),
                m.source.as_str(),
                s.source_url,
                stored_sha256(s, source)?,
                m.added_at,
                m.description,
                m.released,
                serde_json::to_string(&m.author_links).expect("strings serialize"),
                requires_json(s),
                version_recipes_json(s),
                s.extends,
                s.homepage,
                s.pckf,
                s.keenmodding,
                s.cover,
                s.standalone,
                s.own_files,
                s.status.and_then(ModStatus::as_str),
                s.root,
                variants_json(s)
            ],
        )?;
        tx.execute(
            "INSERT INTO profiles (id, install_id, mod_id, state_path, created_at, active_variant)
             VALUES (?1, ?2, ?1, ?3, ?4, ?5)",
            params![
                m.id,
                target.map(|t| &t.id),
                db.store(&profile_dir.join("state")),
                m.added_at,
                default_variant(s)
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
    layout: &Layout,
    s: &Stored,
    target: Option<&Install>,
) -> Result<Mod> {
    let staging = paths.cache.join(format!("mod-{mod_id}-update"));
    let mod_dir = paths.mods.join(mod_id);
    // The parts moved aside so far, put back when a later step fails.
    let mut swapped: Vec<&str> = Vec::new();
    let result: Result<()> = (|| {
        stage(source, layout, &staging, s.archive_name)?;
        // The variant played before, found by its files while they're there.
        let active = active_after_update(db, mod_id, &mod_dir, s)?;
        for part in ["files", "variants", "source"] {
            let live = mod_dir.join(part);
            let old = staging.join(format!("old-{part}"));
            let new = staging.join(part);
            if live.exists() {
                fs::rename(&live, &old)?;
                swapped.push(part);
            }
            if new.exists() {
                fs::rename(&new, &live)?;
            }
        }
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE mods SET authors = ?2, version = ?3, recipe_json = ?4, keenwiki_url = ?5,
                             source_url = ?6, archive_sha256 = ?7, description = ?8,
                             released = ?9, author_links = ?10, requires_json = ?11,
                             name = ?12, source = ?13, index_id = ?14,
                             version_recipes_json = ?15, extends = ?16, homepage = ?17,
                             pckf = ?18, keenmodding = ?19, cover = ?20, standalone = ?21,
                             own_files = ?22, status = ?23, root = ?24,
                             variants_json = ?25
             WHERE id = ?1",
            params![
                mod_id,
                serde_json::to_string(&s.authors).expect("strings serialize"),
                s.version,
                serde_json::to_string(&s.recipe).expect("recipes serialize"),
                s.keenwiki_url,
                s.source_url,
                stored_sha256(s, source)?,
                s.description,
                s.released,
                serde_json::to_string(&s.author_links).expect("strings serialize"),
                requires_json(s),
                s.name,
                s.source.as_str(),
                s.index_id,
                version_recipes_json(s),
                s.extends,
                s.homepage,
                s.pckf,
                s.keenmodding,
                s.cover,
                s.standalone,
                s.own_files,
                s.status.and_then(ModStatus::as_str),
                s.root,
                variants_json(s)
            ],
        )?;
        tx.execute(
            "UPDATE profiles SET install_id = ?2, composed_hash = NULL, active_variant = ?3
             WHERE mod_id = ?1",
            params![mod_id, target.map(|t| &t.id), active],
        )?;
        tx.commit()?;
        Ok(())
    })();
    if result.is_err() {
        for part in swapped.into_iter().rev() {
            let live = mod_dir.join(part);
            if live.exists() {
                let _ = fs::remove_dir_all(&live);
            }
            let _ = fs::rename(staging.join(format!("old-{part}")), &live);
        }
    }
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

/// The variant mod `mod_id` plays after an update to `s`: the one played
/// before, if `s` has it still; for a mod that had none yet, the one whose
/// files it has (its saves are theirs), else the default. `None` without
/// variants.
fn active_after_update(
    db: &Db,
    mod_id: &str,
    mod_dir: &Path,
    s: &Stored,
) -> Result<Option<String>> {
    if s.variants.is_empty() {
        return Ok(None);
    }
    let before: Option<String> = db.conn().query_row(
        "SELECT active_variant FROM profiles WHERE mod_id = ?1",
        [mod_id],
        |r| r.get(0),
    )?;
    let has = |id: &str| s.variants.iter().any(|v| v.id == id);
    Ok(Some(match before {
        Some(v) if has(&v) => v,
        Some(v) => {
            tracing::warn!("mod {mod_id} lost its variant {v}; its saves go to the default");
            default_variant(s).unwrap_or_default().to_string()
        }
        None => played_variant(db, mod_id, &mod_dir.join("files"), s.variants, &s.recipe)?
            .or(default_variant(s))
            .unwrap_or_default()
            .to_string(),
    }))
}

/// Turns the installed index mod `mod_id` into one with the variants of
/// its entry `c` (§9.7), or stages the variants its entry has now: as an
/// update to the same download, from `main`, its archive, and the parts in
/// `c.part_files`. A mod installed before its entry had variants plays the
/// one whose files it had (by fingerprint, else the folder found then, else
/// the default), and its saves stay in `state/`, as that one's; the picker
/// comes at the next "Play" all the same. Never while the mod runs: then
/// it's `false`, to try again at the next index check. A failure changes
/// nothing.
pub fn convert_to_variants(
    paths: &AppPaths,
    db: &Db,
    running: &Running,
    mod_id: &str,
    c: &Curated,
    main: &Path,
) -> Result<bool> {
    let (profile_id, converted): (String, bool) = db.conn().query_row(
        "SELECT p.id, m.variants_json IS NOT NULL
         FROM mods m JOIN profiles p ON p.mod_id = m.id WHERE m.id = ?1",
        [mod_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let Ok(_claim) = running.claim(&profile_id) else {
        return Ok(false);
    };
    if !converted {
        let files = paths.mod_files(mod_id, None);
        let (played, how) = match played_variant(db, mod_id, &files, &c.variants, &c.recipe)? {
            Some(v) => (v.to_string(), "its files"),
            None => match variant_files::found_before(c, main) {
                Some(v) => (v, "the folder found before"),
                None => (
                    c.variants
                        .iter()
                        .find(|v| v.default)
                        .map(|v| v.id.clone())
                        .unwrap_or_default(),
                    "the default",
                ),
            },
        };
        tracing::info!("mod {mod_id} played variant {played} so far, by {how}");
        let before: Option<String> = db.conn().query_row(
            "SELECT active_variant FROM profiles WHERE id = ?1",
            [&profile_id],
            |r| r.get(0),
        )?;
        db.conn().execute(
            "UPDATE profiles SET active_variant = ?2, variant_chosen = 0 WHERE id = ?1",
            params![profile_id, played],
        )?;
        if let Err(e) = install_curated(paths, db, main, c, None, true) {
            db.conn().execute(
                "UPDATE profiles SET active_variant = ?2 WHERE id = ?1",
                params![profile_id, before],
            )?;
            return Err(e);
        }
    } else {
        install_curated(paths, db, main, c, None, true)?;
    }
    tracing::info!("mod {mod_id} has the variants of its entry {}", c.index_id);
    Ok(true)
}

/// The variant whose files `dir` holds, by their fingerprints.
/// Variants with the same files that start differently (Episode Dash's
/// patches) are told apart by the recipe mod `mod_id` was started with;
/// `entry` is the one of variants without their own.
pub(super) fn played_variant<'a>(
    db: &Db,
    mod_id: &str,
    dir: &Path,
    variants: &'a [Variant],
    entry: &Recipe,
) -> Result<Option<&'a str>> {
    let found: Vec<&Variant> = variants
        .iter()
        .filter(|v| {
            v.fingerprint
                .as_ref()
                .is_some_and(|f| variant_files::matches(dir, f))
        })
        .collect();
    if found.len() < 2 {
        return Ok(found.first().map(|v| v.id.as_str()));
    }
    let stored: Option<String> = db
        .conn()
        .query_row(
            "SELECT recipe_json FROM mods WHERE id = ?1",
            [mod_id],
            |r| r.get(0),
        )
        .optional()?;
    let stored: Option<Recipe> = stored.and_then(|j| serde_json::from_str(&j).ok());
    let started = |v: &Variant| Some(v.recipe.as_ref().unwrap_or(entry)) == stored.as_ref();
    Ok(found
        .iter()
        .find(|v| started(v))
        .or(found.first())
        .map(|v| v.id.as_str()))
}

/// An installed mod another one is played over.
struct Base {
    name: String,
    recipe: Recipe,
    requires: Option<Vec<String>>,
    /// It plays without a copy of the game, and so does the mod over it.
    standalone: bool,
}

impl Base {
    /// Its recipe, as a choice for the mod played over it.
    fn option(&self) -> RecipeOption {
        RecipeOption {
            recipe: self.recipe.clone(),
            origin: self.name.clone(),
            game_versions: Vec::new(),
            version_ids: Vec::new(),
            missing_files: Vec::new(),
            base_mod: None,
            made_for: None,
        }
    }
}

/// The installed mod `mod_id` for a mod of `game_id` to be played over: same
/// game, and not played over another one itself.
fn base_of(db: &Db, mod_id: &str, game_id: &str) -> Result<Base> {
    type Row = (String, String, String, Option<String>, Option<String>, bool);
    let (name, game, recipe, requires, extends, standalone): Row = db
        .conn()
        .query_row(
            "SELECT name, game_id, recipe_json, requires_json, extends, COALESCE(standalone, 0)
             FROM mods WHERE id = ?1",
            [mod_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))?;
    if game != game_id || extends.is_some() {
        return Err(Error::IncompatibleInstall(format!(
            "it can't be played over {name}"
        )));
    }
    Ok(Base {
        name,
        recipe: serde_json::from_str(&recipe).unwrap_or(Recipe::Episode),
        requires: parse_requires(requires.as_deref()),
        standalone,
    })
}

/// Whether the mod can be played on `install`. Installs of an unknown
/// version are allowed: the catalog doesn't know every release. A known
/// version the patch file doesn't name is refused, even when the catalog
/// doesn't know the named ones (e.g. a CGA-only patch on the EGA release).
pub(super) fn check_install(game_id: &str, option: &RecipeOption, install: &Install) -> Result<()> {
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

pub(super) fn slug(name: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::KeenwikiOrigin;

    fn batch(file: &str) -> Recipe {
        Recipe::Batch { file: file.into() }
    }

    fn option(recipe: Recipe) -> RecipeOption {
        RecipeOption {
            recipe,
            origin: "PLAY.BAT".into(),
            game_versions: vec!["1.4".into()],
            version_ids: vec!["keen4-1.4-ega".into()],
            missing_files: Vec::new(),
            base_mod: None,
            made_for: None,
        }
    }

    fn info(recipes: Vec<RecipeOption>) -> ModInfo {
        ModInfo {
            name: "Aurora".into(),
            authors: vec!["From the files".into()],
            version: Some("1.0".into()),
            released: Some("2020-01-01".into()),
            game_id: "keen4".into(),
            recipes,
            confident: true,
            file_count: 3,
            save_count: 0,
            index_id: None,
            index_version: None,
            index_files: false,
            made_for: None,
            bundled_game: None,
            complete: false,
            standalone: false,
        }
    }

    fn base() -> Base {
        Base {
            name: "Lego Keen".into(),
            recipe: batch("LEGO.BAT"),
            requires: None,
            standalone: false,
        }
    }

    #[test]
    fn picks_the_players_recipe_or_the_one_of_the_mod_below() {
        let own = info(vec![option(batch("PLAY.BAT")), option(Recipe::Episode)]);
        let picked = |info: &ModInfo, base: Option<&Base>, recipe: &Recipe| {
            picked_option(info, base, recipe).map(|o| (o.recipe, o.origin))
        };
        assert_eq!(
            picked(&own, None, &batch("PLAY.BAT")).unwrap(),
            (batch("PLAY.BAT"), "PLAY.BAT".into())
        );
        // Levels alone start as the mod below does, even where they could
        // start the episode themselves.
        assert_eq!(
            picked(&own, Some(&base()), &Recipe::Episode).unwrap(),
            (batch("LEGO.BAT"), "Lego Keen".into())
        );
        assert_eq!(
            picked(&own, Some(&base()), &batch("PLAY.BAT")).unwrap().0,
            batch("PLAY.BAT")
        );
        // The mod below's recipe may be picked though the files don't offer it.
        assert_eq!(
            picked(&info(Vec::new()), Some(&base()), &batch("LEGO.BAT"))
                .unwrap()
                .0,
            batch("LEGO.BAT")
        );
        for (base, recipe) in [
            (None, batch("LEGO.BAT")),
            (Some(base()), batch("OTHER.BAT")),
        ] {
            assert!(matches!(
                picked(&own, base.as_ref(), &recipe),
                Err(Error::IncompatibleInstall(_))
            ));
        }
    }

    fn request(keenwiki: Option<KeenwikiOrigin>) -> InstallModRequest {
        InstallModRequest {
            path: "C:/mods/aurora.zip".into(),
            name: String::new(),
            recipe: batch("PLAY.BAT"),
            install_id: None,
            keenwiki,
            extends: Some("lego".into()),
            replace: None,
        }
    }

    fn page() -> KeenwikiOrigin {
        KeenwikiOrigin {
            page_url: "https://keenwiki.shikadi.net/wiki/Aurora".into(),
            download_url: "https://example.com/aurora.zip".into(),
            game_id: None,
            authors: vec!["From the page".into()],
            author_links: BTreeMap::from([("From the page".into(), "https://a".into())]),
            version: Some("2.0".into()),
            description: Some("Levels.".into()),
            released: None,
            homepage: Some("https://home".into()),
            pckf: None,
            keenmodding: Some("https://km".into()),
            status: Some("Alpha".into()),
        }
    }

    #[test]
    fn a_picked_mod_keeps_what_its_page_says_over_the_guesses() {
        let info = info(Vec::new());
        let stored = |req| Stored::picked(req, &info, "Aurora".into(), batch("PLAY.BAT"), None);

        let manual = request(None);
        let s = stored(&manual);
        assert_eq!(s.source, ModSource::Manual);
        assert_eq!(s.authors, ["From the files"]);
        assert_eq!(s.version.as_deref(), Some("1.0"));
        assert_eq!(s.source_url, "C:/mods/aurora.zip");
        assert_eq!(s.extends.as_deref(), Some("lego"));
        assert!(s.keenwiki_url.is_none() && s.description.is_none() && s.homepage.is_none());

        let from_page = request(Some(page()));
        let s = stored(&from_page);
        assert_eq!(s.source, ModSource::Keenwiki);
        assert_eq!(s.authors, ["From the page"]);
        assert_eq!(s.author_links.len(), 1);
        assert_eq!(s.version.as_deref(), Some("2.0"));
        // What the page doesn't say comes from the files.
        assert_eq!(s.released.as_deref(), Some("2020-01-01"));
        assert_eq!(s.description.as_deref(), Some("Levels."));
        assert_eq!(s.keenwiki_url, Some(page().page_url.as_str()));
        assert_eq!(s.source_url, "https://example.com/aurora.zip");
        assert_eq!(
            (
                s.homepage.as_deref(),
                s.pckf.as_deref(),
                s.keenmodding.as_deref()
            ),
            (Some("https://home"), None, Some("https://km"))
        );
        assert_eq!(s.status, Some(ModStatus::Alpha));

        // A page without authors leaves the guessed ones.
        let unnamed = request(Some(KeenwikiOrigin {
            authors: Vec::new(),
            ..page()
        }));
        assert_eq!(stored(&unnamed).authors, ["From the files"]);
    }

    #[test]
    fn a_new_version_keeps_what_its_files_cant_tell() {
        let info = info(Vec::new());
        let old = Mod {
            id: "aurora".into(),
            game_id: "keen4".into(),
            name: "Aurora".into(),
            authors: vec!["Old".into()],
            author_links: BTreeMap::from([("Old".into(), "https://old".into())]),
            version: Some("1.0".into()),
            description: Some("Old text.".into()),
            released: None,
            recipe: Recipe::Episode,
            source: ModSource::Keenwiki,
            index_id: None,
            extends: None,
            keenwiki_url: Some("https://keenwiki.shikadi.net/wiki/Old".into()),
            homepage: Some("https://old-home".into()),
            pckf: Some("https://pckf".into()),
            keenmodding: None,
            status: Some(ModStatus::Beta),
            added_at: 0,
            profile_id: "aurora".into(),
        };

        let manual = request(None);
        let s =
            Stored::picked(&manual, &info, "Aurora".into(), batch("PLAY.BAT"), None).keeping(&old);
        assert_eq!(s.author_links, old.author_links);
        assert_eq!(s.description, old.description);
        assert_eq!(s.keenwiki_url, old.keenwiki_url.as_deref());
        assert_eq!(s.homepage, old.homepage);
        assert_eq!(s.pckf, old.pckf);
        assert_eq!(s.status, old.status);
        // What the new files tell wins.
        assert_eq!(s.authors, ["From the files"]);
        assert_eq!(s.source, ModSource::Manual);

        // What the new page says wins too.
        let from_page = request(Some(page()));
        let s = Stored::picked(&from_page, &info, "Aurora".into(), batch("PLAY.BAT"), None)
            .keeping(&old);
        assert_eq!(s.author_links.len(), 1);
        assert!(s.author_links.contains_key("From the page"));
        assert_eq!(s.description.as_deref(), Some("Levels."));
        assert_eq!(s.keenwiki_url, Some(page().page_url.as_str()));
        assert_eq!(s.homepage.as_deref(), Some("https://home"));
        assert_eq!(s.pckf, old.pckf);
        assert_eq!(s.status, Some(ModStatus::Alpha));
    }
}

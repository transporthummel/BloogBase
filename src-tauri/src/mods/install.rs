//! Storing a mod: from a confirmed inspection or an index entry, as a new
//! mod or an update in place.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use rusqlite::{OptionalExtension, params};

use crate::catalog::{self, Game};
use crate::db::Db;
use crate::detect::sha256_hex;
use crate::error::{Error, Result};
use crate::library::{self, Install, now};
use crate::mod_index::IndexEntry;
use crate::paths::AppPaths;
use crate::recipe::{Recipe, VersionRecipes};
use crate::source::{Source, split_dir};

use super::analyze::{Analysis, analyze};
use super::fit::{best_install, fits, needed_game, parse_requires, rehome, requires};
use super::queries::list;
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
    keenwiki_url: Option<&'a str>,
    homepage: Option<String>,
    pckf: Option<String>,
    keenmodding: Option<String>,
    /// The picked folder or ZIP, or the download URL.
    source_url: &'a str,
    /// Name for the kept copy of the archive; default: its own.
    archive_name: Option<&'a str>,
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
            keenwiki_url: c.keenwiki_url.as_deref(),
            // The index entry links it.
            homepage: None,
            pckf: None,
            keenmodding: None,
            source_url: &c.url,
            archive_name: c.url.rsplit('/').next().filter(|n| !n.is_empty()),
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
            keenwiki_url: page.map(|o| o.page_url.as_str()),
            homepage: page.and_then(|o| o.homepage.clone()),
            pckf: page.and_then(|o| o.pckf.clone()),
            keenmodding: page.and_then(|o| o.keenmodding.clone()),
            source_url: page.map_or(&req.path, |o| &o.download_url),
            archive_name: None,
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
    // else where the mod below plays.
    let requires = if option.game_versions.is_empty() {
        base.as_ref().and_then(|b| b.requires.clone())
    } else {
        Some(option.version_ids.clone())
    };
    let target = match &req.install_id {
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
    let stored = Stored::picked(req, &analysis.info, name, option.recipe, requires);
    match &replaced {
        None => store(paths, db, &source, &analysis, &stored, target.as_ref()),
        Some(old) => update(
            paths,
            db,
            &old.id,
            &source,
            &analysis,
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

/// The mod a new version replaces, which must be for the same game.
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
    pub keenwiki_url: Option<String>,
    pub url: String,
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
            recipe: entry.recipe.clone(),
            version_recipes: entry.version_recipes.clone(),
            copy_to_root: entry.copy_to_root.clone(),
            extends: entry.extends.clone(),
            requires: entry.requires.clone(),
            keenwiki_url: entry.links.keenwiki.clone(),
            url: d.url.clone(),
        }
    }
}

/// Ties an installed mod to the index entry whose download it was
/// installed from, and takes the entry's details over what the mod brought
/// along, as an install from the index would store them. How it's played
/// stays as installed: the entry's recipe is made for its newest download.
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
        keenwiki_url,
        homepage,
        pckf,
        keenmodding,
        source_url: _,
        archive_name: _,
    } = Stored::curated(c, None);
    db.conn().execute(
        "UPDATE mods SET name = ?2, authors = ?3, author_links = ?4,
                         version = COALESCE(?5, version), description = ?6, released = ?7,
                         source = ?8, index_id = ?9, keenwiki_url = ?10, homepage = ?11,
                         pckf = ?12, keenmodding = ?13
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
        ],
    )?;
    tracing::info!("tied mod {mod_id} to index entry {}", c.index_id);
    Ok(())
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
    let base_id = match &c.extends {
        Some(base) => {
            Some(find_by_index(db, base)?.ok_or_else(|| Error::MissingGame(base.clone()))?)
        }
        None => None,
    };
    check_recipe_files(paths, &analysis, c, base_id.as_deref())?;
    analysis.copies = copies_to_root(&analysis, &c.copy_to_root)?;

    let stored = Stored::curated(c, base_id);
    let game = game(&c.game_id)?;
    let requires = stored.requires.as_deref();
    if let Some(mod_id) = find_by_index(db, &c.index_id)? {
        let target = updated_install(db, &mod_id, game, requires)?;
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
    store(paths, db, &source, &analysis, &stored, target.as_ref())
}

/// Checks that the files the index recipes run are in the mod's root, or in
/// the one of `base_id`, the mod it's played over.
fn check_recipe_files(
    paths: &AppPaths,
    analysis: &Analysis,
    c: &Curated,
    base_id: Option<&str>,
) -> Result<()> {
    let mut in_root: Vec<String> = analysis
        .files
        .iter()
        .filter(|f| split_dir(f).0 == analysis.root)
        .map(|f| split_dir(f).1.to_string())
        .collect();
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
    if let Source::Zip(zip, _) = source {
        fs::create_dir_all(staging.join("source"))?;
        // The name ends the download's URL; only its last part, so a `\..\`
        // in it can't leave the folder on Windows.
        let name = archive_name
            .and_then(|n| Path::new(n).file_name())
            .unwrap_or_else(|| zip.file_name().expect("a file"));
        fs::copy(zip, staging.join("source").join(name))?;
    }
    Ok(())
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
            extends: s.extends.clone(),
            keenwiki_url: s.keenwiki_url.map(str::to_string),
            homepage: s.homepage.clone(),
            pckf: s.pckf.clone(),
            keenmodding: s.keenmodding.clone(),
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
                               keenmodding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19, ?20, ?21, ?22)",
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
                version_recipes_json(s),
                s.extends,
                s.homepage,
                s.pckf,
                s.keenmodding
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
                             version_recipes_json = ?15, extends = ?16, homepage = ?17,
                             pckf = ?18, keenmodding = ?19
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
                version_recipes_json(s),
                s.extends,
                s.homepage,
                s.pckf,
                s.keenmodding
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

/// An installed mod another one is played over.
struct Base {
    name: String,
    recipe: Recipe,
    requires: Option<Vec<String>>,
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
    type Row = (String, String, String, Option<String>, Option<String>);
    let (name, game, recipe, requires, extends): Row = db
        .conn()
        .query_row(
            "SELECT name, game_id, recipe_json, requires_json, extends FROM mods WHERE id = ?1",
            [mod_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
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

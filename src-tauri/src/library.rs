//! Installed games: import from a folder or ZIP, listing (PLAN.md §4).

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::params;
use serde::Serialize;
use specta::Type;

use crate::backups::{self, Contents, Reason};
use crate::catalog::{self, Game};
use crate::covers;
use crate::db::Db;
use crate::detect::{self, Detection, detect};
use crate::error::{Error, Result};
use crate::mods;
use crate::paths::AppPaths;
use crate::source::{Source, split_dir};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum InstallSource {
    Folder,
    Zip,
    Download,
    Steam,
    Gog,
}

impl InstallSource {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallSource::Folder => "folder",
            InstallSource::Zip => "zip",
            InstallSource::Download => "download",
            InstallSource::Steam => "steam",
            InstallSource::Gog => "gog",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "folder" => InstallSource::Folder,
            "zip" => InstallSource::Zip,
            "download" => InstallSource::Download,
            "steam" => InstallSource::Steam,
            "gog" => InstallSource::Gog,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Install {
    pub id: String,
    pub game_id: String,
    pub version_id: Option<String>,
    pub label: String,
    pub source: InstallSource,
    pub path: String,
    /// Unix epoch seconds.
    #[specta(type = u32)]
    pub added_at: i64,
}

/// Imports every game found in a folder or ZIP the user picked.
pub fn import(paths: &AppPaths, db: &Db, path: &Path) -> Result<Vec<Install>> {
    let kind = match Source::open(path)? {
        Source::Folder(_) => InstallSource::Folder,
        Source::Zip(_) => InstallSource::Zip,
    };
    import_from(paths, db, path, kind, &path.display().to_string(), None)
}

/// Imports every game found in `path`. Each game becomes an install with a
/// vanilla play profile that receives the saves found next to it. `origin`
/// records where the files came from (a path or download URL); `only`
/// limits the import to these game ids.
pub fn import_from(
    paths: &AppPaths,
    db: &Db,
    path: &Path,
    kind: InstallSource,
    origin: &str,
    only: Option<&[String]>,
) -> Result<Vec<Install>> {
    let source = Source::open(path)?;
    let mut detections = detect(&source)?;
    if let Some(only) = only {
        detections.retain(|d| only.contains(&d.game_id));
    }
    if detections.is_empty() {
        return Err(Error::NoGamesFound);
    }
    // The same release twice only makes a second copy of the same files.
    let mut have = release_keys(paths, db)?;
    let mut skipped = Vec::new();
    let mut fresh = Vec::new();
    for d in detections {
        let game = catalog::game(&d.game_id).expect("detections come from the catalog");
        let key = release_key(game, d.version_id.as_deref(), |name| {
            d.base_files
                .iter()
                .find(|rel| split_dir(rel).1.eq_ignore_ascii_case(name))
                .map(|rel| source.read(rel))
                .transpose()
        })?;
        if key.is_some_and(|k| !have.insert(k)) {
            skipped.push(release_name(game, &d));
        } else {
            fresh.push(d);
        }
    }
    if fresh.is_empty() {
        return Err(Error::AlreadyInLibrary(skipped.join(", ")));
    }
    for name in &skipped {
        tracing::info!("not imported again: {name}");
    }
    let detections = fresh;
    let installs = detections
        .iter()
        .map(|d| import_one(paths, db, &source, kind, origin, d))
        .collect::<Result<Vec<_>>>()?;
    // Mods waiting for one of these games can be played now.
    if let Err(e) = mods::rehome(db, None) {
        tracing::warn!("could not put waiting mods on the new games: {e}");
    }
    Ok(installs)
}

/// What tells releases apart: the catalog version, else the executable.
fn release_key(
    game: &Game,
    version_id: Option<&str>,
    read: impl Fn(&str) -> Result<Option<Vec<u8>>>,
) -> Result<Option<String>> {
    if let Some(v) = version_id {
        return Ok(Some(v.to_string()));
    }
    for exe in &game.executables {
        if let Some(data) = read(exe)? {
            return Ok(Some(format!("{} {}", game.id, detect::sha256_hex(&data))));
        }
    }
    Ok(None)
}

/// The release keys of everything in the library.
fn release_keys(paths: &AppPaths, db: &Db) -> Result<HashSet<String>> {
    let mut keys = HashSet::new();
    for install in list(db)? {
        let Some(game) = catalog::game(&install.game_id) else {
            continue;
        };
        let base = paths.library.join(&install.id).join("base");
        let read = |name: &str| -> Result<Option<Vec<u8>>> {
            let Ok(entries) = fs::read_dir(&base) else {
                return Ok(None);
            };
            entries
                .flatten()
                .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
                .map(|e| fs::read(e.path()).map_err(Error::from))
                .transpose()
        };
        keys.extend(release_key(game, install.version_id.as_deref(), read)?);
    }
    Ok(keys)
}

fn release_name(game: &Game, d: &Detection) -> String {
    match d
        .version_id
        .as_deref()
        .and_then(|id| game.versions.iter().find(|v| v.id == id))
    {
        Some(v) => format!("{} {}", game.title, v.label),
        None => game.title.clone(),
    }
}

fn import_one(
    paths: &AppPaths,
    db: &Db,
    source: &Source,
    kind: InstallSource,
    origin: &str,
    d: &Detection,
) -> Result<Install> {
    let game = catalog::game(&d.game_id).expect("detections come from the catalog");
    let id = reserve_id(paths, &game.id)?;
    let install_dir = paths.library.join(&id);
    let profile_dir = paths.profiles.join(&id);
    let staging = paths.cache.join(format!("import-{id}"));

    let result = (|| {
        // Copy into a staging dir first so a failed import leaves nothing half-done.
        let _ = fs::remove_dir_all(&staging);
        for (files, sub) in [(&d.base_files, "base"), (&d.state_files, "state")] {
            fs::create_dir_all(staging.join(sub))?;
            for rel in files {
                source.copy_to(rel, &staging.join(sub).join(split_dir(rel).1))?;
            }
        }
        fs::create_dir(&profile_dir)?;
        fs::rename(staging.join("base"), install_dir.join("base"))?;
        fs::rename(staging.join("state"), profile_dir.join("state"))?;

        let install = Install {
            id: id.clone(),
            game_id: game.id.clone(),
            version_id: d.version_id.clone(),
            label: game.title.clone(),
            source: kind,
            path: install_dir.display().to_string(),
            added_at: now(),
        };
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO installs (id, game_id, version_id, label, source, path, added_at, origin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                install.id,
                install.game_id,
                install.version_id,
                install.label,
                install.source.as_str(),
                install.path,
                install.added_at,
                origin
            ],
        )?;
        tx.execute(
            "INSERT INTO profiles (id, install_id, state_path, created_at) VALUES (?1, ?1, ?2, ?3)",
            params![
                id,
                profile_dir.join("state").display().to_string(),
                install.added_at
            ],
        )?;
        tx.commit()?;
        Ok(install)
    })();

    let _ = fs::remove_dir_all(&staging);
    match &result {
        Ok(install) => tracing::info!(
            "imported {} ({}) from {}: {} files, {} state files",
            install.id,
            install.version_id.as_deref().unwrap_or("unknown version"),
            d.dir,
            d.base_files.len(),
            d.state_files.len()
        ),
        Err(err) => {
            tracing::warn!("import of {} failed, cleaning up: {err}", game.id);
            let _ = fs::remove_dir_all(&install_dir);
            let _ = fs::remove_dir_all(&profile_dir);
        }
    }
    result
}

/// Picks the first free `<game>-<n>` and claims it by creating its install
/// directory, which is atomic even across concurrent imports.
fn reserve_id(paths: &AppPaths, game_id: &str) -> Result<String> {
    for n in 1.. {
        let id = format!("{game_id}-{n}");
        // Never adopt a leftover profile: its saves would end up in the wrong place.
        if paths.profiles.join(&id).exists() {
            continue;
        }
        match fs::create_dir(paths.library.join(&id)) {
            Ok(()) => return Ok(id),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    unreachable!()
}

/// Uninstalls a game: its files and its play profile, and with `with_mods`
/// the mods played on it. Other mods move to another install they fit, or
/// wait for one. With `keep_saves`, saves are archived first (§9.5). What
/// the game was imported from stays untouched.
pub fn remove(
    paths: &AppPaths,
    db: &Db,
    install_id: &str,
    keep_saves: bool,
    with_mods: bool,
) -> Result<()> {
    if !list(db)?.iter().any(|i| i.id == install_id) {
        return Err(Error::NotFound(format!("install {install_id}")));
    }
    let on_it = |mods: bool| -> Result<Vec<String>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM profiles WHERE install_id = ?1 AND mod_id IS {}NULL",
            if mods { "mod_id" } else { "id" },
            if mods { "NOT " } else { "" }
        ))?;
        Ok(stmt
            .query_map([install_id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    };
    if with_mods {
        for mod_id in on_it(true)? {
            mods::delete(paths, db, &mod_id, keep_saves)?;
        }
    }
    let game_profiles = on_it(false)?;
    if keep_saves {
        for profile_id in &game_profiles {
            backups::create(paths, db, profile_id, Contents::State, Reason::GameRemoved)?;
        }
    }
    {
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM profiles WHERE install_id = ?1 AND mod_id IS NULL",
            [install_id],
        )?;
        tx.execute(
            "UPDATE profiles SET install_id = NULL, composed_hash = NULL WHERE install_id = ?1",
            [install_id],
        )?;
        tx.execute("DELETE FROM installs WHERE id = ?1", [install_id])?;
        tx.commit()?;
    }
    mods::rehome(db, None)?;

    let mut dirs = vec![paths.library.join(install_id)];
    for profile_id in &game_profiles {
        dirs.push(paths.profiles.join(profile_id));
        dirs.push(paths.composed(profile_id));
        covers::remove(paths, profile_id);
    }
    for dir in dirs {
        if let Err(e) = fs::remove_dir_all(&dir)
            && e.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!("could not remove {}: {e}", dir.display());
        }
    }
    tracing::info!("removed {install_id} (saves kept: {keep_saves}, mods too: {with_mods})");
    Ok(())
}

/// Gives installs of an unknown version the catalog version their files now
/// match, e.g. after an update taught the catalog more releases.
pub fn identify_unknown(paths: &AppPaths, db: &Db) -> Result<()> {
    identify_unknown_with(paths, db, |id| catalog::game(id).cloned())
}

fn identify_unknown_with(
    paths: &AppPaths,
    db: &Db,
    game_of: impl Fn(&str) -> Option<catalog::Game>,
) -> Result<()> {
    let mut found = false;
    for install in list(db)?.into_iter().filter(|i| i.version_id.is_none()) {
        let Some(game) = game_of(&install.game_id) else {
            continue;
        };
        let base = paths.library.join(&install.id).join("base");
        let version = match Source::open(&base).and_then(|s| detect::identify(&s, &game)) {
            Ok(Some(v)) => v,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("could not check the version of {}: {e}", install.id);
                continue;
            }
        };
        db.conn().execute(
            "UPDATE installs SET version_id = ?1 WHERE id = ?2",
            params![version, install.id],
        )?;
        tracing::info!("{} is {version}", install.id);
        found = true;
    }
    if found {
        mods::rehome(db, None)?;
    }
    Ok(())
}

pub fn list(db: &Db) -> Result<Vec<Install>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, game_id, version_id, label, source, path, added_at
         FROM installs ORDER BY added_at, id",
    )?;
    let rows = stmt.query_map([], |r| {
        let source: String = r.get(4)?;
        Ok(Install {
            id: r.get(0)?,
            game_id: r.get(1)?,
            version_id: r.get(2)?,
            label: r.get(3)?,
            // The CHECK constraint guarantees a known value.
            source: InstallSource::parse(&source).expect("valid install source"),
            path: r.get(5)?,
            added_at: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Whether an install of `game_id` from `origin` is in the library.
pub fn has_origin(db: &Db, game_id: &str, origin: &str) -> Result<bool> {
    Ok(db.conn().query_row(
        "SELECT EXISTS (SELECT 1 FROM installs WHERE game_id = ?1 AND origin = ?2)",
        params![game_id, origin],
        |r| r.get(0),
    )?)
}

/// Whether the library has an install of this exact game version.
pub fn has_version(db: &Db, version_id: &str) -> Result<bool> {
    Ok(db.conn().query_row(
        "SELECT EXISTS (SELECT 1 FROM installs WHERE version_id = ?1)",
        [version_id],
        |r| r.get(0),
    )?)
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::testutil::*;

    fn setup() -> (tempfile::TempDir, AppPaths, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        (tmp, paths, Db::open_in_memory().unwrap())
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn identifies_unknown_versions_the_catalog_learned() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("Keen 4");
        write_folder(&src, KEEN4);
        let id = import(&paths, &db, &src).unwrap()[0].id.clone();
        let version = || list(&db).unwrap()[0].version_id.clone();
        assert_eq!(version(), None);

        let mut taught = catalog::game("keen4").unwrap().clone();
        let exe = fs::read(paths.library.join(&id).join("base/KEEN4E.EXE")).unwrap();
        taught.versions.truncate(1);
        taught.versions[0].files = [("KEEN4E.EXE".to_string(), detect::sha256_hex(&exe))].into();
        identify_unknown_with(&paths, &db, |_| None).unwrap();
        assert_eq!(version(), None);
        identify_unknown_with(&paths, &db, |_| Some(taught.clone())).unwrap();
        assert_eq!(version(), Some(taught.versions[0].id.clone()));
    }

    #[test]
    fn imports_folder_and_moves_state_into_profile() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("Keen 4");
        write_folder(&src, KEEN4);

        let installs = import(&paths, &db, &src).unwrap();
        assert_eq!(installs.len(), 1);
        let i = &installs[0];
        assert_eq!(
            (i.id.as_str(), i.game_id.as_str(), i.source),
            ("keen4-1", "keen4", InstallSource::Folder)
        );

        let base = paths.library.join("keen4-1/base");
        assert_eq!(
            names(&base),
            ["AUDIO.CK4", "EGAGRAPH.CK4", "GAMEMAPS.CK4", "KEEN4E.EXE"]
        );
        let state = paths.profiles.join("keen4-1/state");
        assert_eq!(names(&state), ["SAVEGAM0.CK4", "config.ck4"]);
        assert_eq!(fs::read(state.join("SAVEGAM0.CK4")).unwrap(), b"save");
        // The source stays untouched.
        assert!(src.join("SAVEGAM0.CK4").exists());
        // No staging leftovers.
        assert!(names(&paths.cache).is_empty());

        let listed = list(&db).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "keen4-1");
        let profile: String = db
            .conn()
            .query_row(
                "SELECT install_id FROM profiles WHERE id = 'keen4-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(profile, "keen4-1");
        assert!(has_origin(&db, "keen4", &src.display().to_string()).unwrap());
        assert!(!has_origin(&db, "keen5", &src.display().to_string()).unwrap());
    }

    #[test]
    fn import_from_records_kind_and_origin() {
        let (tmp, paths, db) = setup();
        let zip = tmp.path().join("4keen.zip");
        write_zip(&zip, KEEN4);
        let url = "https://example.invalid/4keen.zip";
        let installs = import_from(&paths, &db, &zip, InstallSource::Download, url, None).unwrap();
        assert_eq!(installs[0].source, InstallSource::Download);
        assert!(has_origin(&db, "keen4", url).unwrap());
        assert!(!has_version(&db, "keen4-1.4-ega").unwrap());
    }

    #[test]
    fn the_same_release_is_imported_once() {
        let (tmp, paths, db) = setup();
        let zip = tmp.path().join("keen4.zip");
        write_zip(&zip, KEEN4);
        import(&paths, &db, &zip).unwrap();
        assert!(matches!(
            import(&paths, &db, &zip),
            Err(Error::AlreadyInLibrary(name)) if name == "Commander Keen 4: Secret of the Oracle"
        ));

        // Another release of the same game gets an id of its own.
        let other: Vec<(&str, &[u8])> = KEEN4
            .iter()
            .map(|&(n, d)| {
                (
                    n,
                    if n == "KEEN4E.EXE" {
                        b"other".as_slice()
                    } else {
                        d
                    },
                )
            })
            .collect();
        let zip = tmp.path().join("other.zip");
        write_zip(&zip, &other);
        let second = import(&paths, &db, &zip).unwrap();
        assert_eq!(second[0].id, "keen4-2");
        assert_eq!(second[0].source, InstallSource::Zip);
        assert_eq!(list(&db).unwrap().len(), 2);
    }

    #[test]
    fn nothing_found() {
        let (tmp, paths, db) = setup();
        write_folder(tmp.path().join("empty").as_path(), &[("README.TXT", b"")]);
        assert!(matches!(
            import(&paths, &db, &tmp.path().join("empty")),
            Err(Error::NoGamesFound)
        ));
    }

    #[test]
    fn failed_import_cleans_up() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        // A conflicting row makes the import fail after the files are in place.
        db.conn()
            .execute(
                "INSERT INTO installs (id, game_id, label, source, path, added_at)
                 VALUES ('keen4-1', 'keen4', 'x', 'folder', '/x', 0)",
                [],
            )
            .unwrap();
        assert!(import(&paths, &db, &src).is_err());
        assert!(!paths.library.join("keen4-1").exists());
        assert!(!paths.profiles.join("keen4-1").exists());
        assert!(names(&paths.cache).is_empty());
    }

    #[test]
    fn removing_a_game_keeps_saves_and_takes_its_mods_along_if_asked() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        import(&paths, &db, &src).unwrap();
        let level_pack = tmp.path().join("levels");
        write_folder(&level_pack, &[("GAMEMAPS.CK4", b"maps")]);
        let info = mods::inspect(&level_pack, None).unwrap();
        let m = mods::install(
            &paths,
            &db,
            &mods::InstallModRequest {
                path: level_pack.display().to_string(),
                name: String::new(),
                recipe: info.recipes[0].recipe.clone(),
                install_id: Some("keen4-1".into()),
                keenwiki: None,
                replace: None,
            },
        )
        .unwrap();
        fs::write(paths.composed("keen4-1").join("x"), b"")
            .or_else(|_| {
                fs::create_dir_all(paths.composed("keen4-1"))?;
                fs::write(paths.composed("keen4-1").join("x"), b"")
            })
            .unwrap();

        remove(&paths, &db, "keen4-1", true, true).unwrap();
        assert!(list(&db).unwrap().is_empty());
        assert!(mods::list(&db).unwrap().is_empty());
        for dir in [
            paths.library.join("keen4-1"),
            paths.profiles.join("keen4-1"),
            paths.composed("keen4-1"),
            paths.mods.join(&m.id),
        ] {
            assert!(!dir.exists(), "{}", dir.display());
        }
        // The imported folder stays, the saves are archived.
        assert!(src.join("SAVEGAM0.CK4").is_file());
        let saved: Vec<String> = db
            .conn()
            .prepare("SELECT profile_id FROM save_backups ORDER BY profile_id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(saved, ["keen4-1"]);
        assert!(matches!(
            remove(&paths, &db, "keen4-1", false, false),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn skips_ids_with_leftover_profiles() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        fs::create_dir_all(paths.profiles.join("keen4-1/state")).unwrap();
        assert_eq!(import(&paths, &db, &src).unwrap()[0].id, "keen4-2");
        assert!(paths.profiles.join("keen4-1/state").exists());
    }
}

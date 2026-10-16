//! Installed games: import from a folder or ZIP, listing (PLAN.md §4).

use std::fs;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::params;
use serde::Serialize;
use specta::Type;

use crate::catalog;
use crate::db::Db;
use crate::detect::{Detection, detect};
use crate::error::{Error, Result};
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
    fn as_str(self) -> &'static str {
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

/// Imports every game found in `path`. Each game becomes an install with a
/// vanilla play profile that receives the saves found next to it.
pub fn import(paths: &AppPaths, db: &Db, path: &Path) -> Result<Vec<Install>> {
    let source = Source::open(path)?;
    let kind = match source {
        Source::Folder(_) => InstallSource::Folder,
        Source::Zip(_) => InstallSource::Zip,
    };
    let detections = detect(&source)?;
    if detections.is_empty() {
        return Err(Error::NoGamesFound);
    }
    detections
        .iter()
        .map(|d| import_one(paths, db, &source, kind, d))
        .collect()
}

fn import_one(
    paths: &AppPaths,
    db: &Db,
    source: &Source,
    kind: InstallSource,
    d: &Detection,
) -> Result<Install> {
    let game = catalog::game(&d.game_id).expect("detections come from the catalog");
    let id = reserve_id(paths, &game.id)?;
    let install_dir = paths.library.join(&id);
    let profile_dir = paths.profiles.join(&id);
    let staging = paths.cache.join(format!("import-{id}"));

    let result =
        (|| {
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
                "INSERT INTO installs (id, game_id, version_id, label, source, path, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    install.id,
                    install.game_id,
                    install.version_id,
                    install.label,
                    install.source.as_str(),
                    install.path,
                    install.added_at
                ],
            )?;
            tx.execute(
            "INSERT INTO profiles (id, install_id, state_path, created_at) VALUES (?1, ?1, ?2, ?3)",
            params![id, profile_dir.join("state").display().to_string(), install.added_at],
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

fn now() -> i64 {
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
    }

    #[test]
    fn importing_twice_gets_a_new_id() {
        let (tmp, paths, db) = setup();
        let zip = tmp.path().join("keen4.zip");
        write_zip(&zip, KEEN4);
        import(&paths, &db, &zip).unwrap();
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
    fn skips_ids_with_leftover_profiles() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        fs::create_dir_all(paths.profiles.join("keen4-1/state")).unwrap();
        assert_eq!(import(&paths, &db, &src).unwrap()[0].id, "keen4-2");
        assert!(paths.profiles.join("keen4-1/state").exists());
    }
}

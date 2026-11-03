//! SQLite connection and schema migrations.

use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

use crate::error::Result;

/// Append-only: never edit a migration that has shipped, add a new one instead.
const MIGRATIONS_SLICE: &[M<'_>] = &[
    M::up(include_str!("../migrations/0001_initial.sql")),
    M::up(include_str!("../migrations/0002_install_origin.sql")),
    M::up(include_str!("../migrations/0003_backup_hash.sql")),
    M::up(include_str!("../migrations/0004_mod_details.sql")),
    M::up(include_str!("../migrations/0005_author_links.sql")),
    M::up(include_str!("../migrations/0006_mods_without_game.sql")).foreign_key_check(),
    M::up(include_str!("../migrations/0007_backup_files.sql")),
    M::up(include_str!("../migrations/0008_version_recipes.sql")),
    M::up(include_str!("../migrations/0009_extends.sql")),
    M::up(include_str!("../migrations/0010_homepage.sql")),
    M::up(include_str!("../migrations/0011_pckf.sql")),
    M::up(include_str!("../migrations/0012_keenmodding.sql")),
    M::up(include_str!("../migrations/0013_session_setup.sql")),
    M::up(include_str!("../migrations/0014_cover.sql")),
];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATIONS_SLICE);

/// The database, and the data folder it lies in: paths inside that folder are
/// stored relative to it ([`Db::store`], [`Db::resolve`]), so the folder can
/// be moved, copied or restored somewhere else.
pub struct Db {
    conn: Mutex<Connection>,
    root: Option<PathBuf>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let root = path.parent().map(Path::to_path_buf);
        Self::init(Connection::open(path)?, root)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, None)
    }

    fn init(mut conn: Connection, root: Option<PathBuf>) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
        // Off while migrating: rebuilding a table must not cascade into the
        // tables that point at it.
        conn.pragma_update(None, "foreign_keys", "OFF")?;
        MIGRATIONS.to_latest(&mut conn)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        relocate(&mut conn)?;
        Ok(Db {
            conn: Mutex::new(conn),
            root,
        })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock can't leave SQLite in a broken state,
        // so a poisoned mutex is safe to keep using.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// How `path` goes into the database: relative to the data folder, with
    /// `/` between its parts, when it lies inside it; else as it is (a DOSBox
    /// of the player's own).
    pub fn store(&self, path: &Path) -> String {
        match self
            .root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
        {
            Some(rel) if rel.components().all(|c| matches!(c, Component::Normal(_))) => rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
            _ => path.display().to_string(),
        }
    }

    /// The path a stored one stands for: relative ones inside the data folder.
    pub fn resolve(&self, stored: &str) -> PathBuf {
        let path = Path::new(stored);
        match &self.root {
            Some(root) if !path.is_absolute() => stored
                .split('/')
                .fold(root.clone(), |dir, part| dir.join(part)),
            _ => path.to_path_buf(),
        }
    }

    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn()
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }
}

/// Whether the database at `path` holds anything of the player's: games,
/// mods, profiles or backups. Opened read-only, without migrating.
pub fn holds_anything(path: &Path) -> Result<bool> {
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut any = false;
    for table in ["installs", "mods", "profiles", "save_backups"] {
        let exists: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |r| r.get(0),
        )?;
        if exists {
            any |= conn.query_row(&format!("SELECT EXISTS (SELECT 1 FROM {table})"), [], |r| {
                r.get::<_, bool>(0)
            })?;
        }
    }
    Ok(any)
}

/// Where each table keeps paths inside the data folder: the rows to look at
/// (not a player's own DOSBox) and the path's parts from the data folder on,
/// `None` for any name. A DOSBox's executable may lie further down.
struct StoredPath {
    table: &'static str,
    column: &'static str,
    rows: &'static str,
    parts: &'static [Option<&'static str>],
    deeper: bool,
}

const STORED_PATHS: &[StoredPath] = &[
    StoredPath {
        table: "installs",
        column: "path",
        rows: "TRUE",
        parts: &[Some("library"), None],
        deeper: false,
    },
    StoredPath {
        table: "mods",
        column: "path",
        rows: "TRUE",
        parts: &[Some("mods"), None],
        deeper: false,
    },
    StoredPath {
        table: "profiles",
        column: "state_path",
        rows: "TRUE",
        parts: &[Some("profiles"), None, Some("state")],
        deeper: false,
    },
    StoredPath {
        table: "save_backups",
        column: "archive_path",
        rows: "TRUE",
        parts: &[Some("backups"), None, None],
        deeper: false,
    },
    StoredPath {
        table: "runtimes",
        column: "path",
        rows: "managed = 1",
        parts: &[Some("runtimes"), Some(crate::runtime::KIND), None, None],
        deeper: true,
    },
];

/// Makes the absolute paths of earlier versions relative to the data folder
/// they were written in, whatever folder that was: a data folder restored or
/// moved elsewhere finds its games, mods, saves and DOSBox again.
fn relocate(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    for stored in STORED_PATHS {
        let StoredPath {
            table,
            column,
            rows,
            ..
        } = stored;
        let found: Vec<(i64, String)> = tx
            .prepare(&format!("SELECT rowid, {column} FROM {table} WHERE {rows}"))?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (rowid, path) in found {
            if let Some(rel) = relative_part(&path, stored) {
                tx.execute(
                    &format!("UPDATE {table} SET {column} = ?1 WHERE rowid = ?2"),
                    rusqlite::params![rel, rowid],
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

/// The part of an absolute path from the data folder on, if its parts from
/// there fit `stored`.
fn relative_part(path: &str, stored: &StoredPath) -> Option<String> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return None;
    }
    let names: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let parts = stored.parts;
    let fits = |at: &usize| {
        let rest = names.len() - at;
        (if stored.deeper {
            rest >= parts.len()
        } else {
            rest == parts.len()
        }) && parts
            .iter()
            .zip(&names[*at..])
            .all(|(part, name)| part.is_none_or(|p| p.eq_ignore_ascii_case(name)))
    };
    // The last place that fits, in case the data folder lies below a folder
    // named like one in it.
    let at = (0..names.len()).rev().find(fits)?;
    Some(names[at..].join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        MIGRATIONS.validate().unwrap();
    }

    #[test]
    fn opens_and_migrates() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS_SLICE.len() as u32);
    }

    #[test]
    fn reopening_a_file_db_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.db");
        drop(Db::open(&path).unwrap());
        let db = Db::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS_SLICE.len() as u32);
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let db = Db::open_in_memory().unwrap();
        let res = db.conn().execute(
            "INSERT INTO profiles (id, install_id, state_path, created_at)
             VALUES ('p1', 'missing', 'x', 0)",
            [],
        );
        assert!(res.is_err());
    }

    #[test]
    fn mods_without_a_game_keep_their_play_sessions_through_the_migration() {
        let mut conn = Connection::open_in_memory().unwrap();
        MIGRATIONS.to_version(&mut conn, 5).unwrap();
        conn.execute_batch(
            "INSERT INTO installs (id, game_id, label, source, path, added_at)
                 VALUES ('i1', 'keen4', 'Keen 4', 'folder', '/x', 0);
             INSERT INTO mods (id, game_id, name, authors, recipe_json, path, source, added_at)
                 VALUES ('m1', 'keen4', 'Mod', '[]', '{}', '/m', 'manual', 0);
             INSERT INTO profiles (id, install_id, mod_id, state_path, created_at)
                 VALUES ('m1', 'i1', 'm1', '/m/state', 0);
             INSERT INTO play_sessions (profile_id, started_at, ended_at) VALUES ('m1', 1, 2);",
        )
        .unwrap();
        let db = Db::init(conn, None).unwrap();
        let conn = db.conn();
        let sessions: i64 = conn
            .query_row("SELECT session_count FROM profile_stats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sessions, 1);
        conn.execute("UPDATE profiles SET install_id = NULL", [])
            .unwrap();
        // A game's own profile always has its install.
        assert!(
            conn.execute(
                "INSERT INTO profiles (id, state_path, created_at) VALUES ('p', '/p', 0)",
                [],
            )
            .is_err()
        );
        // Foreign keys are back on after migrating.
        assert!(
            conn.execute("UPDATE profiles SET install_id = 'gone'", [])
                .is_err()
        );
    }

    #[test]
    fn profile_stats_aggregates_finished_sessions() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.conn();
        conn.execute_batch(
            "INSERT INTO installs (id, game_id, label, source, path, added_at)
                 VALUES ('i1', 'keen4', 'Keen 4', 'folder', '/x', 0);
             INSERT INTO profiles (id, install_id, state_path, created_at)
                 VALUES ('p1', 'i1', '/x/state', 0);
             INSERT INTO play_sessions (profile_id, started_at, ended_at) VALUES ('p1', 100, 160);
             INSERT INTO play_sessions (profile_id, started_at, ended_at) VALUES ('p1', 200, 230);
             INSERT INTO play_sessions (profile_id, started_at) VALUES ('p1', 300);",
        )
        .unwrap();
        let (count, seconds, last): (i64, i64, i64) = conn
            .query_row(
                "SELECT session_count, play_seconds, last_played_at
                 FROM profile_stats WHERE profile_id = 'p1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((count, seconds, last), (3, 90, 300));
    }

    #[test]
    fn stores_paths_inside_the_data_folder_relative_to_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        std::fs::create_dir(&root).unwrap();
        let db = Db::open(&root.join("bloogbase.db")).unwrap();
        let state = root.join("profiles").join("keen4-1").join("state");
        assert_eq!(db.store(&state), "profiles/keen4-1/state");
        assert_eq!(db.resolve("profiles/keen4-1/state"), state);
        // A DOSBox of the player's own stays where it is.
        let own = tmp.path().join("dosbox").join("dosbox.exe");
        assert_eq!(db.store(&own), own.display().to_string());
        assert_eq!(db.resolve(&own.display().to_string()), own);
    }

    #[test]
    fn finds_a_moved_data_folder_written_with_absolute_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("BloogBase");
        let new = tmp
            .path()
            .join("restored")
            .join("mods")
            .join("BloogBase copy");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        let at = |parts: &[&str]| {
            parts
                .iter()
                .fold(old.clone(), |p, part| p.join(part))
                .display()
                .to_string()
        };
        let own = tmp
            .path()
            .join("dosbox")
            .join("dosbox.exe")
            .display()
            .to_string();
        {
            // As earlier versions wrote them.
            let db = Db::open(&old.join("bloogbase.db")).unwrap();
            db.conn()
                .execute_batch(&format!(
                    "INSERT INTO installs (id, game_id, label, source, path, added_at)
                         VALUES ('keen4-1', 'keen4', 'Keen 4', 'folder', '{}', 0);
                     INSERT INTO mods (id, game_id, name, authors, recipe_json, path, source, added_at)
                         VALUES ('mura', 'keen4', 'Mura', '[]', '\"episode\"', '{}', 'index', 0);
                     INSERT INTO profiles (id, install_id, state_path, created_at)
                         VALUES ('keen4-1', 'keen4-1', '{}', 0);
                     INSERT INTO save_backups (profile_id, archive_path, created_at, files_json)
                         VALUES ('keen4-1', '{}', 0, '[]');
                     INSERT INTO runtimes (id, kind, version, path, managed)
                         VALUES ('dosbox-staging-0.83.0', 'dosbox-staging', '0.83.0', '{}', 1),
                                ('custom', 'dosbox-staging', '0.82.0', '{own}', 0);",
                    at(&["library", "keen4-1"]),
                    at(&["mods", "mura"]),
                    at(&["profiles", "keen4-1", "state"]),
                    at(&["backups", "keen4-1", "100.zip"]),
                    at(&["runtimes", "dosbox-staging", "0.83.0", "bin", "dosbox.exe"]),
                ))
                .unwrap();
        }
        std::fs::copy(old.join("bloogbase.db"), new.join("bloogbase.db")).unwrap();

        let db = Db::open(&new.join("bloogbase.db")).unwrap();
        let stored = |sql: &str| -> String { db.conn().query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(stored("SELECT path FROM installs"), "library/keen4-1");
        assert_eq!(stored("SELECT path FROM mods"), "mods/mura");
        assert_eq!(
            stored("SELECT state_path FROM profiles"),
            "profiles/keen4-1/state"
        );
        assert_eq!(
            stored("SELECT archive_path FROM save_backups"),
            "backups/keen4-1/100.zip"
        );
        assert_eq!(
            stored("SELECT path FROM runtimes WHERE managed = 1"),
            "runtimes/dosbox-staging/0.83.0/bin/dosbox.exe"
        );
        assert_eq!(stored("SELECT path FROM runtimes WHERE managed = 0"), own);
        assert_eq!(
            db.resolve(&stored("SELECT path FROM mods")),
            new.join("mods").join("mura")
        );
    }

    #[test]
    fn leaves_paths_alone_that_lie_elsewhere() {
        let elsewhere = std::env::temp_dir().join("keen").join("state");
        let profiles = &STORED_PATHS[2];
        assert_eq!(
            relative_part(&elsewhere.display().to_string(), profiles),
            None
        );
        assert_eq!(relative_part("profiles/keen4-1/state", profiles), None);
    }
}

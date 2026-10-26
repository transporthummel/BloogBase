//! SQLite connection and schema migrations.

use std::path::Path;
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
];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATIONS_SLICE);

pub struct Db(Mutex<Connection>);

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
        // Off while migrating: rebuilding a table must not cascade into the
        // tables that point at it.
        conn.pragma_update(None, "foreign_keys", "OFF")?;
        MIGRATIONS.to_latest(&mut conn)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Db(Mutex::new(conn)))
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock can't leave SQLite in a broken state,
        // so a poisoned mutex is safe to keep using.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn()
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }
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
        let db = Db::init(conn).unwrap();
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
}

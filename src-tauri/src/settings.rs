//! Key/value settings stored in the database.

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use specta::Type;

use crate::db::Db;
use crate::error::Result;

/// Set once the user finished or skipped the setup wizard.
pub const SETUP_COMPLETED: &str = "setup.completed";

pub fn get(db: &Db, key: &str) -> Result<Option<String>> {
    Ok(db
        .conn()
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

pub fn set(db: &Db, key: &str, value: &str) -> Result<()> {
    db.conn().execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    /// The user finished or skipped the setup wizard.
    pub completed: bool,
    /// Games were imported (e.g. before the wizard existed, or by it).
    pub has_installs: bool,
}

pub fn setup_status(db: &Db) -> Result<SetupStatus> {
    let installs: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM installs", [], |r| r.get(0))?;
    Ok(SetupStatus {
        completed: get(db, SETUP_COMPLETED)?.is_some(),
        has_installs: installs > 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_set_overwrite() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(get(&db, "k").unwrap(), None);
        set(&db, "k", "a").unwrap();
        set(&db, "k", "b").unwrap();
        assert_eq!(get(&db, "k").unwrap().as_deref(), Some("b"));
    }

    #[test]
    fn setup_status_reflects_completion_and_library() {
        let db = Db::open_in_memory().unwrap();
        let st = setup_status(&db).unwrap();
        assert!(!st.completed && !st.has_installs);
        db.conn()
            .execute(
                "INSERT INTO installs (id, game_id, label, source, path, added_at)
                 VALUES ('keen4-1', 'keen4', 'x', 'folder', '/x', 0)",
                [],
            )
            .unwrap();
        set(&db, SETUP_COMPLETED, "1").unwrap();
        let st = setup_status(&db).unwrap();
        assert!(st.completed && st.has_installs);
    }
}

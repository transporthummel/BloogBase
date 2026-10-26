//! Key/value settings stored in the database.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
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

const SEEN_INDEX_MODS: &str = "news.seenIndexMods";
const OFFERED_MOD_UPDATES: &str = "news.offeredModUpdates";

/// What the news dialog on start knows already about the mod list. Kept with
/// the library, so a new data folder starts afresh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexNews {
    /// The entries known already; `None` before the list first loaded.
    pub seen: Option<Vec<String>>,
    /// Updates of installed mods offered already, as `<entry id>@<download url>`.
    pub offered: Vec<String>,
}

fn list(db: &Db, key: &str) -> Result<Option<Vec<String>>> {
    // A damaged value just means starting over.
    Ok(get(db, key)?.map(|v| serde_json::from_str(&v).unwrap_or_default()))
}

pub fn index_news(db: &Db) -> Result<IndexNews> {
    Ok(IndexNews {
        seen: list(db, SEEN_INDEX_MODS)?,
        offered: list(db, OFFERED_MOD_UPDATES)?.unwrap_or_default(),
    })
}

pub fn set_index_news(db: &Db, news: &IndexNews) -> Result<()> {
    if let Some(seen) = &news.seen {
        set(
            db,
            SEEN_INDEX_MODS,
            &serde_json::to_string(seen).expect("strings serialize"),
        )?;
    }
    set(
        db,
        OFFERED_MOD_UPDATES,
        &serde_json::to_string(&news.offered).expect("strings serialize"),
    )
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

    #[test]
    fn index_news_round_trip() {
        let db = Db::open_in_memory().unwrap();
        let empty = index_news(&db).unwrap();
        assert_eq!(
            empty,
            IndexNews {
                seen: None,
                offered: vec![]
            }
        );
        let news = IndexNews {
            seen: Some(vec!["a".into(), "b".into()]),
            offered: vec!["a@https://example.org/a.zip".into()],
        };
        set_index_news(&db, &news).unwrap();
        assert_eq!(index_news(&db).unwrap(), news);
    }
}

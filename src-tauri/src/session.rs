//! One play session from start to end: the backup before it, DOSBox, and
//! what is recorded and told afterwards. The Tauri side (events, the lock
//! on the profile, waiting for DOSBox) stays in `commands::launch_profile`.

use tokio::process::Child;

use crate::backups;
use crate::db::Db;
use crate::error::Result;
use crate::exit_screen;
use crate::launcher;
use crate::paths::AppPaths;
use crate::runtime::Runtime;

/// A game that has started.
pub struct Started {
    pub child: Child,
    pub session: i64,
    /// The backup made before it, to compare the saves with afterwards.
    pub backup: Option<u32>,
}

/// Backs up the saves, prepares the profile and starts DOSBox; `shown` is
/// called once its window is up.
pub fn start(
    paths: &AppPaths,
    db: &Db,
    runtime: &Runtime,
    profile_id: &str,
    shown: impl FnOnce() + Send + 'static,
) -> Result<Started> {
    let backup = backups::before_launch(paths, db, profile_id);
    let spec = launcher::prepare(paths, db, profile_id)?;
    let (child, session) = launcher::spawn(runtime, &spec, db, profile_id, shown)?;
    Ok(Started {
        child,
        session,
        backup,
    })
}

/// How a session ended.
#[derive(Debug)]
pub struct Ended {
    pub seconds: u32,
    /// What the game printed before it ended, e.g. an error.
    pub message: Option<String>,
    /// See `PlaySessionEnded::start_problem`.
    pub start_problem: bool,
}

/// Records the end of a session, checks the backup made before it and reads
/// what the game printed last. `quitting`: BloogBase ended the game as it
/// quit. Problems are logged, not returned: the session is over either way.
pub fn end(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    session: i64,
    backup: Option<u32>,
    exit_code: Option<i32>,
    quitting: bool,
) -> Ended {
    let seconds = launcher::finish(db, session, exit_code).unwrap_or_else(|e| {
        tracing::error!("could not record end of session {session}: {e}");
        0
    });
    if let Some(backup) = backup
        && let Err(e) = backups::after_session(db, profile_id, backup)
    {
        tracing::warn!("could not check backup {backup} after the session: {e}");
    }
    tracing::info!("{profile_id} exited with {exit_code:?} after {seconds}s");
    let message = exit_screen::message(&paths.profile(profile_id).exit_screen);
    if let Some(m) = &message {
        tracing::info!("{profile_id} ended with: {m}");
    }
    let start_problem = !quitting
        && (message.is_some() || seconds < launcher::SHORT_SESSION)
        && launcher::first_with_setup(db, session).unwrap_or_else(|e| {
            tracing::warn!("could not tell if session {session} was the first: {e}");
            false
        });
    Ended {
        seconds,
        message,
        start_problem,
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;
    use crate::library::{self, now};
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

    /// A session of `keen4-1` that started `ago` seconds ago.
    fn session(db: &Db, setup: Option<&str>, ago: i64) -> i64 {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO play_sessions (profile_id, started_at, setup) VALUES ('keen4-1', ?1, ?2)",
            params![now() - ago, setup],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn short_first_sessions_with_a_setup_are_start_problems() {
        let (_tmp, paths, db) = setup();
        let end = |session, quitting| end(&paths, &db, "keen4-1", session, None, Some(0), quitting);

        let game = end(session(&db, None, 5), false);
        // The clock may tick on meanwhile.
        assert!((5..=6).contains(&game.seconds));
        assert_eq!((game.message, game.start_problem), (None, false));
        assert!(!end(session(&db, Some("b"), 5), true).start_problem);
        assert!(end(session(&db, Some("a"), 5), false).start_problem);
        assert!(!end(session(&db, Some("a"), 5), false).start_problem);
        let long = launcher::SHORT_SESSION as i64 + 10;
        assert!(!end(session(&db, Some("c"), long), false).start_problem);
    }
}

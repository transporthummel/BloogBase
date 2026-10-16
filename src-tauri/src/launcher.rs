//! Launching a profile in DOSBox Staging: config layering, the two-layer
//! mount, process spawn and play session tracking (PLAN.md §6).
//!
//! Staging specifics (verified against the 0.83 manual):
//! - `--noprimaryconf` and `--nolocalconf` keep the user's own
//!   `dosbox-staging.conf` and any `dosbox.conf` in the working directory out
//!   of BloogBase launches.
//! - Staging joins `[autoexec]` sections of all config files. Instead of
//!   relying on `autoexec_section = overwrite`, launches pass `--noautoexec`
//!   and send their DOS commands via `-c`, so no `[autoexec]` from any layer
//!   ever runs.

use std::collections::HashSet;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::catalog::{self, Engine};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::library::now;
use crate::paths::AppPaths;
use crate::profiles;
use crate::runtime::{self, Runtime};

/// Profiles with a running DOSBox process; one instance per profile (§6.2).
#[derive(Default)]
pub struct Running(Mutex<HashSet<String>>);

impl Running {
    pub fn contains(&self, id: &str) -> bool {
        self.lock().contains(id)
    }

    /// Marks `id` as running, failing if it already is.
    pub fn claim(&self, id: &str) -> Result<()> {
        if self.lock().insert(id.to_string()) {
            Ok(())
        } else {
            Err(Error::AlreadyRunning)
        }
    }

    pub fn release(&self, id: &str) {
        self.lock().remove(id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct PlaySessionEnded {
    pub profile_id: String,
    pub exit_code: Option<i32>,
    pub seconds: u32,
}

fn default_conf(engine: Engine) -> (&'static str, &'static str) {
    match engine {
        Engine::Vorticons => ("vorticons", include_str!("../configs/vorticons.conf")),
        Engine::Galaxy => ("galaxy", include_str!("../configs/galaxy.conf")),
        Engine::Dreams => ("dreams", include_str!("../configs/dreams.conf")),
    }
}

const GLOBAL_OVERRIDE_TEMPLATE: &str = "\
# Your DOSBox Staging settings for all games. They override BloogBase's
# defaults; per-game overrides override these.
# [autoexec] sections are ignored: BloogBase starts the games itself.
";

const PROFILE_OVERRIDE_TEMPLATE: &str = "\
# Your DOSBox Staging settings for this game only. They override BloogBase's
# defaults and your global settings.
# [autoexec] sections are ignored: BloogBase starts the game itself.
";

/// Everything needed to start DOSBox for one profile.
#[derive(Debug)]
pub struct LaunchSpec {
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub log: PathBuf,
}

/// Ensures all directories and config layers exist and builds the command line.
pub fn prepare(paths: &AppPaths, db: &Db, profile_id: &str) -> Result<LaunchSpec> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let game = catalog::game(&profile.game_id)
        .ok_or_else(|| Error::NotFound(format!("game {}", profile.game_id)))?;
    let dirs = paths.profile(profile_id);
    for dir in [&layers.upper, &dirs.config, &dirs.capture] {
        fs::create_dir_all(dir)?;
    }
    let exe = profiles::find_executable(game, &layers.lower)?;

    let (engine, defaults) = default_conf(game.engine);
    let default_path = paths.default_conf(engine);
    write_if_changed(&default_path, defaults)?;
    let global = paths.global_override_conf();
    write_if_missing(&global, GLOBAL_OVERRIDE_TEMPLATE)?;
    write_if_missing(&dirs.override_conf, PROFILE_OVERRIDE_TEMPLATE)?;
    write_if_changed(
        &dirs.launch_conf,
        &launch_conf(&relative(&paths.root, &dirs.capture)),
    )?;

    // DOSBox Staging 0.83 crashes on mount paths with non-ASCII characters
    // (e.g. an umlaut in the Windows user name), so DOSBox runs from the data
    // root and only ever sees paths relative to it.
    let rel = |p: &Path| relative(&paths.root, p);
    Ok(LaunchSpec {
        args: dosbox_args(
            &[
                rel(&default_path),
                rel(&global),
                rel(&dirs.override_conf),
                rel(&dirs.launch_conf),
            ],
            &rel(&layers.lower),
            &rel(&layers.upper),
            &exe,
        ),
        cwd: paths.root.clone(),
        log: paths.logs.join(format!("dosbox-{profile_id}.log")),
    })
}

/// `path` relative to `root` if it's inside, else unchanged.
fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    if !rel.is_relative() && !rel.to_string_lossy().is_ascii() {
        tracing::warn!("DOSBox may not cope with this path: {}", rel.display());
    }
    rel.display().to_string()
}

/// Settings that belong to the launch itself; loaded last so users can't
/// break them by accident.
fn launch_conf(capture: &str) -> String {
    format!(
        "# Generated by BloogBase on every launch. Do not edit.\n\
         [dosbox]\n\
         automount = off\n\
         startup_verbosity = quiet\n\
         \n\
         [capture]\n\
         capture_dir = {capture}\n"
    )
}

fn dosbox_args(confs: &[String], lower: &str, upper: &str, exe: &str) -> Vec<String> {
    let mut args: Vec<String> = vec!["--noprimaryconf".into(), "--nolocalconf".into()];
    for conf in confs {
        args.push("--conf".into());
        args.push(conf.clone());
    }
    args.push("--noautoexec".into());
    for cmd in [
        format!("mount C \"{lower}\""),
        format!("mount C \"{upper}\" -t overlay"),
        "C:".into(),
        exe.to_string(),
        "exit".into(),
    ] {
        args.push("-c".into());
        args.push(cmd);
    }
    args
}

fn write_if_missing(path: &Path, content: &str) -> Result<()> {
    if !path.exists() {
        write_if_changed(path, content)?;
    }
    Ok(())
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if fs::read_to_string(path).is_ok_and(|c| c == content) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, content)?;
    Ok(())
}

/// Starts DOSBox and records the session start. Returns the running child and
/// the session id to pass to [`finish`].
pub fn spawn(
    runtime: &Runtime,
    spec: &LaunchSpec,
    db: &Db,
    profile_id: &str,
) -> Result<(tokio::process::Child, i64)> {
    let log = File::create(&spec.log)?;
    let mut cmd = Command::new(&runtime.path);
    cmd.args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    runtime::hide_console(&mut cmd);
    tracing::info!("launching {profile_id}: {} {:?}", runtime.path, spec.args);
    let child = tokio::process::Command::from(cmd).spawn()?;
    let session = start_session(db, profile_id, now())?;
    Ok((child, session))
}

fn start_session(db: &Db, profile_id: &str, at: i64) -> Result<i64> {
    let conn = db.conn();
    conn.execute(
        "INSERT INTO play_sessions (profile_id, started_at) VALUES (?1, ?2)",
        params![profile_id, at],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Records the end of a session and returns its length in seconds.
pub fn finish(db: &Db, session: i64, exit_code: Option<i32>) -> Result<u32> {
    end_session(db, session, now(), exit_code)
}

fn end_session(db: &Db, session: i64, at: i64, exit_code: Option<i32>) -> Result<u32> {
    let conn = db.conn();
    conn.execute(
        "UPDATE play_sessions SET ended_at = ?2, exit_code = ?3 WHERE id = ?1",
        params![session, at, exit_code],
    )?;
    let started: i64 = conn.query_row(
        "SELECT started_at FROM play_sessions WHERE id = ?1",
        [session],
        |r| r.get(0),
    )?;
    Ok((at - started).max(0) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library;
    use crate::source::testutil::*;

    fn setup() -> (tempfile::TempDir, AppPaths, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data dir"));
        paths.ensure().unwrap();
        let db = Db::open_in_memory().unwrap();
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        library::import(&paths, &db, &src).unwrap();
        (tmp, paths, db)
    }

    #[test]
    fn prepare_writes_layers_and_builds_command_line() {
        let (_tmp, paths, db) = setup();
        let spec = prepare(&paths, &db, "keen4-1").unwrap();
        let dirs = paths.profile("keen4-1");

        // Everything relative to the data root, which DOSBox runs in.
        let p = |parts: &[&str]| parts.iter().collect::<PathBuf>().display().to_string();
        let expected: Vec<String> = [
            "--noprimaryconf",
            "--nolocalconf",
            "--conf",
            &p(&["configs", "defaults", "galaxy.conf"]),
            "--conf",
            &p(&["configs", "global-override.conf"]),
            "--conf",
            &p(&["profiles", "keen4-1", "config", "override.conf"]),
            "--conf",
            &p(&["profiles", "keen4-1", "generated", "launch.conf"]),
            "--noautoexec",
            "-c",
            &format!("mount C \"{}\"", p(&["library", "keen4-1", "base"])),
            "-c",
            &format!(
                "mount C \"{}\" -t overlay",
                p(&["profiles", "keen4-1", "state"])
            ),
            "-c",
            "C:",
            "-c",
            "KEEN4E.EXE",
            "-c",
            "exit",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(spec.args, expected);
        assert_eq!(spec.cwd, paths.root);
        for conf in spec.args.iter().skip(3).step_by(2).take(4) {
            assert!(paths.root.join(conf).is_file(), "{conf}");
        }

        assert!(
            fs::read_to_string(paths.default_conf("galaxy"))
                .unwrap()
                .contains("[cpu]")
        );
        let launch = fs::read_to_string(&dirs.launch_conf).unwrap();
        assert!(launch.contains(&format!(
            "capture_dir = {}",
            p(&["profiles", "keen4-1", "capture"])
        )));
        assert!(!launch.contains("[autoexec]"));
        assert!(dirs.capture.is_dir());
    }

    #[test]
    fn user_overrides_are_kept_and_defaults_refreshed() {
        let (_tmp, paths, db) = setup();
        prepare(&paths, &db, "keen4-1").unwrap();
        let dirs = paths.profile("keen4-1");
        fs::write(&dirs.override_conf, "[cpu]\ncpu_cycles = 1\n").unwrap();
        fs::write(paths.global_override_conf(), "[sdl]\nfullscreen = on\n").unwrap();
        fs::write(paths.default_conf("galaxy"), "stale").unwrap();

        prepare(&paths, &db, "keen4-1").unwrap();
        assert_eq!(
            fs::read_to_string(&dirs.override_conf).unwrap(),
            "[cpu]\ncpu_cycles = 1\n"
        );
        assert_eq!(
            fs::read_to_string(paths.global_override_conf()).unwrap(),
            "[sdl]\nfullscreen = on\n"
        );
        assert_eq!(
            fs::read_to_string(paths.default_conf("galaxy")).unwrap(),
            include_str!("../configs/galaxy.conf")
        );
    }

    #[test]
    fn sessions_are_tracked() {
        let (_tmp, _paths, db) = setup();
        let id = start_session(&db, "keen4-1", 100).unwrap();
        assert_eq!(end_session(&db, id, 160, Some(0)).unwrap(), 60);
        let p = &profiles::list(&db, |_| false).unwrap()[0];
        assert_eq!(
            (p.session_count, p.play_seconds, p.last_played_at),
            (1, 60, Some(100))
        );
    }

    #[test]
    fn one_instance_per_profile() {
        let running = Running::default();
        running.claim("a").unwrap();
        assert!(matches!(running.claim("a"), Err(Error::AlreadyRunning)));
        running.claim("b").unwrap();
        assert!(running.contains("a"));
        running.release("a");
        assert!(!running.contains("a"));
        running.claim("a").unwrap();
    }
}

/// Manual end-to-end check against the real DOSBox Staging download:
/// `BLOOGBASE_E2E_GAME=<folder with a Keen game> cargo test e2e -- --ignored --nocapture`
/// Opens the game for a few seconds, then closes it.
#[cfg(test)]
mod e2e {
    use std::time::Duration;

    use super::*;
    use crate::library;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "downloads DOSBox and opens a window"]
    async fn download_import_and_launch() {
        let game = std::env::var("BLOOGBASE_E2E_GAME").expect("set BLOOGBASE_E2E_GAME");
        // Non-ASCII on purpose, like many Windows user names.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/e2e/Jürgen");
        let _ = fs::remove_dir_all(&root);
        let paths = AppPaths::new(&root);
        paths.ensure().unwrap();
        let db = Db::open(&paths.database).unwrap();

        let status = runtime::install(paths.clone(), &db, |p| eprintln!("{p:?}"))
            .await
            .unwrap();
        let rt = status.active.unwrap();
        eprintln!("runtime: {rt:?}");

        let installs = library::import(&paths, &db, Path::new(&game)).unwrap();
        let id = &installs[0].id;
        let spec = prepare(&paths, &db, id).unwrap();
        let (mut child, session) = spawn(&rt, &spec, &db, id).unwrap();
        tokio::time::sleep(Duration::from_secs(8)).await;
        assert!(
            child.try_wait().unwrap().is_none(),
            "DOSBox exited early, see {}",
            spec.log.display()
        );
        child.kill().await.unwrap();
        let secs = finish(&db, session, None).unwrap();
        assert!(secs >= 8);
        eprintln!("log: {}", spec.log.display());
    }
}

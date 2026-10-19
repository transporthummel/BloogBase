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

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use rusqlite::params;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::oneshot;

use crate::catalog;
use crate::compose;
use crate::db::Db;
use crate::dosconf;
use crate::error::{Error, Result};
use crate::firing;
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

/// The games running right now, each with its session and a way to stop
/// it, so quitting BloogBase can end them or at least their sessions.
#[derive(Default)]
pub struct Playing(Mutex<HashMap<String, (i64, oneshot::Sender<()>)>>);

impl Playing {
    pub fn add(&self, profile_id: &str, session: i64) -> oneshot::Receiver<()> {
        let (stop, stopped) = oneshot::channel();
        self.lock().insert(profile_id.to_string(), (session, stop));
        stopped
    }

    pub fn remove(&self, profile_id: &str) {
        self.lock().remove(profile_id);
    }

    pub fn profile_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.lock().keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Asks every game to stop; each one's session ends as usual.
    pub fn stop_all(&self) {
        for (_, (_, stop)) in self.lock().drain() {
            let _ = stop.send(());
        }
    }

    /// Lets the games run on without BloogBase, ending their sessions now:
    /// the time played so far counts, whatever comes after doesn't.
    pub fn abandon_all(&self, db: &Db) {
        for (profile_id, (session, _)) in self.lock().drain() {
            if let Err(e) = finish(db, session, None) {
                tracing::error!("could not end session {session} of {profile_id}: {e}");
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, (i64, oneshot::Sender<()>)>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Quitting was asked for while these games run; the frontend asks what to do.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct QuitRequested {
    pub games: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct PlaySessionEnded {
    pub profile_id: String,
    pub exit_code: Option<i32>,
    pub seconds: u32,
}

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
    let base = layers
        .lower
        .as_ref()
        .ok_or_else(|| Error::MissingGame(profile.missing_game.clone().unwrap_or_default()))?;
    let firing = (firing::supports(&game.id) && firing::enabled(db)?).then_some(game);
    let (lower, command) = match &layers.mod_layer {
        None if firing.is_none() => (base.clone(), None),
        m => (
            compose::ensure(paths, db, profile_id, base, m.as_ref(), firing)?,
            m.as_ref().and_then(|m| m.recipe.command()),
        ),
    };
    let command = match command {
        Some(c) => c,
        None => profiles::find_executable(game, profile.version_id.as_deref(), &lower)?,
    };

    let confs = dosconf::ensure(paths, game.engine, &dosconf::options(db)?, profile_id)?;

    // DOSBox Staging 0.83 crashes on mount paths with non-ASCII characters
    // (e.g. an umlaut in the Windows user name), so DOSBox runs from the data
    // root and only ever sees paths relative to it.
    let rel = |p: &Path| relative(&paths.root, p);
    Ok(LaunchSpec {
        args: dosbox_args(
            &[
                rel(&confs.defaults),
                rel(&confs.options),
                rel(&confs.global),
                rel(&confs.profile),
                rel(&confs.launch),
            ],
            &rel(&lower),
            &rel(&layers.upper),
            &command,
        ),
        cwd: paths.root.clone(),
        log: paths.logs.join(format!("dosbox-{profile_id}.log")),
    })
}

/// `path` relative to `root` if it's inside, else unchanged.
pub fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    if !rel.is_relative() && !rel.to_string_lossy().is_ascii() {
        tracing::warn!("DOSBox may not cope with this path: {}", rel.display());
    }
    rel.display().to_string()
}

fn dosbox_args(confs: &[String], lower: &str, upper: &str, command: &str) -> Vec<String> {
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
        command.to_string(),
        "exit".into(),
    ] {
        args.push("-c".into());
        args.push(cmd);
    }
    args
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
            &p(&["configs", "options.conf"]),
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
        for conf in spec.args.iter().skip(3).step_by(2).take(5) {
            assert!(paths.root.join(conf).is_file(), "{conf}");
        }

        assert!(
            fs::read_to_string(paths.default_conf("galaxy"))
                .unwrap()
                .contains("[cpu]")
        );
        let launch = fs::read_to_string(&dirs.launch_conf).unwrap();
        assert!(launch.contains(&format!("capture_dir = {}", p(&["..", "capture"]))));
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
    fn one_button_firing_patches_a_copy_of_keen_1_3() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("k1");
        let exe = firing::tests::exe();
        write_folder(
            &src,
            &[
                ("KEEN1.EXE", &exe),
                ("EGAHEAD.CK1", b"h"),
                ("EGALATCH.CK1", b"l"),
                ("EGASPRIT.CK1", b"s"),
                ("LEVEL01.CK1", b"1"),
                ("SOUNDS.CK1", b"a"),
            ],
        );
        library::import(&paths, &db, &src).unwrap();
        firing::set_enabled(&db, true).unwrap();

        let mount = |id: &str| prepare(&paths, &db, id).unwrap().args[14].clone();
        // Keen 4 has no such patch: it still runs from the install.
        assert!(mount("keen4-1").contains(&relative(&paths.root, &paths.library)));
        assert!(mount("keen1-1").contains(&relative(&paths.root, &paths.composed("keen1-1"))));
        let patched = fs::read(paths.composed("keen1-1").join("KEEN1.EXE")).unwrap();
        assert_ne!(patched, exe);
        assert_eq!(
            fs::read(paths.library.join("keen1-1/base/KEEN1.EXE")).unwrap(),
            exe
        );

        firing::set_enabled(&db, false).unwrap();
        assert!(mount("keen1-1").contains(&relative(&paths.root, &paths.library)));
    }

    #[test]
    fn mod_profile_runs_recipe_on_composed_layer() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("mod");
        write_folder(
            &src,
            &[
                ("PLAY.BAT", b"ck4patch mod.pat"),
                ("CK4PATCH.EXE", b"p"),
                ("MOD.PAT", b"%ext ck4"),
                ("gamemaps.ck4", b"new"),
            ],
        );
        let info = crate::mods::inspect(&src, None).unwrap();
        let m = crate::mods::install(
            &paths,
            &db,
            &crate::mods::InstallModRequest {
                path: src.display().to_string(),
                name: "My Mod".into(),
                recipe: info.recipes[0].recipe.clone(),
                install_id: Some("keen4-1".into()),
                keenwiki: None,
            },
        )
        .unwrap();
        assert_eq!(profiles::list(&db, |_| false).unwrap()[1].label, "My Mod");

        let spec = prepare(&paths, &db, &m.profile_id).unwrap();
        let composed = paths.composed("my-mod");
        let tail: Vec<&str> = spec
            .args
            .iter()
            .rev()
            .take(10)
            .rev()
            .map(String::as_str)
            .collect();
        let lower = format!(
            "mount C \"{}\"",
            composed.strip_prefix(&paths.root).unwrap().display()
        );
        assert_eq!(tail[1], lower);
        assert_eq!(tail[7], "CK4PATCH.EXE MOD.PAT");
        assert_eq!(fs::read(composed.join("gamemaps.ck4")).unwrap(), b"new");
        assert!(composed.join("KEEN4E.EXE").is_file());
        let names: Vec<String> = fs::read_dir(&composed)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!names.contains(&"GAMEMAPS.CK4".to_string()));

        // Without its game the mod can't start.
        crate::library::remove(&paths, &db, "keen4-1", false, false).unwrap();
        match prepare(&paths, &db, &m.profile_id) {
            Err(Error::MissingGame(game)) => assert!(game.starts_with("Commander Keen 4")),
            other => panic!("{other:?}"),
        }
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

    #[test]
    fn quitting_stops_games_or_ends_their_sessions() {
        let (_tmp, _paths, db) = setup();
        let playing = Playing::default();
        let mut stopped = playing.add("a", 1);
        let mut left = playing.add("keen4-1", start_session(&db, "keen4-1", 100).unwrap());
        assert_eq!(playing.profile_ids(), ["a", "keen4-1"]);

        playing.remove("a");
        // A game that ended on its own isn't asked to stop.
        assert!(stopped.try_recv().is_err());
        playing.abandon_all(&db);
        assert!(playing.profile_ids().is_empty());
        assert!(left.try_recv().is_err());
        let p = &profiles::list(&db, |_| false).unwrap()[0];
        assert_eq!(p.session_count, 1);

        let mut stopped = playing.add("b", 2);
        playing.stop_all();
        assert_eq!(stopped.try_recv(), Ok(()));
        assert!(playing.profile_ids().is_empty());
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

        let status = runtime::install(paths.clone(), &db, |phase, received, total| {
            eprintln!("{phase:?} {received}/{total}")
        })
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

/// Manual check that real mods start: imports every game folder in
/// `.local/games`, installs every mod in `.local/mods` on each install it
/// fits, runs it in DOSBox for a few seconds and saves a screenshot to
/// `target/e2e-mods/shots/`.
/// `BLOOGBASE_E2E_DOSBOX=<dosbox.exe> cargo test e2e_mods -- --ignored --nocapture --test-threads=1`
#[cfg(all(test, windows))]
mod e2e_mods {
    use std::time::Duration;

    use super::*;
    use crate::{library, mods};

    /// Captures only the DOSBox window (via `PrintWindow`, so nothing else on
    /// the desktop ends up in the image).
    fn screenshot(pid: u32, path: &Path) {
        const SCRIPT: &str = r#"
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System; using System.Runtime.InteropServices;
public class Win {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  public struct RECT { public int L, T, R, B; }
}
'@
[Win]::SetProcessDPIAware() | Out-Null
$h = (Get-Process -Id $env:SHOT_PID).MainWindowHandle
if ($h -eq 0) { exit 1 }
$r = New-Object Win+RECT
[Win]::GetWindowRect($h, [ref]$r) | Out-Null
$bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$dc = $g.GetHdc(); [Win]::PrintWindow($h, $dc, 2) | Out-Null; $g.ReleaseHdc($dc)
$bmp.Save($env:SHOT_PATH)
"#;
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-Command", SCRIPT])
            .env("SHOT_PID", pid.to_string())
            .env("SHOT_PATH", path)
            .status();
    }

    fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        v.sort();
        v
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "opens DOSBox windows"]
    async fn e2e_mods_start() {
        let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.local");
        let dosbox = std::env::var("BLOOGBASE_E2E_DOSBOX").expect("set BLOOGBASE_E2E_DOSBOX");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/e2e-mods");
        let _ = fs::remove_dir_all(&root);
        let paths = AppPaths::new(&root);
        paths.ensure().unwrap();
        let shots = root.join("shots");
        fs::create_dir_all(&shots).unwrap();
        let db = Db::open(&paths.database).unwrap();
        let rt = runtime::set_custom(&db, Some(Path::new(&dosbox)))
            .unwrap()
            .active
            .unwrap();

        for game in sorted_entries(&local.join("games")) {
            match library::import(&paths, &db, &game) {
                Ok(found) => {
                    for i in found {
                        eprintln!("game {}: {} {:?}", game.display(), i.id, i.version_id);
                    }
                }
                Err(e) => eprintln!("game {}: {e}", game.display()),
            }
        }
        let installs = library::list(&db).unwrap();

        for zip in sorted_entries(&local.join("mods")) {
            let name = zip.file_stem().unwrap().to_string_lossy().into_owned();
            let info = match mods::inspect(&zip, None) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("{name}: inspect failed: {e}");
                    continue;
                }
            };
            let mut tried = false;
            for install in installs.iter().filter(|i| i.game_id == info.game_id) {
                let request = mods::InstallModRequest {
                    path: zip.display().to_string(),
                    name: String::new(),
                    recipe: info.recipes[0].recipe.clone(),
                    install_id: Some(install.id.clone()),
                    keenwiki: None,
                };
                let m = match mods::install(&paths, &db, &request) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("{name} on {}: refused: {e}", install.id);
                        continue;
                    }
                };
                tried = true;
                let spec = prepare(&paths, &db, &m.profile_id).unwrap();
                let (mut child, session) = spawn(&rt, &spec, &db, &m.profile_id).unwrap();
                tokio::time::sleep(Duration::from_secs(7)).await;
                let exited = child.try_wait().unwrap();
                if let (None, Some(pid)) = (exited, child.id()) {
                    screenshot(pid, &shots.join(format!("{}.png", m.profile_id)));
                }
                let _ = child.kill().await;
                finish(&db, session, None).unwrap();
                let outcome = match exited {
                    None => "running".to_string(),
                    Some(s) => format!("EXITED EARLY ({s})"),
                };
                eprintln!(
                    "{name} on {} ({:?}): {outcome} -> shots/{}.png",
                    install.id, install.version_id, m.profile_id
                );
            }
            if !tried {
                eprintln!("{name}: no fitting install");
            }
        }
    }
}

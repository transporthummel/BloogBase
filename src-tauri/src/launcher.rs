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
//! - After BloogBase's own mounts, `config -securemode` locks the drives: no
//!   program or batch file of a mod can `mount` a host folder, and
//!   `config -r` (a restart that could leave secure mode) is refused.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use rusqlite::params;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::ChildStderr;
use tokio::sync::oneshot;

use crate::catalog;
use crate::compose;
use crate::db::Db;
use crate::dosconf;
use crate::error::{Error, Result};
use crate::exit_screen;
use crate::firing;
use crate::launcher_copies;
use crate::library::now;
use crate::paths::AppPaths;
use crate::profiles;
use crate::recipe::Recipe;
use crate::runtime::{self, Runtime};

/// Profiles with a running DOSBox process; one instance per profile (§6.2).
#[derive(Default)]
pub struct Running(Arc<Mutex<HashSet<String>>>);

impl Running {
    pub fn contains(&self, id: &str) -> bool {
        lock(&self.0).contains(id)
    }

    /// Marks `id` as running until the claim is dropped, failing if it
    /// already is.
    pub fn claim(&self, id: &str) -> Result<Claim> {
        self.claim_all([id])
    }

    /// Marks all of `ids` as running until the claim is dropped, or none
    /// if one of them already is.
    pub fn claim_all<S: Into<String>>(&self, ids: impl IntoIterator<Item = S>) -> Result<Claim> {
        let mut ids: Vec<String> = ids.into_iter().map(Into::into).collect();
        ids.sort();
        ids.dedup();
        let mut set = lock(&self.0);
        if ids.iter().any(|id| set.contains(id)) {
            return Err(Error::AlreadyRunning);
        }
        set.extend(ids.iter().cloned());
        Ok(Claim {
            set: Arc::clone(&self.0),
            ids,
        })
    }
}

/// Profiles marked as running; dropping it releases them.
#[must_use = "the profiles are released when the claim is dropped"]
pub struct Claim {
    set: Arc<Mutex<HashSet<String>>>,
    ids: Vec<String>,
}

impl Drop for Claim {
    fn drop(&mut self) {
        let mut set = lock(&self.set);
        for id in &self.ids {
            set.remove(id);
        }
    }
}

fn lock(set: &Mutex<HashSet<String>>) -> std::sync::MutexGuard<'_, HashSet<String>> {
    set.lock().unwrap_or_else(|e| e.into_inner())
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

/// How far a launch has got, so the frontend can cover the seconds until
/// DOSBox shows its window. [`PlaySessionEnded`] follows when it closes.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct GameLaunch {
    pub profile_id: String,
    pub phase: LaunchPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum LaunchPhase {
    Starting,
    /// DOSBox's window is up.
    Shown,
    /// It didn't start; the command returns the error.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct PlaySessionEnded {
    pub profile_id: String,
    pub exit_code: Option<i32>,
    pub seconds: u32,
    /// What the game printed before it ended, e.g. an error.
    pub message: Option<String>,
    /// A mod's first session with its setup ended with a message or after
    /// less than 30 seconds (`SHORT_SESSION`): it may not have started properly.
    pub start_problem: bool,
}

/// A first session shorter than this (seconds) looks like the mod didn't start.
pub const SHORT_SESSION: u32 = 30;

/// Everything needed to start DOSBox for one profile.
#[derive(Debug)]
pub struct LaunchSpec {
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub log: PathBuf,
    /// What a mod runs with, see [`mod_setup`]; `None` for games.
    pub setup: Option<String>,
}

/// Ensures all directories and config layers exist and builds the command line.
pub fn prepare(paths: &AppPaths, db: &Db, profile_id: &str) -> Result<LaunchSpec> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let game = catalog::game(&profile.game_id)
        .ok_or_else(|| Error::NotFound(format!("game {}", profile.game_id)))?;
    let setup = match &layers.mod_layer {
        Some(m) => Some(mod_setup(db, &profile, &m.recipe)?),
        None => None,
    };
    let dirs = paths.profile(profile_id);
    for dir in [
        &layers.upper,
        &dirs.config,
        &dirs.capture,
        &dirs.exit_screen,
    ] {
        fs::create_dir_all(dir)?;
    }
    exit_screen::prepare(&dirs.exit_screen)?;
    let base = layers
        .lower
        .as_ref()
        .ok_or_else(|| Error::MissingGame(profile.missing_game.clone().unwrap_or_default()))?;
    let firing = firing::supports(&game.id) && firing::enabled(db)?;
    let (lower, command) = match &layers.mod_layer {
        None if !firing => (base.clone(), None),
        m => {
            let composed = compose::ensure(paths, db, profile_id, base, m.as_ref(), game, firing)?;
            if let (true, Some(m)) = (composed.changed, m) {
                launcher_copies::drop(
                    paths,
                    db,
                    profile_id,
                    crate::saves::profile_game(&profile.game_id, &layers)?,
                    &m.recipe,
                    &composed.dir,
                    &layers.upper,
                )?;
            }
            (composed.dir, m.as_ref().and_then(|m| m.recipe.command()))
        }
    };
    let command = match command {
        Some(c) => c,
        None => profiles::find_executable(game, profile.version_id.as_deref(), &lower)?,
    };
    let command = runnable(command, &lower, &dirs.exit_screen)?;

    let confs = dosconf::ensure(
        paths,
        game.engine,
        dosconf::mod_defaults(&layers),
        &dosconf::options(db)?,
        profile_id,
        dosconf::window_title(&profile, game),
    )?;

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
            &rel(&dirs.exit_screen),
            &command,
        ),
        cwd: paths.root.clone(),
        log: paths.logs.join(format!("dosbox-{profile_id}.log")),
        setup,
    })
}

/// What a mod profile runs with: the copy of the game, the mod's files (and
/// those of the mod it's played over) and the recipe. A new one, after
/// changing the recipe, the copy or the mod, starts over as a first session.
fn mod_setup(db: &Db, profile: &profiles::Profile, recipe: &Recipe) -> Result<String> {
    let (archive, base): (Option<String>, Option<String>) = db.conn().query_row(
        "SELECT m.archive_sha256, (SELECT b.archive_sha256 FROM mods b WHERE b.id = m.extends)
         FROM mods m WHERE m.id = ?1",
        [&profile.mod_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(format!(
        "install={};mod={};base={};recipe={}",
        profile.install_id.as_deref().unwrap_or_default(),
        archive.unwrap_or_default(),
        base.unwrap_or_default(),
        serde_json::to_string(recipe).unwrap_or_default()
    ))
}

/// `path` relative to `root` if it's inside, else unchanged.
pub fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    if !rel.is_relative() && !rel.to_string_lossy().is_ascii() {
        tracing::warn!("DOSBox may not cope with this path: {}", rel.display());
    }
    rel.display().to_string()
}

/// `command` as DOSBox can run it from `-c`, with `lower` the C: drive.
/// DOSBox Staging 0.83 can't start a batch file with a space in its name,
/// quoted or not (DasBrot's First Mod's `DasBrots First Mod.bat`); its
/// short name (`DASBRO~1.BAT`) depends on the other files. And it turns
/// every `'` in a `-c` command into `"` (Billy Can't's
/// `CK5PATCH.EXE Can't.pat` became `Can"t.pat`), but not in batch files.
/// So such a batch file is copied to drive X, and such a command runs from
/// a batch file there.
fn runnable(command: String, lower: &Path, exit_screen: &Path) -> Result<String> {
    let start = exit_screen.join(START);
    let batch = lower.join(&command);
    if command.contains(' ') && is_batch(&command) && batch.is_file() {
        dosconf::write_if_changed(&start, fs::read(batch)?)?;
    } else if command.contains('\'') {
        dosconf::write_if_changed(&start, format!("{}\r\n", run_line(&command)))?;
    } else {
        return Ok(command);
    }
    Ok(format!("{}:\\{START}", exit_screen::DRIVE))
}

/// The batch file [`runnable`] writes.
const START: &str = "START.BAT";

fn is_batch(file: &str) -> bool {
    file.to_ascii_uppercase().ends_with(".BAT")
}

/// `command` without its echo. A batch file started without CALL never
/// returns, so `exit` would not run.
fn run_line(command: &str) -> String {
    if command.split_whitespace().next().is_some_and(is_batch) {
        format!("@call {command}")
    } else {
        format!("@{command}")
    }
}

fn dosbox_args(
    confs: &[String],
    lower: &str,
    upper: &str,
    exit_screen: &str,
    command: &str,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["--noprimaryconf".into(), "--nolocalconf".into()];
    for conf in confs {
        args.push("--conf".into());
        args.push(conf.clone());
    }
    args.push("--noautoexec".into());
    // `@` and `> NUL` keep our commands and their messages off the screen,
    // so the game's output comes first. Batch files still echo, as in DOS.
    // The screen saver stays visible: it marks the end of the game's output.
    for cmd in [
        format!("@mount C \"{lower}\" > NUL"),
        format!("@mount C \"{upper}\" -t overlay > NUL"),
        format!("@mount {} \"{exit_screen}\" > NUL", exit_screen::DRIVE),
        "@config -securemode > NUL".into(),
        "@C:".into(),
        run_line(command),
        exit_screen::command(),
        "exit".into(),
    ] {
        args.push("-c".into());
        args.push(cmd);
    }
    args
}

/// Starts DOSBox and records the session start; `shown` is called once its
/// window is up. Returns the running child and the session id to pass to
/// [`finish`].
pub fn spawn(
    runtime: &Runtime,
    spec: &LaunchSpec,
    db: &Db,
    profile_id: &str,
    shown: impl FnOnce() + Send + 'static,
) -> Result<(tokio::process::Child, i64)> {
    let log = File::create(&spec.log)?;
    let mut cmd = Command::new(&runtime.path);
    cmd.args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(Stdio::piped());
    runtime::hide_console(&mut cmd);
    tracing::info!("launching {profile_id}: {} {:?}", runtime.path, spec.args);
    let mut child = tokio::process::Command::from(cmd).spawn()?;
    if let Some(stderr) = child.stderr.take() {
        let pid = child.id().unwrap_or_default();
        tokio::spawn(watch(stderr, log, pid, shown));
    }
    let session = start_session(db, profile_id, spec.setup.as_deref(), now())?;
    Ok((child, session))
}

/// Copies DOSBox's log output to `log` and, at the first video mode it
/// presents, gives its window our icon and calls `shown`. Reads to the end,
/// so DOSBox never blocks on a full pipe.
async fn watch(stderr: ChildStderr, mut log: File, pid: u32, shown: impl FnOnce()) {
    let mut reader = BufReader::new(stderr);
    let mut shown = Some(shown);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let _ = log.write_all(&line);
        if shows_window(&line)
            && let Some(shown) = shown.take()
        {
            set_icon(pid);
            shown();
        }
    }
}

/// The icon of game windows: a terminal rather than the app's own, so the
/// taskbar tells the two apart.
#[cfg_attr(not(windows), allow(dead_code))]
const GAME_ICON: &[u8] = include_bytes!("../icons/dosbox.ico");

/// The image in the .ico file `ico` best suited for `size` pixels: the
/// smallest one at least that big, else the biggest. PNG or DIB data, as
/// `CreateIconFromResourceEx` takes it.
#[cfg_attr(not(windows), allow(dead_code))]
fn icon_image(ico: &[u8], size: u32) -> Option<&[u8]> {
    let count = u16::from_le_bytes(ico.get(4..6)?.try_into().ok()?);
    let mut images = Vec::new();
    for i in 0..usize::from(count) {
        let entry = ico.get(6 + i * 16..6 + (i + 1) * 16)?;
        // 0 means 256.
        let width = if entry[0] == 0 {
            256
        } else {
            u32::from(entry[0])
        };
        let len = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
        images.push((width, ico.get(offset..offset.checked_add(len)?)?));
    }
    images.sort_by_key(|(width, _)| *width);
    let fits = images.iter().find(|(width, _)| *width >= size);
    fits.or(images.last()).map(|(_, data)| *data)
}

/// [`GAME_ICON`] at `size` pixels as an `HICON`, 0 if Windows can't read it.
#[cfg(windows)]
fn game_icon(size: i32) -> isize {
    use windows_sys::Win32::Foundation::TRUE;
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconFromResourceEx, LR_DEFAULTCOLOR};

    let Some(image) = icon_image(GAME_ICON, size as u32) else {
        return 0;
    };
    // SAFETY: `image` stays valid during the call.
    unsafe {
        CreateIconFromResourceEx(
            image.as_ptr(),
            image.len() as u32,
            TRUE,
            0x0003_0000,
            size,
            size,
            LR_DEFAULTCOLOR,
        ) as isize
    }
}

/// Puts [`GAME_ICON`] on the windows of process `pid` in place of DOSBox's
/// own. Windows keeps the icon across processes as long as BloogBase runs,
/// which it does while games do.
#[cfg(windows)]
fn set_icon(pid: u32) {
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetSystemMetrics, GetWindowThreadProcessId, ICON_BIG, ICON_SMALL, SM_CXICON,
        SM_CXSMICON, SendMessageW, WM_SETICON,
    };

    /// Made once and kept for the app's lifetime, shared by all game windows.
    static ICONS: OnceLock<(isize, isize)> = OnceLock::new();

    struct Icons {
        pid: u32,
        small: isize,
        big: isize,
    }
    unsafe extern "system" fn each(window: HWND, icons: LPARAM) -> i32 {
        // SAFETY: `icons` is the `Icons` passed to `EnumWindows` below.
        let icons = unsafe { &*(icons as *const Icons) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
        if pid == icons.pid {
            unsafe {
                SendMessageW(window, WM_SETICON, ICON_SMALL as usize, icons.small);
                SendMessageW(window, WM_SETICON, ICON_BIG as usize, icons.big);
            }
        }
        TRUE
    }

    // SAFETY: plain Win32 call.
    let load = |metric| game_icon(unsafe { GetSystemMetrics(metric) });
    let &(small, big) = ICONS.get_or_init(|| (load(SM_CXSMICON), load(SM_CXICON)));
    if small == 0 || big == 0 {
        tracing::warn!("no icon for the DOSBox window");
        return;
    }
    let icons = Icons { pid, small, big };
    // SAFETY: `icons` outlives `EnumWindows`.
    unsafe { EnumWindows(Some(each), &icons as *const Icons as LPARAM) };
}

#[cfg(not(windows))]
fn set_icon(_pid: u32) {}

/// E.g. `DISPLAY: VGA 720x400 16-colour text mode 03h at 70.087 Hz, scaled
/// to 1067x800 pixels …`, logged once the window shows that mode; earlier
/// `DISPLAY:` lines only plan the window.
fn shows_window(line: &[u8]) -> bool {
    let line = String::from_utf8_lossy(line);
    line.contains("DISPLAY: ") && line.contains(" mode ") && line.contains(" scaled to ")
}

fn start_session(db: &Db, profile_id: &str, setup: Option<&str>, at: i64) -> Result<i64> {
    let conn = db.conn();
    conn.execute(
        "INSERT INTO play_sessions (profile_id, started_at, setup) VALUES (?1, ?2, ?3)",
        params![profile_id, at, setup],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Whether `session` is its profile's first with a mod's setup.
pub fn first_with_setup(db: &Db, session: i64) -> Result<bool> {
    Ok(db.conn().query_row(
        "SELECT s.setup IS NOT NULL AND NOT EXISTS (
             SELECT 1 FROM play_sessions e
             WHERE e.profile_id = s.profile_id AND e.setup = s.setup AND e.id < s.id)
         FROM play_sessions s WHERE s.id = ?1",
        [session],
        |r| r.get(0),
    )?)
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
    fn batch_files_are_called_so_dosbox_exits() {
        let cmd = |c: &str| dosbox_args(&[], "l", "u", "x", c)[14].clone();
        assert_eq!(cmd("cool1.bat"), "@call cool1.bat");
        assert_eq!(cmd("PLAY.BAT -x"), "@call PLAY.BAT -x");
        assert_eq!(cmd("KEEN1.EXE"), "@KEEN1.EXE");
    }

    #[test]
    fn commands_with_apostrophes_run_from_a_batch_file() {
        let tmp = tempfile::tempdir().unwrap();
        let x = tmp.path();
        assert_eq!(runnable("KEEN1.EXE".into(), x, x).unwrap(), "KEEN1.EXE");
        assert!(!x.join(START).exists());
        let cmd = runnable("CK5PATCH.EXE Can't.pat".into(), x, x).unwrap();
        assert_eq!(cmd, r"X:\START.BAT");
        assert_eq!(
            fs::read_to_string(x.join(START)).unwrap(),
            "@CK5PATCH.EXE Can't.pat\r\n"
        );
        assert_eq!(
            dosbox_args(&[], "l", "u", "x", &cmd)[14],
            r"@call X:\START.BAT"
        );
    }

    #[test]
    fn batch_files_with_spaces_run_from_a_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let (c, x) = (tmp.path().join("c"), tmp.path().join("x"));
        fs::create_dir_all(&c).unwrap();
        fs::create_dir_all(&x).unwrap();
        let batch = "@echo off\r\nck4patch patch4.pat -nowait\r\n";
        fs::write(c.join("DasBrots First Mod.bat"), batch).unwrap();
        let cmd = runnable("DasBrots First Mod.bat".into(), &c, &x).unwrap();
        assert_eq!(cmd, r"X:\START.BAT");
        assert_eq!(fs::read_to_string(x.join(START)).unwrap(), batch);
        assert_eq!(runnable("PLAY.BAT".into(), &c, &x).unwrap(), "PLAY.BAT");
    }

    #[test]
    fn prepare_writes_layers_and_builds_command_line() {
        let (_tmp, paths, db) = setup();
        let spec = prepare(&paths, &db, "keen4-1").unwrap();
        let dirs = paths.profile("keen4-1");
        // Only mods tell first sessions apart.
        assert_eq!(spec.setup, None);

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
            &format!("@mount C \"{}\" > NUL", p(&["library", "keen4-1", "base"])),
            "-c",
            &format!(
                "@mount C \"{}\" -t overlay > NUL",
                p(&["profiles", "keen4-1", "state"])
            ),
            "-c",
            &format!(
                "@mount X \"{}\" > NUL",
                p(&["profiles", "keen4-1", "generated", "exit"])
            ),
            "-c",
            "@config -securemode > NUL",
            "-c",
            "@C:",
            "-c",
            "@KEEN4E.EXE",
            "-c",
            r"X:\SCREEN.COM X:\SCREEN.BIN",
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
        assert!(launch.contains("program=\"Commander Keen 4: Secret of the Oracle – BloogBase\""));
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
    fn launcher_copies_go_when_the_mod_changes() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("mod");
        write_folder(
            &src,
            &[
                ("GO.COM", b"\0/C RENAME A.PAT B.PAT\0CK4PATCH B.PAT\0"),
                ("A.PAT", b"%ext ck4\n%end\n"),
            ],
        );
        let m = crate::mods::install(
            &paths,
            &db,
            &crate::mods::InstallModRequest {
                path: src.display().to_string(),
                name: "Swapper".into(),
                recipe: crate::recipe::Recipe::Program {
                    program: "GO.COM".into(),
                    args: Vec::new(),
                },
                install_id: Some("keen4-1".into()),
                keenwiki: None,
                extends: None,
                replace: None,
            },
        )
        .unwrap();

        // What a run leaves: the renamed patch, the marker hiding the old
        // name, and the game's own files.
        let (_, layers) = profiles::get(&db, &m.profile_id).unwrap();
        let state = &layers.upper;
        let run = |save: &[u8]| {
            fs::create_dir_all(state).unwrap();
            for (name, data) in [
                ("B.PAT", &b"%ext ck4\n%end\n"[..]),
                ("DBOVERLAY_DEL_A.PAT", b"empty"),
                ("SAVEGAM0.CK4", save),
                ("CONFIG.CK4", b"config"),
            ] {
                fs::write(state.join(name), data).unwrap();
            }
        };
        let files = |dir: &Path| -> Vec<String> {
            let mut n: Vec<String> = fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            n.sort();
            n
        };

        // Brought along by an earlier backup restored into the new profile:
        // cleared on its first launch.
        run(b"save");
        prepare(&paths, &db, &m.profile_id).unwrap();
        assert_eq!(files(state), ["CONFIG.CK4", "SAVEGAM0.CK4"]);

        // Unchanged: everything stays.
        run(b"save 2");
        prepare(&paths, &db, &m.profile_id).unwrap();
        assert_eq!(files(state).len(), 4);

        // A mod update: the copies go, after a backup of all of state/.
        fs::write(
            paths.mods.join("swapper/files/A.PAT"),
            b"%ext ck4\n%patch $1 $2\n%end\n",
        )
        .unwrap();
        // As a launch does it: the backup before playing comes first and
        // goes after a session that lost no save; the one before clearing
        // the copies has the same content but stays.
        let before = crate::backups::before_launch(&paths, &db, &m.profile_id).unwrap();
        prepare(&paths, &db, &m.profile_id).unwrap();
        assert_eq!(files(state), ["CONFIG.CK4", "SAVEGAM0.CK4"]);
        assert!(!crate::backups::after_session(&db, &m.profile_id, before).unwrap());
        let backups = crate::backups::list(&db, &m.profile_id).unwrap();
        assert_eq!(backups.len(), 2);
        assert_eq!(
            backups[0].reason,
            Some(crate::backups::Reason::LauncherReset)
        );
        assert_eq!(backups[0].files.len(), 4);

        // Restoring everything brings the copies back; the next launch
        // clears them again. Restoring saves only rebuilds nothing.
        crate::backups::restore(&paths, &db, backups[0].id, None, None, true).unwrap();
        assert_eq!(files(state).len(), 4);
        prepare(&paths, &db, &m.profile_id).unwrap();
        assert_eq!(files(state), ["CONFIG.CK4", "SAVEGAM0.CK4"]);
        crate::backups::restore(&paths, &db, backups[0].id, None, None, false).unwrap();
        let hash = || -> Option<String> {
            db.conn()
                .query_row(
                    "SELECT composed_hash FROM profiles WHERE id = ?1",
                    [&m.profile_id],
                    |r| r.get(0),
                )
                .unwrap()
        };
        assert!(hash().is_some());
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
                extends: None,
                replace: None,
            },
        )
        .unwrap();
        assert_eq!(profiles::list(&db, |_| false).unwrap()[1].label, "My Mod");

        let spec = prepare(&paths, &db, &m.profile_id).unwrap();
        let setup = spec.setup.as_deref().unwrap();
        assert!(setup.contains("CK4PATCH.EXE"), "{setup}");
        let composed = paths.composed("my-mod");
        let tail: Vec<&str> = spec
            .args
            .iter()
            .rev()
            .take(16)
            .rev()
            .map(String::as_str)
            .collect();
        let lower = format!(
            "@mount C \"{}\" > NUL",
            composed.strip_prefix(&paths.root).unwrap().display()
        );
        assert_eq!(tail[1], lower);
        assert_eq!(tail[11], "@CK4PATCH.EXE MOD.PAT");
        assert_eq!(fs::read(composed.join("gamemaps.ck4")).unwrap(), b"new");
        assert!(composed.join("KEEN4E.EXE").is_file());
        let names: Vec<String> = fs::read_dir(&composed)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!names.contains(&"GAMEMAPS.CK4".to_string()));
        // Engine defaults only: the mod needs nothing more.
        assert!(spec.args.iter().any(|a| a.ends_with("galaxy.conf")));
        let launch = fs::read_to_string(paths.profile(&m.profile_id).launch_conf).unwrap();
        assert!(launch.contains("program=\"My Mod – BloogBase\""));

        // A patch that switches on composite colors gets a CGA machine.
        fs::write(
            paths.mods.join("my-mod/files/MOD.PAT"),
            "%ext ck4
%patch $11FDD $B8 $06 $00 $CD $10 $BA $D8 $03 $B0 $1A $EE $CB
",
        )
        .unwrap();
        let spec = prepare(&paths, &db, &m.profile_id).unwrap();
        let defaults = paths.profile(&m.profile_id).defaults_conf;
        assert!(spec.args.contains(&relative(&paths.root, &defaults)));
        let text = fs::read_to_string(&defaults).unwrap();
        assert!(text.starts_with(crate::dosconf::defaults(crate::catalog::Engine::Galaxy).1));
        assert!(text.contains(
            "[dosbox]
machine = cga
"
        ));

        // One that maps 128K of video memory gets a Paradise SVGA.
        fs::write(
            paths.mods.join("my-mod/files/MOD.PAT"),
            "%ext ck4
%patch $6E51 $BA $CE $03 $B0 $06 $EE $42 $EC $24 $F3 $EE
",
        )
        .unwrap();
        prepare(&paths, &db, &m.profile_id).unwrap();
        assert!(fs::read_to_string(&defaults).unwrap().contains(
            "[dosbox]
machine = svga_paradise
"
        ));

        // Without its game the mod can't start.
        crate::library::remove(&paths, &db, "keen4-1", false, false).unwrap();
        match prepare(&paths, &db, &m.profile_id) {
            Err(Error::MissingGame(game)) => assert!(game.starts_with("Commander Keen 4")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_first_presented_mode_means_the_window_is_up() {
        let shows = |l: &str| shows_window(l.as_bytes());
        assert!(!shows(
            "16:54:38.921 | DISPLAY: Using 948x711 window size in windowed mode on display-0\n"
        ));
        assert!(shows(
            "16:54:42.962 | DISPLAY: VGA 720x400 16-colour text mode 03h at 70.087 Hz, \
             scaled to 1067x800 pixels with 1:1.35 (20:27) pixel aspect ratio\n"
        ));
        assert!(!shows(
            "16:54:42.962 | DISPLAY: Presenting at DOS rate without vsync\n"
        ));
    }

    #[test]
    fn game_icon_has_an_image_for_every_size() {
        // PNG images; the 256 one is stored with width 0.
        for size in [16, 20, 24, 32, 40, 48, 64, 96, 256, 512] {
            let image = icon_image(GAME_ICON, size).unwrap();
            assert_eq!(&image[1..4], b"PNG");
            let width = u32::from_be_bytes(image[16..20].try_into().unwrap());
            assert_eq!(
                width,
                if size == 96 || size == 512 {
                    256
                } else {
                    size.min(256)
                }
            );
        }
        assert_eq!(icon_image(b"", 32), None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_reads_the_game_icon() {
        for size in [16, 24, 32, 48, 64] {
            assert_ne!(game_icon(size), 0, "{size}");
        }
    }

    #[test]
    fn sessions_are_tracked() {
        let (_tmp, _paths, db) = setup();
        let id = start_session(&db, "keen4-1", None, 100).unwrap();
        assert_eq!(end_session(&db, id, 160, Some(0)).unwrap(), 60);
        let p = &profiles::list(&db, |_| false).unwrap()[0];
        assert_eq!(
            (p.session_count, p.play_seconds, p.last_played_at),
            (1, 60, Some(100))
        );
    }

    #[test]
    fn first_sessions_are_those_with_a_new_setup() {
        let (_tmp, _paths, db) = setup();
        let game = start_session(&db, "keen4-1", None, 100).unwrap();
        assert!(!first_with_setup(&db, game).unwrap());
        let first = start_session(&db, "keen4-1", Some("a"), 200).unwrap();
        let again = start_session(&db, "keen4-1", Some("a"), 300).unwrap();
        let changed = start_session(&db, "keen4-1", Some("b"), 400).unwrap();
        let back = start_session(&db, "keen4-1", Some("a"), 500).unwrap();
        assert!(first_with_setup(&db, first).unwrap());
        assert!(!first_with_setup(&db, again).unwrap());
        assert!(first_with_setup(&db, changed).unwrap());
        assert!(!first_with_setup(&db, back).unwrap());
    }

    #[test]
    fn one_instance_per_profile() {
        let running = Running::default();
        let a = running.claim("a").unwrap();
        assert!(matches!(running.claim("a"), Err(Error::AlreadyRunning)));
        let _b = running.claim("b").unwrap();
        assert!(running.contains("a"));
        drop(a);
        assert!(!running.contains("a"));
        let _a = running.claim("a").unwrap();
    }

    #[test]
    fn claims_all_or_none() {
        let running = Running::default();
        let b = running.claim("b").unwrap();
        assert!(matches!(
            running.claim_all(["a", "b", "c"]),
            Err(Error::AlreadyRunning)
        ));
        assert!(!running.contains("a") && !running.contains("c"));
        drop(b);
        let all = running.claim_all(["a", "b", "b"]).unwrap();
        assert!(running.contains("a") && running.contains("b"));
        drop(all);
        assert!(!running.contains("a") && !running.contains("b"));
        let _none = running.claim_all(None::<String>).unwrap();
    }

    #[test]
    fn quitting_stops_games_or_ends_their_sessions() {
        let (_tmp, _paths, db) = setup();
        let playing = Playing::default();
        let mut stopped = playing.add("a", 1);
        let mut left = playing.add("keen4-1", start_session(&db, "keen4-1", None, 100).unwrap());
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
        let (mut child, session) = spawn(&rt, &spec, &db, id, || {}).unwrap();
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
                    extends: None,
                    replace: None,
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
                let (mut child, session) = spawn(&rt, &spec, &db, &m.profile_id, || {}).unwrap();
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

//! The DOSBox Staging runtime: pinned downloads, install, custom executables
//! (PLAN.md §2.1).

use std::fs::{self, File};
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::params;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::db::Db;
use crate::downloads;
use crate::error::{Error, Result};
use crate::jobs::JobPhase;
use crate::paths::AppPaths;

pub const KIND: &str = "dosbox-staging";
const CUSTOM_ID: &str = "custom";

#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    downloads: Vec<Download>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Download {
    os: String,
    arch: Vec<String>,
    url: String,
    sha256: String,
    size: u64,
    archive: ArchiveKind,
    /// Relative to the extracted folder.
    executable: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ArchiveKind {
    Zip,
    TarXz,
    Dmg,
}

static MANIFEST: LazyLock<Manifest> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../runtimes/dosbox-staging.json"))
        .expect("bundled runtime manifest is invalid")
});

pub fn pinned_version() -> &'static str {
    &MANIFEST.version
}

fn download_for(os: &str, arch: &str) -> Option<&'static Download> {
    MANIFEST
        .downloads
        .iter()
        .find(|d| d.os == os && d.arch.iter().any(|a| a == arch))
}

/// The pinned build for the platform we're running on.
pub fn current_download() -> Option<&'static Download> {
    download_for(std::env::consts::OS, std::env::consts::ARCH)
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub id: String,
    pub version: String,
    /// Path to the executable.
    pub path: String,
    /// Downloaded and managed by BloogBase, as opposed to a custom executable.
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    /// The runtime launches use; `None` if DOSBox still has to be installed.
    pub active: Option<Runtime>,
    /// The managed runtime, if installed (also when a custom one is active).
    pub managed: Option<Runtime>,
    pub pinned_version: String,
    /// Whether a managed download exists for this OS/architecture.
    pub download_available: bool,
}

pub fn status(db: &Db) -> Result<RuntimeStatus> {
    let runtimes = list(db)?;
    let custom = runtimes.iter().find(|r| !r.managed).cloned();
    let managed = runtimes
        .iter()
        .filter(|r| r.managed)
        // Prefer the pinned version, else the newest one that's installed.
        .max_by_key(|r| (r.version == pinned_version(), version_key(&r.version)))
        .cloned();
    Ok(RuntimeStatus {
        active: custom.or_else(|| managed.clone()),
        managed,
        pinned_version: pinned_version().to_string(),
        download_available: current_download().is_some(),
    })
}

/// The runtime to launch with, or `RuntimeMissing`.
pub fn active(db: &Db) -> Result<Runtime> {
    status(db)?.active.ok_or(Error::RuntimeMissing)
}

/// Registered runtimes whose executable still exists.
fn list(db: &Db) -> Result<Vec<Runtime>> {
    let conn = db.conn();
    let mut stmt =
        conn.prepare("SELECT id, version, path, managed FROM runtimes WHERE kind = ?1")?;
    let rows = stmt.query_map([KIND], |r| {
        Ok(Runtime {
            id: r.get(0)?,
            version: r.get(1)?,
            path: r.get(2)?,
            managed: r.get(3)?,
        })
    })?;
    let all: Vec<Runtime> = rows.collect::<rusqlite::Result<_>>()?;
    Ok(all
        .into_iter()
        .filter(|r| {
            let exists = Path::new(&r.path).is_file();
            if !exists {
                tracing::warn!("runtime {} is gone: {}", r.id, r.path);
            }
            exists
        })
        .collect())
}

fn version_key(v: &str) -> Vec<u32> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter_map(|p| p.parse().ok())
        .collect()
}

fn register(db: &Db, rt: &Runtime) -> Result<()> {
    db.conn().execute(
        "INSERT OR REPLACE INTO runtimes (id, kind, version, path, managed)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![rt.id, KIND, rt.version, rt.path, rt.managed],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Custom executable

/// Sets (`Some`) or clears (`None`) the user's own DOSBox Staging executable.
pub fn set_custom(db: &Db, path: Option<&Path>) -> Result<RuntimeStatus> {
    match path {
        None => {
            db.conn()
                .execute("DELETE FROM runtimes WHERE id = ?1", [CUSTOM_ID])?;
        }
        Some(path) => {
            if !path.is_file() {
                return Err(Error::InvalidRuntime(format!(
                    "{} is not a file",
                    path.display()
                )));
            }
            let version = probe(path)?;
            register(
                db,
                &Runtime {
                    id: CUSTOM_ID.into(),
                    version,
                    path: path.display().to_string(),
                    managed: false,
                },
            )?;
        }
    }
    status(db)
}

/// Runs `<exe> --version` and returns the DOSBox Staging version it reports.
/// Plain DOSBox and other forks are rejected: launches rely on Staging's
/// overlay mounts and command-line switches.
fn probe(exe: &Path) -> Result<String> {
    let mut cmd = Command::new(exe);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::InvalidRuntime(format!("could not start {}: {e}", exe.display())))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait()?.is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::InvalidRuntime("no answer to --version".into()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = child.wait_with_output()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    parse_version_output(&text).ok_or_else(|| {
        let first = text.lines().next().unwrap_or("").trim();
        Error::InvalidRuntime(format!("not DOSBox Staging (reported \"{first}\")"))
    })
}

/// Extracts the version from Staging's `--version` banner, e.g.
/// `dosbox-staging, version 0.83.0`.
fn parse_version_output(text: &str) -> Option<String> {
    let line = text.lines().find(|l| {
        let l = l.to_ascii_lowercase();
        l.contains("dosbox-staging") || l.contains("dosbox staging")
    })?;
    line.split(|c: char| c.is_whitespace() || c == ',')
        .map(|w| w.trim_start_matches(['v', 'V']))
        .find(|w| w.starts_with(|c: char| c.is_ascii_digit()) && w.contains('.'))
        .map(str::to_string)
}

/// Keeps console-subsystem programs (like DOSBox on Windows) from opening a
/// console window when started from the GUI.
pub fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

// ---------------------------------------------------------------------------
// Managed install

static INSTALLING: AtomicBool = AtomicBool::new(false);

struct InstallGuard;

impl InstallGuard {
    fn acquire() -> Result<Self> {
        INSTALLING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| InstallGuard)
            .map_err(|_| Error::Busy)
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::Release);
    }
}

/// Downloads, verifies and installs the pinned DOSBox Staging build.
/// `progress` gets the phase and (received, total) bytes.
pub async fn install(
    paths: AppPaths,
    db: &Db,
    progress: impl Fn(JobPhase, u64, u64),
) -> Result<RuntimeStatus> {
    let _guard = InstallGuard::acquire()?;
    let dl = current_download().ok_or(Error::UnsupportedPlatform)?;
    let file_name = dl.url.rsplit('/').next().unwrap_or("dosbox-staging");
    let archive = paths.cache.join("downloads").join(file_name);

    tracing::info!(
        "downloading DOSBox Staging {} from {}",
        pinned_version(),
        dl.url
    );
    downloads::fetch(
        &dl.url,
        &dl.sha256,
        dl.size,
        &archive,
        &|received, total| progress(JobPhase::Downloading, received, total),
    )
    .await?;

    progress(JobPhase::Installing, dl.size, dl.size);
    let version = pinned_version().to_string();
    let a = archive.clone();
    let rt = tokio::task::spawn_blocking(move || install_archive(&paths, &version, dl, &a))
        .await
        .map_err(|e| Error::Extract(e.to_string()))?;
    let _ = fs::remove_file(&archive);
    let rt = rt?;
    register(db, &rt)?;
    tracing::info!("installed DOSBox Staging {} at {}", rt.version, rt.path);
    status(db)
}

/// Extracts a verified archive into `runtimes/dosbox-staging/<version>/`,
/// replacing an earlier install of the same version.
fn install_archive(
    paths: &AppPaths,
    version: &str,
    dl: &Download,
    archive: &Path,
) -> Result<Runtime> {
    let parent = paths.runtimes.join(KIND);
    let target = parent.join(version);
    let staging = parent.join(format!(".{version}.partial"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;

    let result = (|| {
        let out = staging.join("out");
        fs::create_dir(&out)?;
        extract(dl.archive, archive, &out, &staging)?;
        let root = content_root(&out)?;
        if !root.join(&dl.executable).is_file() {
            return Err(Error::Extract(format!(
                "{} is missing from the archive",
                dl.executable
            )));
        }
        if target.exists() {
            fs::remove_dir_all(&target)?;
        }
        fs::rename(&root, &target)?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    result?;

    let exe = target.join(&dl.executable);
    make_executable(&exe)?;
    if dl.archive != ArchiveKind::Dmg {
        // Portable mode: Staging keeps its config folder (shell history,
        // mapper files) next to the executable instead of sharing the user's
        // own. Not for the macOS app bundle, writing into it breaks its signature.
        let marker = exe.with_file_name("dosbox-staging.conf");
        fs::write(
            marker,
            "# Portable mode for BloogBase. Launches don't load this file.\n",
        )?;
    }
    Ok(Runtime {
        id: format!("{KIND}-{version}"),
        version: version.into(),
        path: exe.display().to_string(),
        managed: true,
    })
}

fn extract(kind: ArchiveKind, archive: &Path, out: &Path, scratch: &Path) -> Result<()> {
    match kind {
        ArchiveKind::Zip => zip::ZipArchive::new(File::open(archive)?)?.extract(out)?,
        ArchiveKind::TarXz => {
            // Runtime archives are a few dozen MB, decoding into memory is fine.
            let mut tar = Vec::new();
            lzma_rs::xz_decompress(&mut BufReader::new(File::open(archive)?), &mut tar)
                .map_err(|e| Error::Extract(e.to_string()))?;
            tar::Archive::new(Cursor::new(tar)).unpack(out)?;
        }
        ArchiveKind::Dmg => extract_dmg(archive, out, scratch)?,
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn extract_dmg(archive: &Path, out: &Path, scratch: &Path) -> Result<()> {
    let mount = scratch.join("mnt");
    fs::create_dir_all(&mount)?;
    run(Command::new("hdiutil")
        .args([
            "attach",
            "-nobrowse",
            "-readonly",
            "-noautoopen",
            "-mountpoint",
        ])
        .arg(&mount)
        .arg(archive))?;
    let copied = (|| {
        let mut found = false;
        for entry in fs::read_dir(&mount)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_string_lossy().ends_with(".app") {
                // `ditto` keeps the bundle's symlinks, permissions and signature intact.
                run(Command::new("ditto").arg(entry.path()).arg(out.join(&name)))?;
                found = true;
            }
        }
        if found {
            Ok(())
        } else {
            Err(Error::Extract(
                "no application found in the disk image".into(),
            ))
        }
    })();
    let _ = run(Command::new("hdiutil")
        .arg("detach")
        .arg(&mount)
        .arg("-quiet"));
    copied?;
    // Only reached for a verified download; best effort, the attribute is
    // usually absent because we downloaded the file ourselves.
    let _ = Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(out)
        .status();
    Ok(())
}

#[cfg(target_os = "macos")]
fn run(cmd: &mut Command) -> Result<()> {
    let out = cmd.output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::Extract(format!(
            "{:?} failed: {}",
            cmd.get_program(),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(not(target_os = "macos"))]
fn extract_dmg(_archive: &Path, _out: &Path, _scratch: &Path) -> Result<()> {
    Err(Error::Extract(
        "disk images can only be opened on macOS".into(),
    ))
}

/// Archives usually wrap everything in one versioned folder; use its content.
fn content_root(dir: &Path) -> Result<PathBuf> {
    let entries: Vec<_> = fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
    match entries.as_slice() {
        [only]
            if only.file_type()?.is_dir()
                && !only.file_name().to_string_lossy().ends_with(".app") =>
        {
            Ok(only.path())
        }
        _ => Ok(dir.to_path_buf()),
    }
}

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = fs::metadata(path)?.permissions();
        perm.set_mode(perm.mode() | 0o755);
        fs::set_permissions(path, perm)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::testutil::write_zip;

    fn setup() -> (tempfile::TempDir, AppPaths, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        (tmp, paths, Db::open_in_memory().unwrap())
    }

    fn fake_download(archive: ArchiveKind, executable: &str) -> Download {
        Download {
            os: "test".into(),
            arch: vec!["test".into()],
            url: "https://example.invalid/x".into(),
            sha256: String::new(),
            size: 0,
            archive,
            executable: executable.into(),
        }
    }

    #[test]
    fn manifest_covers_desktop_platforms() {
        for (os, arch) in [
            ("windows", "x86_64"),
            ("linux", "x86_64"),
            ("macos", "x86_64"),
            ("macos", "aarch64"),
        ] {
            let dl = download_for(os, arch).unwrap_or_else(|| panic!("{os}/{arch}"));
            assert!(dl.url.starts_with("https://github.com/dosbox-staging/"));
            assert!(dl.url.contains(&MANIFEST.version));
            assert_eq!(dl.sha256.len(), 64);
            assert!(dl.size > 0);
        }
        assert!(download_for("linux", "aarch64").is_none());
    }

    #[test]
    fn parses_staging_version_banner() {
        assert_eq!(
            parse_version_output("dosbox-staging, version 0.83.0\n\nCopyright (C) ...").as_deref(),
            Some("0.83.0")
        );
        assert_eq!(
            parse_version_output("DOSBox Staging version v0.84.0-alpha-0fdf9").as_deref(),
            Some("0.84.0-alpha-0fdf9")
        );
        assert_eq!(parse_version_output("DOSBox version 0.74-3"), None);
        assert_eq!(parse_version_output(""), None);
    }

    #[test]
    fn installs_zip_and_strips_top_folder() {
        let (tmp, paths, db) = setup();
        let archive = tmp.path().join("dosbox.zip");
        write_zip(
            &archive,
            &[
                ("dosbox-staging-v9/dosbox.exe", b"exe"),
                ("dosbox-staging-v9/resources/x.txt", b"x"),
            ],
        );
        let rt = install_archive(
            &paths,
            "9.0",
            &fake_download(ArchiveKind::Zip, "dosbox.exe"),
            &archive,
        )
        .unwrap();
        let root = paths.runtimes.join("dosbox-staging/9.0");
        assert_eq!(PathBuf::from(&rt.path), root.join("dosbox.exe"));
        assert!(root.join("resources/x.txt").is_file());
        // No leftovers next to the install.
        let siblings: Vec<_> = fs::read_dir(paths.runtimes.join("dosbox-staging"))
            .unwrap()
            .collect();
        assert_eq!(siblings.len(), 1);

        register(&db, &rt).unwrap();
        let st = status(&db).unwrap();
        assert_eq!(st.active.unwrap().id, "dosbox-staging-9.0");

        // Reinstalling the same version replaces it.
        install_archive(
            &paths,
            "9.0",
            &fake_download(ArchiveKind::Zip, "dosbox.exe"),
            &archive,
        )
        .unwrap();
        assert!(root.join("dosbox.exe").is_file());
    }

    #[test]
    fn installs_tar_xz() {
        let (tmp, paths, _db) = setup();
        let mut tar = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "dosbox-linux/dosbox", &b"elf"[..])
            .unwrap();
        let tar = tar.into_inner().unwrap();
        let mut xz = Vec::new();
        lzma_rs::xz_compress(&mut Cursor::new(tar), &mut xz).unwrap();
        let archive = tmp.path().join("dosbox.tar.xz");
        fs::write(&archive, xz).unwrap();

        let rt = install_archive(
            &paths,
            "9.0",
            &fake_download(ArchiveKind::TarXz, "dosbox"),
            &archive,
        )
        .unwrap();
        assert_eq!(fs::read(&rt.path).unwrap(), b"elf");
    }

    #[test]
    fn missing_executable_fails_cleanly() {
        let (tmp, paths, _db) = setup();
        let archive = tmp.path().join("dosbox.zip");
        write_zip(&archive, &[("top/readme.txt", b"x")]);
        let res = install_archive(
            &paths,
            "9.0",
            &fake_download(ArchiveKind::Zip, "dosbox.exe"),
            &archive,
        );
        assert!(matches!(res, Err(Error::Extract(_))));
        assert!(!paths.runtimes.join("dosbox-staging/9.0").exists());
        assert_eq!(
            fs::read_dir(paths.runtimes.join("dosbox-staging"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn custom_runtime_wins_and_vanishes_when_deleted() {
        let (tmp, paths, db) = setup();
        assert!(matches!(active(&db), Err(Error::RuntimeMissing)));

        let managed = paths.runtimes.join("dosbox.exe");
        let custom = tmp.path().join("my-dosbox.exe");
        fs::write(&managed, b"").unwrap();
        fs::write(&custom, b"").unwrap();
        for (id, path, is_managed) in [
            ("dosbox-staging-0.1", &managed, true),
            (CUSTOM_ID, &custom, false),
        ] {
            register(
                &db,
                &Runtime {
                    id: id.into(),
                    version: "0.1".into(),
                    path: path.display().to_string(),
                    managed: is_managed,
                },
            )
            .unwrap();
        }
        assert_eq!(active(&db).unwrap().id, CUSTOM_ID);
        assert_eq!(
            status(&db).unwrap().managed.unwrap().id,
            "dosbox-staging-0.1"
        );

        fs::remove_file(&custom).unwrap();
        assert_eq!(active(&db).unwrap().id, "dosbox-staging-0.1");
        set_custom(&db, None).unwrap();
        assert!(matches!(
            set_custom(&db, Some(&tmp.path().join("nope"))),
            Err(Error::InvalidRuntime(_))
        ));
    }

    #[test]
    fn install_guard_is_exclusive() {
        let g = InstallGuard::acquire().unwrap();
        assert!(matches!(InstallGuard::acquire(), Err(Error::Busy)));
        drop(g);
        drop(InstallGuard::acquire().unwrap());
    }
}

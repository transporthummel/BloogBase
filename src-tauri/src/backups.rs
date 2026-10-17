//! Backups of what a profile's game wrote (PLAN.md §4.4, §11): by hand, or
//! automatically before each launch and before anything BloogBase changes in
//! `state/` itself. Archives live in `backups/<profile-id>/` and carry a small
//! manifest so they can be exported and imported again.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use zip::write::SimpleFileOptions;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::library::now;
use crate::overlay_view;
use crate::paths::AppPaths;
use crate::profiles::{self, Layers};
use crate::saves::{self, SaveFile};
use crate::settings;

const MANIFEST: &str = "bloogbase-backup.json";

/// Archive entries: path inside the archive, content.
type Files = Vec<(String, Vec<u8>)>;

/// Settings keys: automatic backup before each launch, and how many of
/// the automatic backups to keep per profile.
pub const AUTO_KEY: &str = "backups.auto";
pub const KEEP_KEY: &str = "backups.keep";
pub const DEFAULT_KEEP: u32 = 10;

/// Why BloogBase made a backup on its own. Stored in the label column with
/// a prefix no user label gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    BeforeLaunch,
    BeforeRestore,
    BeforeDelete,
    ModRemoved,
}

const REASON_PREFIX: &str = "bloogbase:";

impl Reason {
    fn key(self) -> &'static str {
        match self {
            Reason::BeforeLaunch => "before-launch",
            Reason::BeforeRestore => "before-restore",
            Reason::BeforeDelete => "before-delete",
            Reason::ModRemoved => "mod-removed",
        }
    }

    fn from_label(label: &str) -> Option<Reason> {
        // "Mod removed" is what M4 wrote before reasons existed.
        if label == "Mod removed" {
            return Some(Reason::ModRemoved);
        }
        let key = label.strip_prefix(REASON_PREFIX)?;
        [
            Reason::BeforeLaunch,
            Reason::BeforeRestore,
            Reason::BeforeDelete,
            Reason::ModRemoved,
        ]
        .into_iter()
        .find(|r| r.key() == key)
    }
}

/// What a backup is made of.
pub enum Contents<'a> {
    /// Everything in `state/`, deletion markers included, so a restore
    /// reproduces exactly what DOS saw (§2.2).
    State,
    /// These files as DOS sees them (they may come from the lower layer).
    Files(&'a [String]),
}

/// Who asked for a backup.
pub enum Origin<'a> {
    User(Option<&'a str>),
    Auto(Reason),
}

impl From<Reason> for Origin<'_> {
    fn from(r: Reason) -> Self {
        Origin::Auto(r)
    }
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub id: u32,
    pub profile_id: String,
    /// The user's label; automatic backups have a `reason` instead.
    pub label: Option<String>,
    pub reason: Option<Reason>,
    pub created_at: u32,
    pub files: Vec<String>,
    pub auto: bool,
    pub size: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    app: String,
    game_id: String,
    profile_id: String,
    label: Option<String>,
    created_at: u32,
    files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettings {
    /// Back up before every launch.
    pub auto: bool,
    /// Automatic backups kept per profile; older ones are removed.
    pub keep: u32,
}

pub fn get_settings(db: &Db) -> Result<BackupSettings> {
    Ok(BackupSettings {
        auto: settings::get(db, AUTO_KEY)?.is_none_or(|v| v != "0"),
        keep: settings::get(db, KEEP_KEY)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_KEEP),
    })
}

pub fn set_settings(db: &Db, s: &BackupSettings) -> Result<()> {
    settings::set(db, AUTO_KEY, if s.auto { "1" } else { "0" })?;
    settings::set(db, KEEP_KEY, &s.keep.clamp(1, 100).to_string())
}

/// The files to archive, as (path in the archive, content).
fn collect(layers: &Layers, contents: Contents<'_>) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    match contents {
        Contents::State => {
            for entry in walkdir::WalkDir::new(&layers.upper) {
                let entry = match entry {
                    Ok(e) => e,
                    // No state yet: nothing to back up.
                    Err(e) if e.depth() == 0 => return Ok(out),
                    Err(e) => return Err(std::io::Error::from(e).into()),
                };
                if entry.file_type().is_file() {
                    let rel = entry
                        .path()
                        .strip_prefix(&layers.upper)
                        .expect("walkdir yields children");
                    out.push((
                        rel.to_string_lossy().replace('\\', "/"),
                        fs::read(entry.path())?,
                    ));
                }
            }
        }
        Contents::Files(files) => {
            let view = overlay_view::list(layers.view_lower(), &layers.upper)?;
            for file in files {
                let entry = view
                    .iter()
                    .find(|e| &e.path == file)
                    .ok_or_else(|| Error::NotFound(file.clone()))?;
                out.push((
                    entry.path.clone(),
                    fs::read(saves::host_path(layers, entry))?,
                ));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn content_hash(files: &[(String, Vec<u8>)]) -> String {
    let mut h = Sha256::new();
    for (path, data) in files {
        h.update(path.as_bytes());
        h.update([0]);
        h.update((data.len() as u64).to_le_bytes());
        h.update(data);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Archives a profile's files. Returns `None` if there was nothing to back
/// up, or, for automatic backups, nothing changed since the last backup.
pub fn create<'a>(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    contents: Contents<'_>,
    origin: impl Into<Origin<'a>>,
) -> Result<Option<Backup>> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let files = collect(&layers, contents)?;
    if files.is_empty() {
        return Ok(None);
    }
    let (label, auto) = match origin.into() {
        Origin::User(label) => (
            label
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from),
            false,
        ),
        Origin::Auto(reason) => (Some(format!("{REASON_PREFIX}{}", reason.key())), true),
    };
    let hash = content_hash(&files);
    if auto {
        let last: Option<Option<String>> = db
            .conn()
            .query_row(
                "SELECT content_hash FROM save_backups WHERE profile_id = ?1
                 ORDER BY created_at DESC, id DESC LIMIT 1",
                [profile_id],
                |r| r.get(0),
            )
            .optional()?;
        if last.flatten().as_deref() == Some(hash.as_str()) {
            tracing::debug!("{profile_id} unchanged since its last backup");
            return Ok(None);
        }
    }

    let created_at = now() as u32;
    let names: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
    let manifest = Manifest {
        app: "BloogBase".into(),
        game_id: profile.game_id.clone(),
        profile_id: profile_id.to_string(),
        label: label.clone(),
        created_at,
        files: names.clone(),
    };
    let archive = new_archive_path(paths, profile_id, created_at)?;
    write_zip(&archive, &files, &manifest)?;

    let id = {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO save_backups
                 (profile_id, label, archive_path, created_at, files_json, auto, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                profile_id,
                label,
                archive.display().to_string(),
                created_at,
                serde_json::to_string(&names).expect("strings serialize"),
                auto,
                hash
            ],
        )?;
        conn.last_insert_rowid() as u32
    };
    tracing::info!("backed up {} files of {profile_id}", names.len());
    if auto {
        prune(db, profile_id, get_settings(db)?.keep)?;
    }
    get(db, id).map(Some)
}

fn new_archive_path(paths: &AppPaths, profile_id: &str, at: u32) -> Result<PathBuf> {
    let dir = paths.backups.join(profile_id);
    fs::create_dir_all(&dir)?;
    let mut n = 0;
    loop {
        let name = if n == 0 {
            format!("{at}.zip")
        } else {
            format!("{at}-{n}.zip")
        };
        let path = dir.join(name);
        if !path.exists() {
            return Ok(path);
        }
        n += 1;
    }
}

fn write_zip(path: &Path, files: &[(String, Vec<u8>)], manifest: &Manifest) -> Result<()> {
    let tmp = path.with_extension("zip.part");
    {
        let mut zip = zip::ZipWriter::new(File::create(&tmp)?);
        for (rel, data) in files {
            zip.start_file(rel.as_str(), SimpleFileOptions::default())?;
            zip.write_all(data)?;
        }
        zip.start_file(MANIFEST, SimpleFileOptions::default())?;
        zip.write_all(&serde_json::to_vec_pretty(manifest).expect("manifest serializes"))?;
        zip.finish()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Removes the oldest automatic backups beyond `keep`.
fn prune(db: &Db, profile_id: &str, keep: u32) -> Result<()> {
    let old: Vec<(u32, String)> = {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, archive_path FROM save_backups WHERE profile_id = ?1 AND auto = 1
             ORDER BY created_at DESC, id DESC LIMIT -1 OFFSET ?2",
        )?;
        stmt.query_map(params![profile_id, keep], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    for (id, archive) in old {
        remove(db, id, &archive)?;
    }
    Ok(())
}

fn remove(db: &Db, id: u32, archive: &str) -> Result<()> {
    if let Err(e) = fs::remove_file(archive)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(e.into());
    }
    db.conn()
        .execute("DELETE FROM save_backups WHERE id = ?1", [id])?;
    Ok(())
}

const SELECT: &str = "SELECT id, profile_id, label, created_at, files_json, auto, archive_path
                      FROM save_backups";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(Backup, String)> {
    let label: Option<String> = r.get(2)?;
    let files: String = r.get(4)?;
    let archive: String = r.get(6)?;
    let reason = label.as_deref().and_then(Reason::from_label);
    Ok((
        Backup {
            id: r.get(0)?,
            profile_id: r.get(1)?,
            label: if reason.is_some() { None } else { label },
            reason,
            created_at: r.get(3)?,
            files: serde_json::from_str(&files).unwrap_or_default(),
            auto: r.get(5)?,
            size: fs::metadata(&archive).map_or(0, |m| m.len() as u32),
        },
        archive,
    ))
}

fn get_with_archive(db: &Db, id: u32) -> Result<(Backup, String)> {
    db.conn()
        .query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("backup {id}")))
}

pub fn get(db: &Db, id: u32) -> Result<Backup> {
    Ok(get_with_archive(db, id)?.0)
}

/// A profile's backups, newest first.
pub fn list(db: &Db, profile_id: &str) -> Result<Vec<Backup>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE profile_id = ?1 ORDER BY created_at DESC, id DESC"
    ))?;
    let rows = stmt
        .query_map([profile_id], from_row)?
        .map(|r| r.map(|(b, _)| b))
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

pub fn delete(db: &Db, id: u32) -> Result<()> {
    let (_, archive) = get_with_archive(db, id)?;
    remove(db, id, &archive)
}

/// The archive's files except the manifest, read in full (Keen saves are small).
fn read_zip(path: &Path) -> Result<(Files, Option<Manifest>)> {
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    let mut files = Vec::new();
    let mut manifest = None;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if !f.is_file() {
            continue;
        }
        // Refuses absolute paths and `..`.
        let Some(name) = f.enclosed_name() else {
            continue;
        };
        let name = name.to_string_lossy().replace('\\', "/");
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        if name == MANIFEST {
            manifest = serde_json::from_slice(&data).ok();
        } else {
            files.push((name, data));
        }
    }
    Ok((files, manifest))
}

/// The saves in a backup, described like the profile's current ones.
pub fn saves_in(db: &Db, id: u32) -> Result<Vec<SaveFile>> {
    let (backup, archive) = get_with_archive(db, id)?;
    let (profile, _) = profiles::get(db, &backup.profile_id)?;
    let game = saves::game_of(&profile.game_id)?;
    let (files, manifest) = read_zip(Path::new(&archive))?;
    let at = manifest.map_or(backup.created_at, |m| m.created_at);
    Ok(files
        .iter()
        .filter(|(name, _)| saves::is_save(game, name))
        .map(|(name, data)| saves::describe(game, name, data, Some(at)))
        .collect())
}

/// Puts a backup's files back into `state/`, after backing up what's there.
/// With `file`, only that file, written under the name `target` (another
/// slot) if given. Files not in the backup stay as they are.
pub fn restore(
    paths: &AppPaths,
    db: &Db,
    id: u32,
    file: Option<&str>,
    target: Option<&str>,
) -> Result<()> {
    let (backup, archive) = get_with_archive(db, id)?;
    let (profile, layers) = profiles::get(db, &backup.profile_id)?;
    let game = saves::game_of(&profile.game_id)?;
    let (files, _) = read_zip(Path::new(&archive))?;
    let chosen: Vec<(String, &[u8])> = match file {
        None => files
            .iter()
            .map(|(n, d)| (n.clone(), d.as_slice()))
            .collect(),
        Some(file) => {
            let (_, data) = files
                .iter()
                .find(|(n, _)| n == file)
                .ok_or_else(|| Error::NotFound(format!("{file} in backup {id}")))?;
            let name = target.unwrap_or(file);
            if !saves::is_save(game, name) && name != file {
                return Err(Error::NotFound(format!("save slot {name}")));
            }
            vec![(name.to_string(), data.as_slice())]
        }
    };

    create(
        paths,
        db,
        &backup.profile_id,
        Contents::State,
        Reason::BeforeRestore,
    )?;
    fs::create_dir_all(&layers.upper)?;
    for (name, data) in chosen {
        let file_name = name.rsplit('/').next().unwrap_or(&name);
        if !overlay_view::is_marker(file_name) {
            overlay_view::prepare_write(&layers.upper, &name)?;
        }
        // The name may differ in case from the file DOS made; replace that one.
        let dest = existing_ci(&layers.upper, &name).unwrap_or_else(|| layers.upper.join(&name));
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(dest, data)?;
    }
    tracing::info!("restored backup {id} into {}", backup.profile_id);
    Ok(())
}

/// A file in `dir` whose `/`-separated relative name matches case-insensitively.
fn existing_ci(dir: &Path, rel: &str) -> Option<PathBuf> {
    let mut path = dir.to_path_buf();
    for part in rel.split('/') {
        let found = fs::read_dir(&path)
            .ok()?
            .filter_map(|e| e.ok())
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        path = found.path();
    }
    Some(path)
}

pub fn export(db: &Db, id: u32, dest: &Path) -> Result<()> {
    let (_, archive) = get_with_archive(db, id)?;
    fs::copy(archive, dest)?;
    Ok(())
}

/// Adds a ZIP as a new backup of a profile, to be restored from there.
/// BloogBase exports keep everything; other archives contribute the files
/// that look like this game's saves, config or scores, wherever they are.
pub fn import(paths: &AppPaths, db: &Db, profile_id: &str, src: &Path) -> Result<Backup> {
    let (profile, _) = profiles::get(db, profile_id)?;
    let game = saves::game_of(&profile.game_id)?;
    let (files, manifest) = read_zip(src).map_err(|e| match e {
        Error::Archive(_) => Error::UnsupportedSource(src.to_path_buf()),
        e => e,
    })?;
    let (mut files, label) = match manifest {
        Some(m) if m.game_id != profile.game_id => {
            return Err(Error::OtherGame(m.game_id));
        }
        Some(m) => (files, m.label.filter(|l| !l.starts_with(REASON_PREFIX))),
        None => {
            let mut picked: Vec<(String, Vec<u8>)> = Vec::new();
            for (name, data) in files {
                let base = name.rsplit('/').next().unwrap_or(&name).to_string();
                if game.is_state_file(&base)
                    && !picked.iter().any(|(n, _)| n.eq_ignore_ascii_case(&base))
                {
                    picked.push((base, data));
                }
            }
            let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned());
            (picked, stem)
        }
    };
    if files.is_empty() {
        return Err(Error::NoSavesFound);
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let created_at = now() as u32;
    let names: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
    let manifest = Manifest {
        app: "BloogBase".into(),
        game_id: profile.game_id.clone(),
        profile_id: profile_id.to_string(),
        label: label.clone(),
        created_at,
        files: names.clone(),
    };
    let archive = new_archive_path(paths, profile_id, created_at)?;
    write_zip(&archive, &files, &manifest)?;
    let id = {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO save_backups
                 (profile_id, label, archive_path, created_at, files_json, auto, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
            params![
                profile_id,
                label,
                archive.display().to_string(),
                created_at,
                serde_json::to_string(&names).expect("strings serialize"),
                content_hash(&files)
            ],
        )?;
        conn.last_insert_rowid() as u32
    };
    get(db, id)
}

/// The automatic backup before a launch (§6.2 step 3), if switched on.
/// Failing to back up must not keep anyone from playing: it's logged only.
pub fn before_launch(paths: &AppPaths, db: &Db, profile_id: &str) {
    let enabled = get_settings(db).map(|s| s.auto).unwrap_or(true);
    if !enabled {
        return;
    }
    if let Err(e) = create(paths, db, profile_id, Contents::State, Reason::BeforeLaunch) {
        tracing::warn!("backup before launching {profile_id} failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saves::tests::{keen4_save, setup, state};

    fn names(backups: &[Backup]) -> Vec<Option<Reason>> {
        backups.iter().map(|b| b.reason).collect()
    }

    #[test]
    fn state_backup_includes_markers_and_manifest() {
        let (_tmp, paths, db) = setup();
        fs::write(state(&paths).join("DBOVERLAY_DEL_X.CK4"), b"empty").unwrap();
        let b = create(
            &paths,
            &db,
            "keen4-1",
            Contents::State,
            Origin::User(Some(" Boss ")),
        )
        .unwrap()
        .unwrap();
        assert_eq!(b.label.as_deref(), Some("Boss"));
        assert_eq!(b.reason, None);
        assert!(!b.auto);
        assert_eq!(
            b.files,
            ["DBOVERLAY_DEL_X.CK4", "SAVEGAM0.CK4", "config.ck4"]
        );
        let (_, archive) = get_with_archive(&db, b.id).unwrap();
        let (files, manifest) = read_zip(Path::new(&archive)).unwrap();
        assert_eq!(files.len(), 3);
        let m = manifest.unwrap();
        assert_eq!(
            (m.game_id.as_str(), m.label.as_deref()),
            ("keen4", Some("Boss"))
        );
    }

    #[test]
    fn auto_backups_skip_unchanged_state_and_are_pruned() {
        let (_tmp, paths, db) = setup();
        set_settings(
            &db,
            &BackupSettings {
                auto: true,
                keep: 2,
            },
        )
        .unwrap();
        before_launch(&paths, &db, "keen4-1");
        before_launch(&paths, &db, "keen4-1");
        assert_eq!(list(&db, "keen4-1").unwrap().len(), 1);

        for i in 0..3u8 {
            fs::write(state(&paths).join("SAVEGAM1.CK4"), [i]).unwrap();
            before_launch(&paths, &db, "keen4-1");
        }
        // A manual one is never pruned.
        create(&paths, &db, "keen4-1", Contents::State, Origin::User(None)).unwrap();
        let all = list(&db, "keen4-1").unwrap();
        assert_eq!(all.iter().filter(|b| b.auto).count(), 2);
        assert_eq!(all.iter().filter(|b| !b.auto).count(), 1);
        let on_disk = fs::read_dir(paths.backups.join("keen4-1")).unwrap().count();
        assert_eq!(on_disk, 3);
        assert_eq!(names(&all[1..]), [Some(Reason::BeforeLaunch); 2]);

        set_settings(
            &db,
            &BackupSettings {
                auto: false,
                keep: 2,
            },
        )
        .unwrap();
        fs::write(state(&paths).join("SAVEGAM1.CK4"), b"new").unwrap();
        before_launch(&paths, &db, "keen4-1");
        assert_eq!(list(&db, "keen4-1").unwrap().len(), 3);
    }

    #[test]
    fn settings_default_and_clamp() {
        let (_tmp, _paths, db) = setup();
        let s = get_settings(&db).unwrap();
        assert!(s.auto);
        assert_eq!(s.keep, DEFAULT_KEEP);
        set_settings(
            &db,
            &BackupSettings {
                auto: false,
                keep: 0,
            },
        )
        .unwrap();
        let s = get_settings(&db).unwrap();
        assert!(!s.auto);
        assert_eq!(s.keep, 1);
    }

    #[test]
    fn restore_all_or_one_file_into_another_slot() {
        let (_tmp, paths, db) = setup();
        let st = state(&paths);
        fs::write(st.join("SAVEGAM0.CK4"), keen4_save("old")).unwrap();
        let b = create(&paths, &db, "keen4-1", Contents::State, Origin::User(None))
            .unwrap()
            .unwrap();
        fs::write(st.join("SAVEGAM0.CK4"), keen4_save("new")).unwrap();
        fs::write(st.join("SAVEGAM4.CK4"), b"later").unwrap();

        restore(&paths, &db, b.id, None, None).unwrap();
        assert_eq!(
            fs::read(st.join("SAVEGAM0.CK4")).unwrap(),
            keen4_save("old")
        );
        // Files the backup doesn't have stay.
        assert!(st.join("SAVEGAM4.CK4").exists());
        // What was there got backed up first.
        assert_eq!(
            list(&db, "keen4-1").unwrap()[0].reason,
            Some(Reason::BeforeRestore)
        );

        // Into another slot, removing a deletion marker that would hide it.
        fs::write(st.join("DBOVERLAY_DEL_SAVEGAM2.CK4"), b"empty").unwrap();
        restore(
            &paths,
            &db,
            b.id,
            Some("SAVEGAM0.CK4"),
            Some("SAVEGAM2.CK4"),
        )
        .unwrap();
        assert_eq!(
            fs::read(st.join("SAVEGAM2.CK4")).unwrap(),
            keen4_save("old")
        );
        assert!(!st.join("DBOVERLAY_DEL_SAVEGAM2.CK4").exists());

        assert!(restore(&paths, &db, b.id, Some("SAVEGAM0.CK4"), Some("EVIL.EXE")).is_err());
        assert!(restore(&paths, &db, b.id, Some("nope"), None).is_err());
        assert!(restore(&paths, &db, 999, None, None).is_err());
    }

    #[test]
    fn restore_replaces_differently_cased_file() {
        let (_tmp, paths, db) = setup();
        let st = state(&paths);
        let b = create(
            &paths,
            &db,
            "keen4-1",
            Contents::Files(&["config.ck4".into()]),
            Origin::User(None),
        )
        .unwrap()
        .unwrap();
        fs::remove_file(st.join("config.ck4")).unwrap();
        fs::write(st.join("CONFIG.CK4"), b"other").unwrap();
        restore(&paths, &db, b.id, None, None).unwrap();
        let names: Vec<String> = fs::read_dir(&st)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.eq_ignore_ascii_case("config.ck4"))
            .collect();
        assert_eq!(names.len(), 1);
        assert_eq!(fs::read(st.join(&names[0])).unwrap(), b"cfg");
    }

    #[test]
    fn saves_in_a_backup_are_described() {
        let (_tmp, paths, db) = setup();
        fs::write(state(&paths).join("SAVEGAM0.CK4"), keen4_save("hello")).unwrap();
        let b = create(&paths, &db, "keen4-1", Contents::State, Origin::User(None))
            .unwrap()
            .unwrap();
        let saves = saves_in(&db, b.id).unwrap();
        assert_eq!(saves.len(), 1);
        assert_eq!(
            saves[0].details.as_ref().unwrap().name.as_deref(),
            Some("hello")
        );
        assert_eq!(saves[0].modified_at, Some(b.created_at));
    }

    #[test]
    fn export_and_import_round_trip() {
        let (tmp, paths, db) = setup();
        let b = create(
            &paths,
            &db,
            "keen4-1",
            Contents::State,
            Origin::User(Some("Mine")),
        )
        .unwrap()
        .unwrap();
        let out = tmp.path().join("export.zip");
        export(&db, b.id, &out).unwrap();
        let imported = import(&paths, &db, "keen4-1", &out).unwrap();
        assert_eq!(imported.label.as_deref(), Some("Mine"));
        assert_eq!(imported.files, b.files);
        delete(&db, b.id).unwrap();
        assert_eq!(list(&db, "keen4-1").unwrap().len(), 1);
    }

    #[test]
    fn import_picks_this_games_files_from_any_zip() {
        let (tmp, paths, db) = setup();
        let zip_path = tmp.path().join("forum-saves.zip");
        {
            let mut zip = zip::ZipWriter::new(File::create(&zip_path).unwrap());
            for (name, data) in [
                ("keen/SAVEGAM3.CK4", b"a".as_slice()),
                ("README.TXT", b"b"),
                ("SAVED1.CK1", b"c"),
                ("../SAVEGAM4.CK4", b"d"),
            ] {
                zip.start_file(name, SimpleFileOptions::default()).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        let b = import(&paths, &db, "keen4-1", &zip_path).unwrap();
        assert_eq!(b.files, ["SAVEGAM3.CK4"]);
        assert_eq!(b.label.as_deref(), Some("forum-saves"));

        // Nothing usable, a BloogBase backup of another game, not a ZIP.
        let empty = tmp.path().join("empty.zip");
        zip::ZipWriter::new(File::create(&empty).unwrap())
            .finish()
            .unwrap();
        assert!(matches!(
            import(&paths, &db, "keen4-1", &empty),
            Err(Error::NoSavesFound)
        ));
        let other = tmp.path().join("other.zip");
        write_zip(
            &other,
            &[("SAVED1.CK1".into(), vec![1])],
            &Manifest {
                app: "BloogBase".into(),
                game_id: "keen1".into(),
                profile_id: "keen1-1".into(),
                label: None,
                created_at: 0,
                files: vec!["SAVED1.CK1".into()],
            },
        )
        .unwrap();
        assert!(matches!(
            import(&paths, &db, "keen4-1", &other),
            Err(Error::OtherGame(g)) if g == "keen1"
        ));
        let text = tmp.path().join("a.txt");
        fs::write(&text, b"x").unwrap();
        assert!(matches!(
            import(&paths, &db, "keen4-1", &text),
            Err(Error::UnsupportedSource(_))
        ));
    }

    #[test]
    fn reasons_round_trip_through_labels() {
        for r in [
            Reason::BeforeLaunch,
            Reason::BeforeRestore,
            Reason::BeforeDelete,
            Reason::ModRemoved,
        ] {
            assert_eq!(
                Reason::from_label(&format!("{REASON_PREFIX}{}", r.key())),
                Some(r)
            );
        }
        assert_eq!(Reason::from_label("Mod removed"), Some(Reason::ModRemoved));
        assert_eq!(Reason::from_label("my label"), None);
    }
}

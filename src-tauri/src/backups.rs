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

use crate::catalog::Game;
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
    /// Kept after the session only if that overwrote a save (§11.1): then
    /// it becomes [`Reason::BeforeOverwrite`].
    BeforeLaunch,
    BeforeOverwrite,
    BeforeRestore,
    BeforeDelete,
    ModRemoved,
    GameRemoved,
    /// Before dropping the copies a mod's launcher left (`launcher_copies`).
    LauncherReset,
}

const REASON_PREFIX: &str = "bloogbase:";

impl Reason {
    fn key(self) -> &'static str {
        match self {
            Reason::BeforeLaunch => "before-launch",
            Reason::BeforeOverwrite => "before-overwrite",
            Reason::BeforeRestore => "before-restore",
            Reason::BeforeDelete => "before-delete",
            Reason::ModRemoved => "mod-removed",
            Reason::GameRemoved => "game-removed",
            Reason::LauncherReset => "launcher-reset",
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
            Reason::BeforeOverwrite,
            Reason::BeforeRestore,
            Reason::BeforeDelete,
            Reason::ModRemoved,
            Reason::GameRemoved,
            Reason::LauncherReset,
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
    /// Of `files`: the saves, the game's other files (settings, high
    /// scores), and anything else. Deletion markers count as none of them.
    pub saves: u32,
    pub settings: u32,
    pub other: u32,
    pub auto: bool,
    pub size: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    app: String,
    game_id: String,
    /// Missing in backups made before it was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version_id: Option<String>,
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
    hex(h.finalize().as_slice())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn file_hash(data: &[u8]) -> String {
    hex(Sha256::digest(data).as_slice())
}

/// Notes what a backup holds, file by file (§11.1).
fn record_files(conn: &rusqlite::Connection, id: u32, files: &[(String, Vec<u8>)]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT OR REPLACE INTO backup_files (backup_id, path, hash) VALUES (?1, ?2, ?3)",
    )?;
    for (path, data) in files {
        stmt.execute(params![id, path, file_hash(data)])?;
    }
    Ok(())
}

/// Fills in `backup_files` for backups made before it existed. An archive
/// that can't be read is skipped; it just won't offer earlier versions.
pub fn record_older(db: &Db) -> Result<()> {
    let todo: Vec<(u32, String)> = {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, archive_path FROM save_backups b
             WHERE NOT EXISTS (SELECT 1 FROM backup_files f WHERE f.backup_id = b.id)",
        )?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    for (id, archive) in todo {
        match read_zip(Path::new(&archive)) {
            Ok((files, _)) => record_files(&db.conn(), id, &files)?,
            Err(e) => tracing::warn!("could not read backup {id} at {archive}: {e}"),
        }
    }
    Ok(())
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
        // A backup before a launch goes again after the session unless a
        // save was lost, so it stands in only for another one of its kind.
        let before_launch = format!("{REASON_PREFIX}{}", Reason::BeforeLaunch.key());
        let skip_before_launch = label.as_deref() != Some(before_launch.as_str());
        let last: Option<Option<String>> = db
            .conn()
            .query_row(
                "SELECT content_hash FROM save_backups WHERE profile_id = ?1
                 AND NOT (?2 AND label IS ?3)
                 ORDER BY created_at DESC, id DESC LIMIT 1",
                params![profile_id, skip_before_launch, before_launch],
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
        version_id: profile.version_id.clone(),
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
        let id = conn.last_insert_rowid() as u32;
        record_files(&conn, id, &files)?;
        id
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
            saves: 0,
            settings: 0,
            other: 0,
            auto: r.get(5)?,
            size: fs::metadata(&archive).map_or(0, |m| m.len() as u32),
        },
        archive,
    ))
}

/// Fills in what kinds of files a backup holds.
fn tally(game: &Game, mut b: Backup) -> Backup {
    for file in &b.files {
        let name = file.rsplit('/').next().unwrap_or(file);
        if overlay_view::is_marker(name) {
            continue;
        }
        if saves::is_save(game, file) {
            b.saves += 1;
        } else if !file.contains('/') && game.is_state_file(name) {
            b.settings += 1;
        } else {
            b.other += 1;
        }
    }
    b
}

fn game_of_profile(db: &Db, profile_id: &str) -> Result<&'static Game> {
    saves::game_of(&profiles::get(db, profile_id)?.0.game_id)
}

fn get_with_archive(db: &Db, id: u32) -> Result<(Backup, String)> {
    db.conn()
        .query_row(&format!("{SELECT} WHERE id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("backup {id}")))
}

pub fn get(db: &Db, id: u32) -> Result<Backup> {
    let b = get_with_archive(db, id)?.0;
    Ok(tally(game_of_profile(db, &b.profile_id)?, b))
}

/// The backup made when this mod or game was removed before, offered when
/// it's added again (§9.5).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EarlierBackup {
    pub backup: Backup,
    /// The version the saves come from, when known.
    pub version_id: Option<String>,
}

/// Settings key for the backup already restored or declined for a profile.
fn offered_key(profile_id: &str) -> String {
    format!("backups.offered.{profile_id}")
}

/// For a profile never played: the newest backup made before it was added,
/// when its mod (same id) or its game (same game, any version) was removed,
/// unless the player already answered for this profile. A profile added
/// again later is asked again.
pub fn earlier(db: &Db, profile_id: &str) -> Result<Option<EarlierBackup>> {
    let (profile, _) = profiles::get(db, profile_id)?;
    if profile.session_count > 0 {
        return Ok(None);
    }
    let reason = match profile.mod_id {
        Some(_) => Reason::ModRemoved,
        None => Reason::GameRemoved,
    };
    let candidates: Vec<(Backup, String)> = {
        let conn = db.conn();
        // Backups of this profile, or of one that's gone.
        let mut stmt = conn.prepare(&format!(
            "{SELECT} b WHERE b.profile_id = ?1
                OR NOT EXISTS (SELECT 1 FROM profiles p WHERE p.id = b.profile_id)
             ORDER BY created_at DESC, id DESC"
        ))?;
        stmt.query_map([profile_id], from_row)?
            .collect::<rusqlite::Result<_>>()?
    };
    let answered = settings::get(db, &offered_key(profile_id))?;
    let game = saves::game_of(&profile.game_id)?;
    for (backup, archive) in candidates {
        // A mod is the same mod by its id; a copy there before isn't added again.
        if backup.reason != Some(reason)
            || (profile.mod_id.is_some() && backup.profile_id != profile_id)
            || profile.added_at < backup.created_at
        {
            continue;
        }
        let manifest = match read_zip(Path::new(&archive)) {
            Ok((_, Some(m))) => m,
            Ok(_) => continue,
            Err(e) => {
                tracing::warn!("could not read backup {} at {archive}: {e}", backup.id);
                continue;
            }
        };
        if manifest.game_id != profile.game_id {
            continue;
        }
        if answered == Some(format!("{}:{}", profile.added_at, backup.id)) {
            return Ok(None);
        }
        return Ok(Some(EarlierBackup {
            backup: tally(game, backup),
            version_id: manifest.version_id,
        }));
    }
    Ok(None)
}

/// Answers the offer of [`earlier`]: with `restore`, the backup and the
/// other backups of its removed profile move to this one, and everything in
/// it is put back. Either way it isn't offered for this profile again.
pub fn answer_earlier(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    backup_id: u32,
    restore: bool,
) -> Result<()> {
    let (profile, _) = profiles::get(db, profile_id)?;
    if restore {
        let (backup, _) = get_with_archive(db, backup_id)?;
        if backup.profile_id != profile_id {
            db.conn().execute(
                "UPDATE save_backups SET profile_id = ?1 WHERE profile_id = ?2",
                [profile_id, backup.profile_id.as_str()],
            )?;
        }
        self::restore(paths, db, backup_id, None, None, true)?;
    }
    settings::set(
        db,
        &offered_key(profile_id),
        &format!("{}:{backup_id}", profile.added_at),
    )
}

/// A profile's backups, newest first.
pub fn list(db: &Db, profile_id: &str) -> Result<Vec<Backup>> {
    let game = game_of_profile(db, profile_id)?;
    let conn = db.conn();
    let mut stmt = conn.prepare(&format!(
        "{SELECT} WHERE profile_id = ?1 ORDER BY created_at DESC, id DESC"
    ))?;
    let rows = stmt
        .query_map([profile_id], from_row)?
        .map(|r| r.map(|(b, _)| tally(game, b)))
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

/// Puts a backup's saves back into `state/`, after backing up what's there;
/// with `include_other`, all its files: settings, high scores and deletion
/// markers too. With `file`, only that file, written under the name
/// `target` (another slot) if given. Files not in the backup stay as they are.
pub fn restore(
    paths: &AppPaths,
    db: &Db,
    id: u32,
    file: Option<&str>,
    target: Option<&str>,
    include_other: bool,
) -> Result<()> {
    let (backup, archive) = get_with_archive(db, id)?;
    let (profile, layers) = profiles::get(db, &backup.profile_id)?;
    let game = saves::game_of(&profile.game_id)?;
    let (files, _) = read_zip(Path::new(&archive))?;
    let chosen: Vec<(String, &[u8])> = match file {
        None => files
            .iter()
            .filter(|(n, _)| include_other || saves::is_save(game, n))
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
        version_id: profile.version_id.clone(),
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
        let id = conn.last_insert_rowid() as u32;
        record_files(&conn, id, &files)?;
        id
    };
    get(db, id)
}

/// The automatic backup before a launch (§6.2 step 3), if switched on; its
/// id, for [`after_session`]. Failing to back up must not keep anyone from
/// playing: it's logged only.
pub fn before_launch(paths: &AppPaths, db: &Db, profile_id: &str) -> Option<u32> {
    let enabled = get_settings(db).map(|s| s.auto).unwrap_or(true);
    if !enabled {
        return None;
    }
    match create(paths, db, profile_id, Contents::State, Reason::BeforeLaunch) {
        Ok(made) => made.map(|b| b.id),
        Err(e) => {
            tracing::warn!("backup before launching {profile_id} failed: {e}");
            None
        }
    }
}

/// The hash of each save DOS sees now, by upper-cased file name.
fn current_saves(game: &Game, layers: &Layers) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for entry in overlay_view::list(layers.view_lower(), &layers.upper)? {
        if saves::is_save(game, &entry.path) {
            let data = fs::read(saves::host_path(layers, &entry))?;
            out.push((entry.path.to_ascii_uppercase(), file_hash(&data)));
        }
    }
    Ok(out)
}

/// After a session: keeps the backup made before it only if the session
/// overwrote or lost a save that existed then (§11.1), and then names it
/// for that. Returns whether it was kept.
pub fn after_session(db: &Db, profile_id: &str, backup_id: u32) -> Result<bool> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let game = saves::game_of(&profile.game_id)?;
    let now = current_saves(game, &layers)?;
    let before: Vec<(String, String)> = {
        let conn = db.conn();
        let mut stmt = conn.prepare("SELECT path, hash FROM backup_files WHERE backup_id = ?1")?;
        stmt.query_map([backup_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let changed = before
        .iter()
        .filter(|(path, _)| saves::is_save(game, path))
        .any(|(path, hash)| {
            let upper = path.to_ascii_uppercase();
            !now.iter().any(|(p, h)| *p == upper && h == hash)
        });
    if changed {
        db.conn().execute(
            "UPDATE save_backups SET label = ?1 WHERE id = ?2",
            params![
                format!("{REASON_PREFIX}{}", Reason::BeforeOverwrite.key()),
                backup_id
            ],
        )?;
    } else {
        tracing::debug!("{profile_id} lost no save; dropping backup {backup_id}");
        delete(db, backup_id)?;
    }
    Ok(changed)
}

/// An earlier version of a save, kept in a backup.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveVersion {
    pub backup_id: u32,
    /// `modified_at` is when the backup was made.
    pub save: SaveFile,
}

/// The backed-up `(backup id, path, hash, created_at)` rows for a save file,
/// newest first.
fn backed_up(db: &Db, profile_id: &str, file: &str) -> Result<Vec<(u32, String, String, u32)>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT b.id, f.path, f.hash, b.created_at
         FROM backup_files f JOIN save_backups b ON b.id = f.backup_id
         WHERE b.profile_id = ?1 AND f.path = ?2 COLLATE NOCASE
         ORDER BY b.created_at DESC, b.id DESC",
    )?;
    let rows = stmt
        .query_map(params![profile_id, file], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Each content a save had in a backup other than `current` (its hash now),
/// once, newest first.
fn distinct_versions(
    rows: Vec<(u32, String, String, u32)>,
    current: Option<&str>,
) -> Vec<(u32, String, u32)> {
    let mut seen: Vec<String> = current.map(String::from).into_iter().collect();
    let mut out = Vec::new();
    for (id, path, hash, at) in rows {
        if !seen.contains(&hash) {
            seen.push(hash);
            out.push((id, path, at));
        }
    }
    out
}

/// How many earlier versions of a save with content `data` the backups hold.
pub fn version_count(db: &Db, profile_id: &str, file: &str, data: &[u8]) -> Result<u32> {
    let rows = backed_up(db, profile_id, file)?;
    Ok(distinct_versions(rows, Some(&file_hash(data))).len() as u32)
}

/// The earlier versions of a save that backups hold, newest first.
pub fn save_versions(db: &Db, profile_id: &str, file: &str) -> Result<Vec<SaveVersion>> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let game = saves::game_of(&profile.game_id)?;
    let upper = file.to_ascii_uppercase();
    let current = current_saves(game, &layers)?
        .into_iter()
        .find(|(p, _)| *p == upper)
        .map(|(_, h)| h);
    let mut out = Vec::new();
    for (id, path, at) in distinct_versions(backed_up(db, profile_id, file)?, current.as_deref()) {
        let (_, archive) = get_with_archive(db, id)?;
        let (files, _) = read_zip(Path::new(&archive))?;
        if let Some((_, data)) = files.iter().find(|(n, _)| *n == path) {
            out.push(SaveVersion {
                backup_id: id,
                save: saves::describe(game, &path, data, Some(at)),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library;
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

        restore(&paths, &db, b.id, None, None, false).unwrap();
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
            false,
        )
        .unwrap();
        assert_eq!(
            fs::read(st.join("SAVEGAM2.CK4")).unwrap(),
            keen4_save("old")
        );
        assert!(!st.join("DBOVERLAY_DEL_SAVEGAM2.CK4").exists());

        assert!(
            restore(
                &paths,
                &db,
                b.id,
                Some("SAVEGAM0.CK4"),
                Some("EVIL.EXE"),
                false
            )
            .is_err()
        );
        assert!(restore(&paths, &db, b.id, Some("nope"), None, false).is_err());
        assert!(restore(&paths, &db, 999, None, None, false).is_err());
    }

    #[test]
    fn restore_all_leaves_settings_unless_asked() {
        let (_tmp, paths, db) = setup();
        let st = state(&paths);
        fs::write(st.join("SAVEGAM0.CK4"), keen4_save("old")).unwrap();
        fs::write(st.join("DBOVERLAY_DEL_SAVEGAM3.CK4"), b"empty").unwrap();
        let b = create(&paths, &db, "keen4-1", Contents::State, Origin::User(None))
            .unwrap()
            .unwrap();
        assert_eq!((b.saves, b.settings, b.other), (1, 1, 0));
        fs::write(st.join("SAVEGAM0.CK4"), keen4_save("new")).unwrap();
        fs::write(st.join("config.ck4"), b"changed").unwrap();
        fs::remove_file(st.join("DBOVERLAY_DEL_SAVEGAM3.CK4")).unwrap();

        restore(&paths, &db, b.id, None, None, false).unwrap();
        assert_eq!(
            fs::read(st.join("SAVEGAM0.CK4")).unwrap(),
            keen4_save("old")
        );
        assert_eq!(fs::read(st.join("config.ck4")).unwrap(), b"changed");
        assert!(!st.join("DBOVERLAY_DEL_SAVEGAM3.CK4").exists());

        restore(&paths, &db, b.id, None, None, true).unwrap();
        assert_eq!(fs::read(st.join("config.ck4")).unwrap(), b"cfg");
        assert!(st.join("DBOVERLAY_DEL_SAVEGAM3.CK4").exists());
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
        restore(&paths, &db, b.id, None, None, true).unwrap();
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
    fn backup_before_launch_stays_only_if_a_save_was_lost() {
        let (_tmp, paths, db) = setup();
        let st = state(&paths);
        fs::write(st.join("SAVEGAM0.CK4"), keen4_save("first")).unwrap();

        // New saves, settings and scores alone don't keep it.
        let id = before_launch(&paths, &db, "keen4-1").unwrap();
        fs::write(st.join("SAVEGAM1.CK4"), keen4_save("new")).unwrap();
        fs::write(st.join("config.ck4"), b"changed").unwrap();
        assert!(!after_session(&db, "keen4-1", id).unwrap());
        assert!(list(&db, "keen4-1").unwrap().is_empty());
        assert!(
            paths
                .backups
                .join("keen4-1")
                .read_dir()
                .unwrap()
                .next()
                .is_none()
        );

        // Overwriting one does, and so does losing one.
        let id = before_launch(&paths, &db, "keen4-1").unwrap();
        fs::write(st.join("SAVEGAM0.CK4"), keen4_save("over")).unwrap();
        assert!(after_session(&db, "keen4-1", id).unwrap());
        let id = before_launch(&paths, &db, "keen4-1").unwrap();
        fs::remove_file(st.join("SAVEGAM1.CK4")).unwrap();
        assert!(after_session(&db, "keen4-1", id).unwrap());
        assert_eq!(
            names(&list(&db, "keen4-1").unwrap()),
            [Some(Reason::BeforeOverwrite); 2]
        );
    }

    #[test]
    fn earlier_versions_of_a_save_once_each_newest_first() {
        let (_tmp, paths, db) = setup();
        let st = state(&paths);
        let user = || {
            create(&paths, &db, "keen4-1", Contents::State, Origin::User(None))
                .unwrap()
                .unwrap()
        };
        fs::write(st.join("SAVEGAM0.CK4"), keen4_save("one")).unwrap();
        let first = user();
        // The same content again counts once, under its newest backup.
        fs::write(st.join("config.ck4"), b"changed").unwrap();
        let again = user();
        fs::write(st.join("SAVEGAM0.CK4"), keen4_save("two")).unwrap();
        user();
        fs::write(st.join("SAVEGAM0.CK4"), keen4_save("three")).unwrap();

        let versions = save_versions(&db, "keen4-1", "savegam0.ck4").unwrap();
        let names: Vec<_> = versions
            .iter()
            .map(|v| v.save.details.as_ref().unwrap().name.clone().unwrap())
            .collect();
        assert_eq!(names, ["two", "one"]);
        assert_eq!(versions[1].backup_id, again.id);
        assert_ne!(versions[1].backup_id, first.id);
        let saves = saves::list(&db, "keen4-1").unwrap();
        assert_eq!(saves[0].versions, 2);

        // Back to "two": that one isn't earlier any more.
        fs::write(st.join("SAVEGAM0.CK4"), keen4_save("two")).unwrap();
        assert_eq!(
            save_versions(&db, "keen4-1", "SAVEGAM0.CK4").unwrap().len(),
            1
        );
        assert_eq!(saves::list(&db, "keen4-1").unwrap()[0].versions, 1);
    }

    #[test]
    fn files_of_older_backups_are_noted_on_start() {
        let (_tmp, paths, db) = setup();
        fs::write(state(&paths).join("SAVEGAM0.CK4"), keen4_save("old")).unwrap();
        let b = create(&paths, &db, "keen4-1", Contents::State, Origin::User(None))
            .unwrap()
            .unwrap();
        fs::write(state(&paths).join("SAVEGAM0.CK4"), keen4_save("new")).unwrap();
        db.conn().execute("DELETE FROM backup_files", []).unwrap();
        assert!(
            save_versions(&db, "keen4-1", "SAVEGAM0.CK4")
                .unwrap()
                .is_empty()
        );

        record_older(&db).unwrap();
        let versions = save_versions(&db, "keen4-1", "SAVEGAM0.CK4").unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].backup_id, b.id);
        // Deleting a backup takes its rows along.
        delete(&db, b.id).unwrap();
        let left: u32 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM backup_files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
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
                version_id: None,
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
            Reason::BeforeOverwrite,
            Reason::BeforeRestore,
            Reason::BeforeDelete,
            Reason::ModRemoved,
            Reason::GameRemoved,
        ] {
            assert_eq!(
                Reason::from_label(&format!("{REASON_PREFIX}{}", r.key())),
                Some(r)
            );
        }
        assert_eq!(Reason::from_label("Mod removed"), Some(Reason::ModRemoved));
        assert_eq!(Reason::from_label("my label"), None);
    }

    #[test]
    fn offers_the_removal_backup_to_a_game_added_again() {
        let (tmp, paths, db) = setup();
        let version = profiles::get(&db, "keen4-1").unwrap().0.version_id;
        fs::write(state(&paths).join("SAVEGAM1.CK4"), keen4_save("old")).unwrap();
        library::remove(&paths, &db, "keen4-1", true, false).unwrap();
        // Left behind, so the copy added again gets another id.
        fs::create_dir_all(state(&paths)).unwrap();
        library::import(&paths, &db, &tmp.path().join("k4")).unwrap();

        let offer = earlier(&db, "keen4-2").unwrap().unwrap();
        assert_eq!(offer.backup.reason, Some(Reason::GameRemoved));
        assert_eq!(offer.backup.profile_id, "keen4-1");
        assert_eq!(offer.backup.saves, 2);
        assert_eq!(offer.version_id, version);

        answer_earlier(&paths, &db, "keen4-2", offer.backup.id, true).unwrap();
        let restored = paths.profiles.join("keen4-2/state/SAVEGAM1.CK4");
        assert_eq!(fs::read(restored).unwrap(), keen4_save("old"));
        // Its backups came along; asked once per adding.
        assert!(
            list(&db, "keen4-2")
                .unwrap()
                .iter()
                .any(|b| b.id == offer.backup.id)
        );
        assert!(earlier(&db, "keen4-2").unwrap().is_none());
    }

    #[test]
    fn a_declined_backup_is_offered_again_only_after_adding_again() {
        let (tmp, paths, db) = setup();
        library::remove(&paths, &db, "keen4-1", true, false).unwrap();
        library::import(&paths, &db, &tmp.path().join("k4")).unwrap();
        let offer = earlier(&db, "keen4-1").unwrap().unwrap();
        answer_earlier(&paths, &db, "keen4-1", offer.backup.id, false).unwrap();
        assert!(earlier(&db, "keen4-1").unwrap().is_none());
        assert!(!paths.profiles.join("keen4-1/state/SAVEGAM1.CK4").exists());

        // Added again later, without keeping the saves this time.
        library::remove(&paths, &db, "keen4-1", false, false).unwrap();
        library::import(&paths, &db, &tmp.path().join("k4")).unwrap();
        db.conn()
            .execute("UPDATE profiles SET created_at = created_at + 1", [])
            .unwrap();
        assert_eq!(
            earlier(&db, "keen4-1").unwrap().map(|o| o.backup.id),
            Some(offer.backup.id)
        );
    }

    #[test]
    fn played_or_older_profiles_are_not_offered_anything() {
        let (tmp, paths, db) = setup();
        library::remove(&paths, &db, "keen4-1", true, false).unwrap();
        library::import(&paths, &db, &tmp.path().join("k4")).unwrap();
        assert!(earlier(&db, "keen4-1").unwrap().is_some());
        db.conn()
            .execute(
                "INSERT INTO play_sessions (profile_id, started_at, ended_at) VALUES ('keen4-1', 1, 2)",
                [],
            )
            .unwrap();
        assert!(earlier(&db, "keen4-1").unwrap().is_none());
        // A copy that was there before the other one went.
        db.conn().execute("DELETE FROM play_sessions", []).unwrap();
        db.conn()
            .execute("UPDATE profiles SET created_at = created_at - 10", [])
            .unwrap();
        assert!(earlier(&db, "keen4-1").unwrap().is_none());
    }
}

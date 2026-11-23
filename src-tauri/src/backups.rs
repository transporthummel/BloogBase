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
    /// The saves made before a mod had worlds, taken out of `state/` when
    /// its first world begins (§9.6).
    BeforeWorlds,
    /// Before a world was deleted.
    WorldDeleted,
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
            Reason::BeforeWorlds => "before-worlds",
            Reason::WorldDeleted => "world-deleted",
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
            Reason::BeforeWorlds,
            Reason::WorldDeleted,
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
    /// The world it was made of, for mods with worlds (§9.6).
    pub world: Option<u32>,
    /// The variant it was made of, for mods with variants (§9.7).
    pub variant: Option<String>,
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
    /// The world it was made of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    world: Option<u32>,
    /// In the backup made when a mod is removed: all its worlds, so they
    /// come back with it (§9.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    worlds: Option<crate::worlds::KeptWorlds>,
    /// The variant it was made of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    variant: Option<String>,
    /// In the backup made when a mod is removed: the variant played, with
    /// the saves of the others under `bloogbase-variants/` (§9.7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    variants: Option<crate::variants::KeptVariants>,
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
        stmt.query_map([], |r| Ok((r.get(0)?, archive_at(db, r.get(1)?))))?
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
    let (_, layers) = profiles::get(db, profile_id)?;
    let mut files = collect(&layers, contents)?;
    let world = crate::worlds::active_world(db, profile_id)?;
    let variant = crate::variants::active_variant(db, profile_id)?;
    let origin = origin.into();
    let mut kept = None;
    let mut kept_variants = None;
    if matches!(origin, Origin::Auto(Reason::ModRemoved)) {
        if let Some((worlds, parked)) = crate::worlds::for_removal(paths, db, profile_id)? {
            files.extend(parked);
            kept = Some(worlds);
        }
        if let Some((variants, parked)) = crate::variants::for_removal(paths, db, profile_id)? {
            files.extend(parked);
            kept_variants = Some(variants);
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
    }
    let of = Of {
        world,
        worlds: kept,
        variant,
        variants: kept_variants,
    };
    store(paths, db, profile_id, files, origin, of)
}

/// What a backup was made of, besides the profile: its world or variant,
/// and when its mod is removed, all of them.
#[derive(Default)]
struct Of {
    world: Option<u32>,
    worlds: Option<crate::worlds::KeptWorlds>,
    variant: Option<String>,
    variants: Option<crate::variants::KeptVariants>,
}

/// Archives the files and saves of a world that isn't played: they aren't
/// in `state/`, but in its folder.
pub fn create_of_world(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    mut files: Files,
    reason: Reason,
    world: u32,
) -> Result<Option<Backup>> {
    files.sort_by(|a, b| a.0.cmp(&b.0));
    store(
        paths,
        db,
        profile_id,
        files,
        Origin::Auto(reason),
        Of {
            world: Some(world),
            ..Of::default()
        },
    )
}

/// Archives `files` as a backup of a profile, as [`create`] describes.
fn store(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    files: Files,
    origin: Origin<'_>,
    of: Of,
) -> Result<Option<Backup>> {
    let (profile, _) = profiles::get(db, profile_id)?;
    if files.is_empty() {
        return Ok(None);
    }
    let (label, reason) = match origin {
        Origin::User(label) => (
            label
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from),
            None,
        ),
        Origin::Auto(reason) => (
            Some(format!("{REASON_PREFIX}{}", reason.key())),
            Some(reason),
        ),
    };
    // Saves from before worlds are only in that backup: it's kept like the
    // player's own, never pruned (§9.6).
    let auto = reason.is_some() && reason != Some(Reason::BeforeWorlds);
    let hash = content_hash(&files);
    // The backup made when a profile is removed is the one that outlives it
    // (`forget`) and is offered when it comes back, so it's always made.
    let removal = matches!(reason, Some(Reason::ModRemoved | Reason::GameRemoved));
    if auto && !removal {
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
        world: of.world,
        worlds: of.worlds,
        variant: of.variant.clone(),
        variants: of.variants,
    };
    let archive = new_archive_path(paths, profile_id, created_at)?;
    write_zip(&archive, &files, &manifest)?;

    let id = {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO save_backups
                 (profile_id, label, archive_path, created_at, files_json, auto, content_hash,
                  world_id, variant)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                profile_id,
                label,
                db.store(&archive),
                created_at,
                serde_json::to_string(&names).expect("strings serialize"),
                auto,
                hash,
                of.world,
                of.variant
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
        stmt.query_map(params![profile_id, keep], |r| {
            Ok((r.get(0)?, archive_at(db, r.get(1)?)))
        })?
        .collect::<rusqlite::Result<_>>()?
    };
    for (id, archive) in old {
        remove(db, id, &archive)?;
    }
    Ok(())
}

/// For a profile being removed: without `keep_saves` all its backups go.
/// With it, the automatic ones go, but not the player's own or those made
/// for this or an earlier removal (one declined when the profile came back
/// has nothing newer); they come back with the offer of [`earlier`].
pub fn forget(db: &Db, profile_id: &str, keep_saves: bool) -> Result<()> {
    let [mod_removed, game_removed] = removal_labels();
    let gone = remove_where(
        db,
        "profile_id = ?1 AND NOT (?2 AND (auto = 0 OR label IN (?3, ?4, 'Mod removed')))",
        params![profile_id, keep_saves, mod_removed, game_removed],
    )?;
    if gone > 0 {
        tracing::info!("removed {gone} backups of {profile_id}");
    }
    Ok(())
}

/// The automatic backups of profiles removed before [`forget`] existed, as
/// it would have handled them with "keep saves".
pub fn forget_orphans(db: &Db) -> Result<()> {
    let [mod_removed, game_removed] = removal_labels();
    let gone = remove_where(
        db,
        "auto = 1 AND label NOT IN (?1, ?2, 'Mod removed')
         AND NOT EXISTS (SELECT 1 FROM profiles p WHERE p.id = b.profile_id)",
        params![mod_removed, game_removed],
    )?;
    if gone > 0 {
        tracing::info!("removed {gone} backups of removed profiles");
    }
    Ok(())
}

fn removal_labels() -> [String; 2] {
    [Reason::ModRemoved, Reason::GameRemoved].map(|r| format!("{REASON_PREFIX}{}", r.key()))
}

/// Removes the backups matching `condition`. Returns how many.
fn remove_where(db: &Db, condition: &str, params: impl rusqlite::Params) -> Result<usize> {
    let gone: Vec<(u32, String)> = {
        let conn = db.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT id, archive_path FROM save_backups b WHERE {condition}"
        ))?;
        stmt.query_map(params, |r| Ok((r.get(0)?, archive_at(db, r.get(1)?))))?
            .collect::<rusqlite::Result<_>>()?
    };
    for (id, archive) in &gone {
        remove(db, *id, archive)?;
    }
    Ok(gone.len())
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

const SELECT: &str = "SELECT id, profile_id, label, created_at, files_json, auto, archive_path,
                             world_id, variant
                      FROM save_backups";

/// Where a backup's archive is, from its stored path.
fn archive_at(db: &Db, stored: String) -> String {
    db.resolve(&stored).display().to_string()
}

fn from_row(db: &Db, r: &rusqlite::Row<'_>) -> rusqlite::Result<(Backup, String)> {
    let label: Option<String> = r.get(2)?;
    let files: String = r.get(4)?;
    let archive = archive_at(db, r.get(6)?);
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
            world: r.get(7)?,
            variant: r.get(8)?,
        },
        archive,
    ))
}

/// The game a profile's backed-up files are counted by: a mod's variants
/// may name their saves each their own way (Rescue of the Last's `.ROL`
/// and `.RES`).
struct Games<'a> {
    db: &'a Db,
    profile_id: &'a str,
    played: &'static Game,
    active: Option<String>,
    others: std::cell::RefCell<std::collections::HashMap<String, &'static Game>>,
}

impl<'a> Games<'a> {
    fn of_profile(db: &'a Db, profile_id: &'a str) -> Result<Self> {
        Ok(Games {
            db,
            profile_id,
            played: game_of_profile(db, profile_id)?,
            active: crate::variants::active_variant(db, profile_id)?,
            others: Default::default(),
        })
    }

    /// The game as `variant` plays it; one the mod no longer has counts
    /// as the variant played.
    fn of(&self, variant: Option<&str>) -> &'static Game {
        let Some(id) = variant.filter(|v| self.active.as_deref() != Some(*v)) else {
            return self.played;
        };
        self.others
            .borrow_mut()
            .entry(id.to_string())
            .or_insert_with(|| {
                crate::variants::variant_game(self.db, self.profile_id, id).unwrap_or(self.played)
            })
    }
}

/// Fills in what kinds of files a backup holds. The saves of the worlds
/// and variants not played, kept when a mod is removed, count too.
fn tally(games: &Games<'_>, mut b: Backup) -> Backup {
    let worlds = format!("{}/", crate::worlds::ARCHIVE_DIR);
    let variants = format!("{}/", crate::variants::ARCHIVE_DIR);
    for file in &b.files {
        let name = file.rsplit('/').next().unwrap_or(file);
        if overlay_view::is_marker(name) {
            continue;
        }
        let (game, file) = if let Some(rest) = file.strip_prefix(&variants) {
            let (id, rest) = rest.split_once('/').unwrap_or(("", rest));
            (games.of(Some(id)), rest)
        } else if let Some(rest) = file.strip_prefix(&worlds) {
            let rest = rest.split_once('/').map_or(rest, |(_, r)| r);
            (games.of(b.variant.as_deref()), rest)
        } else {
            (games.of(b.variant.as_deref()), file.as_str())
        };
        if saves::is_save(game, file) {
            b.saves += 1;
        } else if game.is_state_file(file) {
            b.settings += 1;
        } else {
            b.other += 1;
        }
    }
    b
}

fn game_of_profile(db: &Db, profile_id: &str) -> Result<&'static Game> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    saves::profile_game(&profile.game_id, &layers)
}

fn get_with_archive(db: &Db, id: u32) -> Result<(Backup, String)> {
    db.conn()
        .query_row(&format!("{SELECT} WHERE id = ?1"), [id], |r| {
            from_row(db, r)
        })
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("backup {id}")))
}

pub fn get(db: &Db, id: u32) -> Result<Backup> {
    let b = get_with_archive(db, id)?.0;
    let profile_id = b.profile_id.clone();
    Ok(tally(&Games::of_profile(db, &profile_id)?, b))
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
        stmt.query_map([profile_id], |r| from_row(db, r))?
            .collect::<rusqlite::Result<_>>()?
    };
    let answered = settings::get(db, &offered_key(profile_id))?;
    let games = Games::of_profile(db, profile_id)?;
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
            backup: tally(&games, backup),
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
    let games = Games::of_profile(db, profile_id)?;
    let rows: Vec<Backup> = {
        let conn = db.conn();
        let mut stmt = conn.prepare(&format!(
            "{SELECT} WHERE profile_id = ?1 ORDER BY created_at DESC, id DESC"
        ))?;
        stmt.query_map([profile_id], |r| from_row(db, r))?
            .map(|r| r.map(|(b, _)| b))
            .collect::<rusqlite::Result<_>>()?
    };
    Ok(rows.into_iter().map(|b| tally(&games, b)).collect())
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
    let (profile, layers) = profiles::get(db, &backup.profile_id)?;
    let game = saves::profile_game(&profile.game_id, &layers)?;
    let (files, manifest) = read_zip(Path::new(&archive))?;
    let files = as_kept_now(game, files);
    let at = manifest.map_or(backup.created_at, |m| m.created_at);
    Ok(files
        .iter()
        .filter(|(name, _)| saves::is_save(game, name))
        .map(|(name, data)| saves::describe(game, name, data, Some(at)))
        .collect())
}

/// A backup's files where the game keeps them now: a mod whose index entry
/// came to name its root (`oldRoot`) played from that folder before, so
/// saves and settings without a folder in its older backups go there.
fn as_kept_now(game: &Game, files: Files) -> Files {
    let Some(dir) = &game.old_root else {
        return files;
    };
    files
        .into_iter()
        .map(|(name, data)| {
            if !name.contains('/') && !overlay_view::is_marker(&name) && game.is_state_file(&name) {
                (format!("{dir}/{name}"), data)
            } else {
                (name, data)
            }
        })
        .collect()
}

/// Puts a backup's saves back into `state/`, after backing up what's there
/// if it holds files of the kind restored (saves, or the others); with `include_other`, all its files: settings, high scores and deletion
/// markers too. With `file`, only that file, written under the name
/// `target` (another slot) if given. Files not in the backup stay as they are.
/// Whether what was there was backed up.
pub fn restore(
    paths: &AppPaths,
    db: &Db,
    id: u32,
    file: Option<&str>,
    target: Option<&str>,
    include_other: bool,
) -> Result<bool> {
    let (backup, archive) = get_with_archive(db, id)?;
    let (mut files, manifest) = read_zip(Path::new(&archive))?;
    // The worlds and variants of a removed mod come back with all of the
    // backup.
    let whole = file.is_none() && include_other;
    let (worlds, variants) = manifest.map_or((None, None), |m| (m.worlds, m.variants));
    if let Some(kept) = worlds.filter(|_| whole) {
        crate::worlds::bring_back(paths, db, &backup.profile_id, &kept, &files)?;
    }
    if let Some(kept) = variants.filter(|_| whole) {
        crate::variants::bring_back(paths, db, &backup.profile_id, &kept, &files)?;
    }
    let parked = format!("{}/", crate::worlds::ARCHIVE_DIR);
    let parked_variants = format!("{}/", crate::variants::ARCHIVE_DIR);
    files.retain(|(name, _)| !name.starts_with(&parked) && !name.starts_with(&parked_variants));
    // A variant's saves are played in it: it's played again first, if the
    // mod still has it.
    if let Some(variant) = &backup.variant
        && crate::variants::active_variant(db, &backup.profile_id)?.as_ref() != Some(variant)
        && let Err(e) = crate::variants::switch(paths, db, &backup.profile_id, variant)
    {
        tracing::warn!("restoring backup {id} in the variant played: {e}");
    }
    let mut files = as_kept_now(game_of_profile(db, &backup.profile_id)?, files);
    // A world's saves fit only that world: it's played again first, if
    // it's still there. Another world's files never replace the ones of
    // the world played.
    if let Some(def) = crate::worlds::definition(db, &backup.profile_id)? {
        let mut active = crate::worlds::active_world(db, &backup.profile_id)?;
        if let Some(world) = backup.world
            && active != Some(world)
            && crate::worlds::activate(paths, db, &backup.profile_id, world).is_ok()
        {
            active = Some(world);
        }
        if backup.world.is_none() || backup.world != active {
            files.retain(|(name, _)| !def.holds(name));
        }
    }
    let backed_up = put_back(
        paths,
        db,
        &backup.profile_id,
        &files,
        file,
        target,
        include_other,
    )?;
    tracing::info!("restored backup {id} into {}", backup.profile_id);
    Ok(backed_up)
}

/// Whether a profile's `state/` holds saves, and other files (settings,
/// high scores and the like; deletion markers count as neither).
fn state_kinds(game: &Game, layers: &Layers) -> Result<(bool, bool)> {
    let (mut saves_found, mut other) = (false, false);
    for (name, _) in collect(layers, Contents::State)? {
        let file_name = name.rsplit('/').next().unwrap_or(&name);
        if saves::is_save(game, &name) {
            saves_found = true;
        } else if !overlay_view::is_marker(file_name) {
            other = true;
        }
    }
    Ok((saves_found, other))
}

/// Writes `files` into a profile's `state/`, after backing up what's there
/// if it holds files of the kind restored, as [`restore`] describes.
fn put_back(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    files: &Files,
    file: Option<&str>,
    target: Option<&str>,
    include_other: bool,
) -> Result<bool> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let game = saves::profile_game(&profile.game_id, &layers)?;
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
                .ok_or_else(|| Error::NotFound(file.to_string()))?;
            let name = target.unwrap_or(file);
            if !saves::is_save(game, name) && name != file {
                return Err(Error::NotFound(format!("save slot {name}")));
            }
            vec![(name.to_string(), data.as_slice())]
        }
    };

    let restored_saves = chosen.iter().any(|(n, _)| saves::is_save(game, n));
    let restored_other = chosen.iter().any(|(n, _)| !saves::is_save(game, n));
    // Backed up only if it holds files of the kind restored: restoring saves
    // where there are only settings and high scores, or the other way round,
    // replaces nothing a backup would bring back.
    let (has_saves, has_other) = state_kinds(game, &layers)?;
    let backed_up = (restored_saves && has_saves) || (restored_other && has_other);
    if backed_up {
        create(
            paths,
            db,
            profile_id,
            Contents::State,
            Reason::BeforeRestore,
        )?;
    }
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
    // Copies a mod's launcher left may be back: the next launch counts the
    // composed folder as changed and clears them (`launcher_copies`).
    if restored_other {
        db.conn().execute(
            "UPDATE profiles SET composed_hash = NULL WHERE id = ?1",
            [profile_id],
        )?;
    }
    Ok(backed_up)
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

/// The saves, settings and high scores of a profile's game lying in a
/// folder, such as an old copy of the game or a mod, to restore from there
/// without keeping them as a backup.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderSaves {
    pub saves: Vec<SaveFile>,
    /// Settings and high scores.
    pub settings: u32,
    /// Whether files lie there without a folder of the game's
    /// ([`Game::save_dirs`]) to go into: the player picks the part.
    pub loose: bool,
}

/// A file read from a folder, with its modification time.
type TimedFile = (String, Vec<u8>, Option<u32>);

/// The files directly in `dir` that look like `game`'s saves, config or
/// scores: an old copy keeps them next to its program. A game that keeps
/// them in folders ([`Game::save_dirs`]) finds them in those folders of a
/// whole copy; a folder named like one of them is that part; files in any
/// other go into `part`, if given.
fn read_folder(game: &Game, dir: &Path, part: Option<&str>) -> Result<Vec<TimedFile>> {
    let part = match part {
        Some(p) => Some(
            game.save_dirs
                .iter()
                .find(|d| d.eq_ignore_ascii_case(p))
                .ok_or_else(|| Error::NotFound(format!("save folder {p}")))?
                .as_str(),
        ),
        None => None,
    };
    let save_dir = |name: &str| {
        game.save_dirs
            .iter()
            .find(|d| d.eq_ignore_ascii_case(name))
            .map(String::as_str)
    };
    let mut files = Vec::new();
    let mut whole_copy = false;
    if !game.save_dirs.is_empty() {
        for entry in fs::read_dir(dir)?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().is_ok_and(|t| t.is_dir())
                && let Some(d) = save_dir(&name)
            {
                whole_copy = true;
                read_files(game, &entry.path(), Some(d), &mut files)?;
            }
        }
    }
    // The game doesn't look in a whole copy's root.
    if !whole_copy {
        let own = dir.file_name().and_then(|n| save_dir(&n.to_string_lossy()));
        read_files(game, dir, own.or(part), &mut files)?;
    }
    if files.is_empty() {
        return Err(Error::NoSavesFound);
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// Adds the state files lying directly in `dir`, into the game's folder
/// `into` if given.
fn read_files(
    game: &Game,
    dir: &Path,
    into: Option<&str>,
    files: &mut Vec<TimedFile>,
) -> Result<()> {
    for entry in fs::read_dir(dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().is_ok_and(|t| t.is_file()) && game.is_state_file(&name) {
            let path = entry.path();
            let rel = match into {
                Some(d) => format!("{d}/{name}"),
                None => name,
            };
            files.push((rel, fs::read(&path)?, saves::modified(&path)));
        }
    }
    Ok(())
}

/// What [`restore_folder`] can take from `dir`.
pub fn folder_saves(db: &Db, profile_id: &str, dir: &Path) -> Result<FolderSaves> {
    let game = game_of_profile(db, profile_id)?;
    let files = read_folder(game, dir, None)?;
    let mut saves: Vec<SaveFile> = files
        .iter()
        .filter(|(name, ..)| saves::is_save(game, name))
        .map(|(name, data, at)| saves::describe(game, name, data, *at))
        .collect();
    saves.sort_by(|a, b| (a.slot, &a.file).cmp(&(b.slot, &b.file)));
    let settings = (files.len() - saves.len()) as u32;
    let loose = !game.save_dirs.is_empty() && files.iter().any(|(n, ..)| !n.contains('/'));
    Ok(FolderSaves {
        saves,
        settings,
        loose,
    })
}

/// Puts saves from a folder into a profile as [`restore`] does from a
/// backup; only what the profile has now is backed up. Whether it was.
/// Files the folder doesn't place go into the game's folder `part`.
#[allow(clippy::too_many_arguments)]
pub fn restore_folder(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    dir: &Path,
    part: Option<&str>,
    file: Option<&str>,
    target: Option<&str>,
    include_other: bool,
) -> Result<bool> {
    let game = game_of_profile(db, profile_id)?;
    let files: Files = read_folder(game, dir, part)?
        .into_iter()
        .map(|(name, data, _)| (name, data))
        .collect();
    // In the root, the game wouldn't see them.
    if !game.save_dirs.is_empty() && files.iter().any(|(n, _)| !n.contains('/')) {
        return Err(Error::NotFound("save folder".into()));
    }
    let backed_up = put_back(paths, db, profile_id, &files, file, target, include_other)?;
    tracing::info!("restored saves from {} into {profile_id}", dir.display());
    Ok(backed_up)
}

/// Adds a ZIP as a new backup of a profile, to be restored from there.
/// BloogBase exports keep everything; other archives contribute the files
/// that look like this game's saves, config or scores, wherever they are.
pub fn import(paths: &AppPaths, db: &Db, profile_id: &str, src: &Path) -> Result<Backup> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let game = saves::profile_game(&profile.game_id, &layers)?;
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
                let mut parts = name.rsplit('/');
                let base = parts.next().unwrap_or(&name);
                // In a folder the game keeps saves in (a launcher's part),
                // a file stays there.
                let dir = parts
                    .next()
                    .and_then(|p| game.save_dirs.iter().find(|d| d.eq_ignore_ascii_case(p)));
                let rel = match dir {
                    Some(d) => format!("{d}/{base}"),
                    None => base.to_string(),
                };
                if game.is_state_file(&rel)
                    && !picked.iter().any(|(n, _)| n.eq_ignore_ascii_case(&rel))
                {
                    picked.push((rel, data));
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
        world: None,
        worlds: None,
        variant: None,
        variants: None,
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
                db.store(&archive),
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
    let game = saves::profile_game(&profile.game_id, &layers)?;
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
    let game = saves::profile_game(&profile.game_id, &layers)?;
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
    fn restoring_backs_up_only_files_of_the_kind_restored() {
        let (tmp, paths, db) = setup();
        let st = state(&paths);
        let before_restore = |db: &Db| {
            list(db, "keen4-1")
                .unwrap()
                .iter()
                .filter(|b| b.reason == Some(Reason::BeforeRestore))
                .count()
        };
        // The fixture's save goes; only settings are left (in lower case,
        // as the fixture names them).
        fs::remove_file(st.join("SAVEGAM0.CK4")).unwrap();
        fs::write(st.join("config.ck4"), b"mine").unwrap();
        let old = tmp.path().join("old");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("SAVEGAM1.CK4"), b"a").unwrap();
        fs::write(old.join("CONFIG.CK4"), b"old").unwrap();

        // Saves over settings only: nothing to keep.
        restore_folder(&paths, &db, "keen4-1", &old, None, None, None, false).unwrap();
        assert_eq!(before_restore(&db), 0);
        // Saves over saves, and settings over settings, are backed up.
        fs::write(st.join("SAVEGAM1.CK4"), b"newer").unwrap();
        restore_folder(&paths, &db, "keen4-1", &old, None, None, None, false).unwrap();
        assert_eq!(before_restore(&db), 1);
        fs::write(st.join("config.ck4"), b"changed").unwrap();
        restore_folder(
            &paths,
            &db,
            "keen4-1",
            &old,
            None,
            Some("CONFIG.CK4"),
            None,
            false,
        )
        .unwrap();
        assert_eq!(before_restore(&db), 2);

        // Settings over saves only: nothing to keep either.
        fs::remove_file(st.join("config.ck4")).unwrap();
        fs::write(st.join("SAVEGAM1.CK4"), b"newest").unwrap();
        restore_folder(
            &paths,
            &db,
            "keen4-1",
            &old,
            None,
            Some("CONFIG.CK4"),
            None,
            false,
        )
        .unwrap();
        assert_eq!(before_restore(&db), 2);
        assert_eq!(fs::read(st.join("CONFIG.CK4")).unwrap(), b"old");
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
                world: None,
                worlds: None,
                variant: None,
                variants: None,
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
    fn saves_in_a_folder_are_restored_without_a_backup_of_them() {
        let (tmp, paths, db) = setup();
        let old = tmp.path().join("KEEN4");
        for (name, data) in [
            ("KEEN4E.EXE", b"exe".as_slice()),
            ("savegam2.ck4", b"a"),
            ("CONFIG.CK4", b"old config"),
            ("sub/SAVEGAM5.CK4", b"deeper"),
        ] {
            let path = old.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, data).unwrap();
        }
        let found = folder_saves(&db, "keen4-1", &old).unwrap();
        let names: Vec<_> = found.saves.iter().map(|s| s.file.as_str()).collect();
        assert_eq!(names, ["savegam2.ck4"]);
        assert_eq!(found.saves[0].slot, Some(3));
        assert!(found.saves[0].modified_at.is_some());
        assert_eq!(found.settings, 1);

        let state = paths.profiles.join("keen4-1/state");
        fs::write(state.join("CONFIG.CK4"), b"mine").unwrap();
        restore_folder(&paths, &db, "keen4-1", &old, None, None, None, false).unwrap();
        assert_eq!(fs::read(state.join("savegam2.ck4")).unwrap(), b"a");
        assert_eq!(fs::read(state.join("CONFIG.CK4")).unwrap(), b"mine");
        restore_folder(
            &paths,
            &db,
            "keen4-1",
            &old,
            None,
            Some("savegam2.ck4"),
            Some("SAVEGAM7.CK4"),
            false,
        )
        .unwrap();
        assert_eq!(fs::read(state.join("SAVEGAM7.CK4")).unwrap(), b"a");
        // Only what the profile had is backed up, never the folder.
        let made = list(&db, "keen4-1").unwrap();
        assert!(!made.is_empty());
        assert!(made.iter().all(|b| b.reason == Some(Reason::BeforeRestore)));

        let empty = tmp.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(matches!(
            folder_saves(&db, "keen4-1", &empty),
            Err(Error::NoSavesFound)
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

        // Removed again, keeping saves: the newer backup is offered when
        // the game is back, and the declined one stays.
        library::remove(&paths, &db, "keen4-1", true, false).unwrap();
        library::import(&paths, &db, &tmp.path().join("k4")).unwrap();
        db.conn()
            .execute("UPDATE profiles SET created_at = created_at + 1", [])
            .unwrap();
        let again = earlier(&db, "keen4-1").unwrap().unwrap();
        assert_ne!(again.backup.id, offer.backup.id);
        assert!(get(&db, offer.backup.id).is_ok());

        // Removed without keeping the saves: nothing is left to offer.
        library::remove(&paths, &db, "keen4-1", false, false).unwrap();
        library::import(&paths, &db, &tmp.path().join("k4")).unwrap();
        db.conn()
            .execute("UPDATE profiles SET created_at = created_at + 2", [])
            .unwrap();
        assert!(earlier(&db, "keen4-1").unwrap().is_none());
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

    #[test]
    fn automatic_backups_of_removed_profiles_go_at_start() {
        let (_tmp, paths, db) = setup();
        fs::write(state(&paths).join("SAVEGAM1.CK4"), keen4_save("a")).unwrap();
        before_launch(&paths, &db, "keen4-1").unwrap();
        create(&paths, &db, "keen4-1", Contents::State, Origin::User(None)).unwrap();
        create(&paths, &db, "keen4-1", Contents::State, Reason::GameRemoved).unwrap();
        // Still there: nothing goes.
        forget_orphans(&db).unwrap();
        assert_eq!(list(&db, "keen4-1").unwrap().len(), 3);

        // Removed before `forget` existed.
        db.conn()
            .execute("DELETE FROM profiles WHERE id = 'keen4-1'", [])
            .unwrap();
        forget_orphans(&db).unwrap();
        let left: Vec<Option<String>> = db
            .conn()
            .prepare("SELECT label FROM save_backups ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(left, [None, Some("bloogbase:game-removed".to_string())]);
        assert_eq!(
            fs::read_dir(paths.backups.join("keen4-1")).unwrap().count(),
            2
        );
    }
}

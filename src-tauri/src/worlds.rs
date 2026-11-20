//! Worlds of mods that generate their levels at every start (PLAN.md §9.6):
//! the files a generator wrote, kept after it ran, with the saves made while
//! playing them. The index entry declares how (`worlds`).
//!
//! The active world's files and saves are in the profile's `state/`, where
//! the game sees them; the others wait in `worlds/<id>/` next to it. A
//! switch moves them file by file, with a journal (`worlds/swap.json`) so a
//! switch cut short is finished at the next one.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backups::{self, Contents, Reason};
use crate::catalog::glob_match;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::library::now;
use crate::paths::AppPaths;
use crate::recipe::Recipe;
use crate::saves;
use crate::settings;
use crate::slots::{self, ArchiveFiles, Owner, Slots, file_names};

/// How a mod keeps its worlds, from its index entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Worlds {
    /// Generates a new world and plays it; the seed and options are
    /// appended to its command.
    pub new: Recipe,
    /// Plays the world in `state/` without generating anew.
    pub play: Recipe,
    /// What a world is made of, as the generator writes it into `state/`:
    /// DOS names or patterns.
    pub files: Vec<String>,
    #[serde(default)]
    pub seed: Option<Seed>,
    #[serde(default)]
    pub options: Vec<WorldOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Seed {
    /// The argument before the number, e.g. `/SEED`.
    pub arg: String,
    /// The largest seed a player may enter.
    pub max: u32,
    /// Where the generator notes the seed it used.
    #[serde(default)]
    pub read: Option<SeedRead>,
}

/// The seed is the number right after `after` in `file`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SeedRead {
    pub file: String,
    pub after: String,
}

/// An option of the generator, offered when starting a new world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldOption {
    pub id: String,
    pub arg: String,
    #[serde(rename = "type")]
    pub kind: OptionKind,
    #[serde(default)]
    pub min: Option<u32>,
    #[serde(default)]
    pub max: Option<u32>,
    #[serde(default)]
    pub default: Option<u32>,
    /// Language code → label; `en` is always there.
    pub label: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum OptionKind {
    /// Passed as `arg` when on (1).
    Flag,
    /// Passed as `arg n`.
    Number,
}

impl Worlds {
    /// Whether `name` is one of a world's generated files.
    pub fn holds(&self, name: &str) -> bool {
        !name.contains('/') && self.files.iter().any(|p| glob_match(p, name))
    }

    /// Whether `names` hold a whole world: something for each pattern.
    fn complete(&self, names: &[String]) -> bool {
        self.files
            .iter()
            .all(|p| names.iter().any(|n| glob_match(p, n)))
    }

    /// The option values with the defaults filled in, or an error for one
    /// the mod doesn't have or out of its range.
    fn checked_options(&self, given: &BTreeMap<String, u32>) -> Result<BTreeMap<String, u32>> {
        if let Some(id) = given
            .keys()
            .find(|id| !self.options.iter().any(|o| &o.id == *id))
        {
            return Err(Error::InvalidSetting(format!("world option {id}")));
        }
        let mut out = BTreeMap::new();
        for o in &self.options {
            let value = given.get(&o.id).copied();
            let value = match o.kind {
                OptionKind::Flag => match value.unwrap_or(0) {
                    v @ (0 | 1) => v,
                    _ => return Err(Error::InvalidSetting(format!("world option {}", o.id))),
                },
                OptionKind::Number => {
                    let v = value.or(o.default).unwrap_or(0);
                    if o.min.is_some_and(|m| v < m) || o.max.is_some_and(|m| v > m) {
                        return Err(Error::InvalidSetting(format!("world option {}", o.id)));
                    }
                    v
                }
            };
            out.insert(o.id.clone(), value);
        }
        Ok(out)
    }

    /// The arguments for the generator.
    fn args(&self, seed: Option<u32>, options: &BTreeMap<String, u32>) -> Vec<String> {
        let mut out = Vec::new();
        if let (Some(s), Some(seed)) = (&self.seed, seed) {
            out.push(s.arg.clone());
            out.push(seed.to_string());
        }
        for o in &self.options {
            let value = options.get(&o.id).copied().unwrap_or(0);
            match o.kind {
                OptionKind::Flag if value != 0 => out.push(o.arg.clone()),
                OptionKind::Flag => {}
                OptionKind::Number => {
                    out.push(o.arg.clone());
                    out.push(value.to_string());
                }
            }
        }
        out
    }
}

/// A kept world.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct World {
    /// Counts up per profile; the app names a world without `name` after
    /// it or its seed.
    pub id: u32,
    pub name: Option<String>,
    pub seed: Option<u32>,
    /// Option id → value as generated (flags 0 or 1).
    pub options: BTreeMap<String, u32>,
    /// Its first start, Unix epoch seconds.
    pub created_at: u32,
    pub last_played_at: u32,
    pub favourite: bool,
    /// Its files and saves are the ones the game sees.
    pub active: bool,
    pub saves: u32,
}

/// What the Worlds tab and the "Start new world" dialog show.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileWorlds {
    pub definition: Worlds,
    /// Favourites first, then by last start.
    pub worlds: Vec<World>,
    pub active: Option<u32>,
    /// What the last new world was started with.
    pub last_options: BTreeMap<String, u32>,
}

/// How to start a profile with worlds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WorldStart {
    Continue {
        id: u32,
    },
    New {
        seed: Option<u32>,
        #[serde(default)]
        options: BTreeMap<String, u32>,
    },
}

/// A new world under way, recorded when its session ends.
#[derive(Debug, Clone, PartialEq)]
pub struct NewWorld {
    pub seed: Option<u32>,
    pub options: BTreeMap<String, u32>,
    pub started_at: i64,
}

/// What a session of a profile with worlds runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// The DOS command, in place of the recipe's.
    pub command: String,
    pub args: Vec<String>,
    /// Set when it generates a new world.
    pub new: Option<NewWorld>,
}

/// The mod's worlds, for a profile of a mod that has them.
pub fn definition(db: &Db, profile_id: &str) -> Result<Option<Worlds>> {
    let json: Option<String> = db
        .conn()
        .query_row(
            "SELECT m.worlds_json FROM profiles p JOIN mods m ON m.id = p.mod_id
             WHERE p.id = ?1",
            [profile_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(json.and_then(|j| match serde_json::from_str(&j) {
        Ok(w) => Some(w),
        Err(e) => {
            tracing::warn!("unreadable worlds of {profile_id}: {e}");
            None
        }
    }))
}

fn active(db: &Db, profile_id: &str) -> Result<Option<u32>> {
    Ok(db.conn().query_row(
        "SELECT active_world FROM profiles WHERE id = ?1",
        [profile_id],
        |r| r.get(0),
    )?)
}

fn set_active(db: &Db, profile_id: &str, world: Option<u32>) -> Result<()> {
    db.conn().execute(
        "UPDATE profiles SET active_world = ?2 WHERE id = ?1",
        params![profile_id, world],
    )?;
    Ok(())
}

/// Where the worlds not played wait.
pub fn dir(paths: &AppPaths, profile_id: &str) -> PathBuf {
    paths.profiles.join(profile_id).join("worlds")
}

fn world_dir(paths: &AppPaths, profile_id: &str, id: u32) -> PathBuf {
    dir(paths, profile_id).join(id.to_string())
}
/// What a profile's worlds are about: the game as it plays and `state/`.
struct Ctx<'a> {
    paths: &'a AppPaths,
    db: &'a Db,
    profile_id: &'a str,
    def: &'a Worlds,
    owner: Owner,
    dir: PathBuf,
}

impl<'a> Ctx<'a> {
    fn new(paths: &'a AppPaths, db: &'a Db, profile_id: &'a str, def: &'a Worlds) -> Result<Self> {
        Ok(Ctx {
            paths,
            db,
            profile_id,
            def,
            owner: Owner::of(db, profile_id)?,
            dir: dir(paths, profile_id),
        })
    }
}

impl Slots for Ctx<'_> {
    type Key = u32;

    fn dir(&self) -> &Path {
        &self.dir
    }

    fn owner(&self) -> &Owner {
        &self.owner
    }

    /// Whether a file in `state/` belongs to the world played.
    fn belongs(&self, name: &str) -> bool {
        self.def.holds(name) || saves::is_save(self.owner.game, name)
    }

    fn active(&self) -> Result<Option<u32>> {
        active(self.db, self.profile_id)
    }

    fn set_active(&self, key: Option<&u32>) -> Result<()> {
        set_active(self.db, self.profile_id, key.copied())
    }

    /// When no world is recorded as played: saves made before worlds (or in
    /// a world never recorded) go into a backup, and the rest away.
    fn park_none(&self, names: &[String]) -> Result<()> {
        let saves: Vec<String> = names
            .iter()
            .filter(|n| saves::is_save(self.owner.game, n))
            .cloned()
            .collect();
        if !saves.is_empty() {
            backups::create(
                self.paths,
                self.db,
                self.profile_id,
                Contents::Files(&saves),
                Reason::BeforeWorlds,
            )?;
            tracing::info!(
                "{} saves of {} from before worlds backed up",
                saves.len(),
                self.profile_id
            );
        }
        for name in names {
            fs::remove_file(self.owner.upper.join(name))?;
        }
        Ok(())
    }
}

fn exists(db: &Db, profile_id: &str, id: u32) -> Result<bool> {
    Ok(db.conn().query_row(
        "SELECT EXISTS (SELECT 1 FROM worlds WHERE profile_id = ?1 AND id = ?2)",
        params![profile_id, id],
        |r| r.get(0),
    )?)
}

fn last_options_key(profile_id: &str) -> String {
    format!("worlds.options.{profile_id}")
}

/// Makes a profile ready for a session with worlds and says what it runs;
/// `None` for profiles without worlds. Without `start`, the active world
/// goes on, or a new one begins with the options of the last.
pub fn prepare(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    start: Option<WorldStart>,
) -> Result<Option<Run>> {
    let Some(def) = definition(db, profile_id)? else {
        return Ok(None);
    };
    let ctx = Ctx::new(paths, db, profile_id, &def)?;
    ctx.recover()?;
    let start = match start {
        Some(s) => s,
        None => match active(db, profile_id)? {
            Some(id) => WorldStart::Continue { id },
            None => WorldStart::New {
                seed: None,
                options: last_options(db, profile_id)?,
            },
        },
    };
    // A seed and options a world was already made with make it again: that
    // world goes on instead of a copy of it.
    let start = match start {
        WorldStart::New {
            seed: Some(seed),
            options,
        } => match same_world(db, profile_id, &def, seed, &def.checked_options(&options)?)? {
            Some(id) => WorldStart::Continue { id },
            None => WorldStart::New {
                seed: Some(seed),
                options,
            },
        },
        s => s,
    };
    let command = |r: &Recipe| {
        r.command()
            .ok_or_else(|| Error::NotFound(format!("command of the worlds of {profile_id}")))
    };
    match start {
        WorldStart::Continue { id } => {
            if !exists(db, profile_id, id)? {
                return Err(Error::NotFound(format!("world {id}")));
            }
            ctx.switch(Some(id))?;
            if !def.complete(&file_names(&ctx.owner.upper)?) {
                return Err(Error::WorldIncomplete);
            }
            db.conn().execute(
                "UPDATE worlds SET last_played_at = ?3 WHERE profile_id = ?1 AND id = ?2",
                params![profile_id, id, now()],
            )?;
            Ok(Some(Run {
                command: command(&def.play)?,
                args: Vec::new(),
                new: None,
            }))
        }
        WorldStart::New { seed, options } => {
            if let Some(s) = seed
                && def.seed.as_ref().is_none_or(|d| s > d.max)
            {
                return Err(Error::InvalidSetting(format!("seed {s}")));
            }
            let options = def.checked_options(&options)?;
            match active(db, profile_id)? {
                Some(_) => ctx.switch(None)?,
                None => ctx.park(None)?,
            }
            settings::set(
                db,
                &last_options_key(profile_id),
                &serde_json::to_string(&options).expect("options serialize"),
            )?;
            Ok(Some(Run {
                command: command(&def.new)?,
                args: def.args(seed, &options),
                new: Some(NewWorld {
                    seed,
                    options,
                    started_at: now(),
                }),
            }))
        }
    }
}

/// The world made with `seed` and `options`, if there is one. Options a
/// world predates count with their defaults.
fn same_world(
    db: &Db,
    profile_id: &str,
    def: &Worlds,
    seed: u32,
    options: &BTreeMap<String, u32>,
) -> Result<Option<u32>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, options_json FROM worlds WHERE profile_id = ?1 AND seed = ?2 ORDER BY id",
    )?;
    let rows = stmt
        .query_map(params![profile_id, seed], |r| {
            Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().find_map(|(id, json)| {
        let mut kept: BTreeMap<String, u32> = serde_json::from_str(&json).ok()?;
        // Ones the mod no longer has don't tell worlds apart.
        kept.retain(|k, _| def.options.iter().any(|o| &o.id == k));
        (def.checked_options(&kept).ok()? == *options).then_some(id)
    }))
}

fn last_options(db: &Db, profile_id: &str) -> Result<BTreeMap<String, u32>> {
    Ok(settings::get(db, &last_options_key(profile_id))?
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default())
}

/// The number after `after` in `text`.
fn number_after(text: &str, after: &str) -> Option<u32> {
    let rest = &text[text.find(after)? + after.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// After a session that generated a new world: records it as the active
/// world if the generator wrote all of it. Its id, or `None` if not.
pub fn after_session(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    new: &NewWorld,
) -> Result<Option<u32>> {
    let Some(def) = definition(db, profile_id)? else {
        return Ok(None);
    };
    let ctx = Ctx::new(paths, db, profile_id, &def)?;
    let names = file_names(&ctx.owner.upper)?;
    if !def.complete(&names) {
        tracing::warn!("{profile_id}: the generator wrote no complete world");
        return Ok(None);
    }
    let read = def
        .seed
        .as_ref()
        .and_then(|s| s.read.as_ref())
        .and_then(|r| {
            let name = names.iter().find(|n| n.eq_ignore_ascii_case(&r.file))?;
            let text = fs::read(ctx.owner.upper.join(name)).ok()?;
            number_after(&String::from_utf8_lossy(&text), &r.after)
        });
    let seed = read.or(new.seed);
    let id = {
        let conn = db.conn();
        // Not the id of a deleted world that backups still name.
        let id: u32 = conn.query_row(
            "SELECT MAX(COALESCE((SELECT MAX(id) FROM worlds WHERE profile_id = ?1), 0),
                        COALESCE((SELECT MAX(world_id) FROM save_backups WHERE profile_id = ?1), 0))
                    + 1",
            [profile_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO worlds (profile_id, id, seed, options_json, created_at, last_played_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![
                profile_id,
                id,
                seed,
                serde_json::to_string(&new.options).expect("options serialize"),
                new.started_at
            ],
        )?;
        id
    };
    set_active(db, profile_id, Some(id))?;
    tracing::info!("{profile_id}: new world {id} with seed {seed:?}");
    Ok(Some(id))
}

/// A profile's worlds, or `None` if its mod has none. Only reads, so it
/// needs no hold on the profile: a switch cut short is finished by the next
/// launch or deletion, which have one.
pub fn list(paths: &AppPaths, db: &Db, profile_id: &str) -> Result<Option<ProfileWorlds>> {
    let Some(def) = definition(db, profile_id)? else {
        return Ok(None);
    };
    let ctx = Ctx::new(paths, db, profile_id, &def)?;
    let active = active(db, profile_id)?;
    let mut worlds = Vec::new();
    for w in records(db, profile_id)? {
        let is_active = active == Some(w.id);
        worlds.push(World {
            saves: ctx.save_count(&w.id, is_active)?,
            id: w.id,
            name: w.name,
            seed: w.seed,
            options: w.options,
            created_at: w.created_at,
            last_played_at: w.last_played_at,
            favourite: w.favourite,
            active: is_active,
        });
    }
    Ok(Some(ProfileWorlds {
        last_options: last_options(db, profile_id)?,
        definition: def,
        worlds,
        active,
    }))
}

/// Names a world; an empty name gives it back its default.
pub fn rename(db: &Db, profile_id: &str, id: u32, name: Option<&str>) -> Result<()> {
    let name = name.map(str::trim).filter(|n| !n.is_empty());
    let changed = db.conn().execute(
        "UPDATE worlds SET name = ?3 WHERE profile_id = ?1 AND id = ?2",
        params![profile_id, id, name],
    )?;
    if changed == 0 {
        return Err(Error::NotFound(format!("world {id}")));
    }
    Ok(())
}

pub fn set_favourite(db: &Db, profile_id: &str, id: u32, favourite: bool) -> Result<()> {
    let changed = db.conn().execute(
        "UPDATE worlds SET favourite = ?3 WHERE profile_id = ?1 AND id = ?2",
        params![profile_id, id, favourite],
    )?;
    if changed == 0 {
        return Err(Error::NotFound(format!("world {id}")));
    }
    Ok(())
}

/// Removes a world after backing up its files and saves. The active one
/// leaves `state/`, and no world is played until one is chosen or begun.
/// Not while the game runs: the caller holds the profile.
pub fn delete(paths: &AppPaths, db: &Db, profile_id: &str, id: u32) -> Result<()> {
    let def = definition(db, profile_id)?
        .ok_or_else(|| Error::NotFound(format!("worlds of {profile_id}")))?;
    let ctx = Ctx::new(paths, db, profile_id, &def)?;
    ctx.recover()?;
    if !exists(db, profile_id, id)? {
        return Err(Error::NotFound(format!("world {id}")));
    }
    if active(db, profile_id)? == Some(id) {
        let names = ctx.in_state()?;
        if !names.is_empty() {
            backups::create(
                paths,
                db,
                profile_id,
                Contents::Files(&names),
                Reason::WorldDeleted,
            )?;
        }
        for name in &names {
            fs::remove_file(ctx.owner.upper.join(name))?;
        }
        set_active(db, profile_id, None)?;
    } else {
        let dir = world_dir(paths, profile_id, id);
        let mut files = Vec::new();
        for name in file_names(&dir)? {
            files.push((name.clone(), fs::read(dir.join(&name))?));
        }
        if !files.is_empty() {
            backups::create_of_world(paths, db, profile_id, files, Reason::WorldDeleted, id)?;
        }
        if let Err(e) = fs::remove_dir_all(&dir)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            return Err(e.into());
        }
    }
    db.conn().execute(
        "DELETE FROM worlds WHERE profile_id = ?1 AND id = ?2",
        params![profile_id, id],
    )?;
    tracing::info!("{profile_id}: deleted world {id}");
    Ok(())
}

/// Makes `id` the world played without starting it: for the player to see
/// its saves, or to restore one of its backups. Not while the game runs.
pub fn activate(paths: &AppPaths, db: &Db, profile_id: &str, id: u32) -> Result<()> {
    let def = definition(db, profile_id)?
        .ok_or_else(|| Error::NotFound(format!("worlds of {profile_id}")))?;
    if !exists(db, profile_id, id)? {
        return Err(Error::NotFound(format!("world {id}")));
    }
    Ctx::new(paths, db, profile_id, &def)?.switch(Some(id))
}

/// A world as the backup made when its mod is removed keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptWorld {
    pub id: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub seed: Option<u32>,
    #[serde(default)]
    pub options: BTreeMap<String, u32>,
    pub created_at: u32,
    pub last_played_at: u32,
    #[serde(default)]
    pub favourite: bool,
}

/// A profile's worlds in a removal backup: its rows, and which was played.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptWorlds {
    pub active: Option<u32>,
    pub worlds: Vec<KeptWorld>,
}

/// A profile's worlds as stored: favourites first, then by last start.
fn records(db: &Db, profile_id: &str) -> Result<Vec<KeptWorld>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, name, seed, options_json, created_at, last_played_at, favourite
         FROM worlds WHERE profile_id = ?1
         ORDER BY favourite DESC, last_played_at DESC, id DESC",
    )?;
    let rows = stmt
        .query_map([profile_id], |r| {
            Ok(KeptWorld {
                id: r.get(0)?,
                name: r.get(1)?,
                seed: r.get(2)?,
                options: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default(),
                created_at: r.get(4)?,
                last_played_at: r.get(5)?,
                favourite: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Where a removal backup keeps the files of the worlds not played:
/// `bloogbase-worlds/<id>/<file>`.
pub const ARCHIVE_DIR: &str = "bloogbase-worlds";

/// For the backup made when a profile's mod is removed: its worlds, and
/// the files of those not played, as archive entries. `None` without any.
pub fn for_removal(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
) -> Result<Option<(KeptWorlds, ArchiveFiles)>> {
    if let Some(def) = definition(db, profile_id)? {
        Ctx::new(paths, db, profile_id, &def)?.recover()?;
    }
    let worlds = records(db, profile_id)?;
    if worlds.is_empty() {
        return Ok(None);
    }
    let files = slots::pack(&dir(paths, profile_id), ARCHIVE_DIR, |key| {
        worlds.iter().any(|w| w.id.to_string() == key)
    })?;
    let active = active(db, profile_id)?;
    Ok(Some((KeptWorlds { active, worlds }, files)))
}

/// Brings back the worlds a removal backup kept, with the files of those
/// not played (archive entries under [`ARCHIVE_DIR`]); the played one's are
/// restored into `state/` with the rest of the backup.
pub fn bring_back(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    kept: &KeptWorlds,
    files: &[(String, Vec<u8>)],
) -> Result<()> {
    slots::unpack(&dir(paths, profile_id), ARCHIVE_DIR, files, |key| {
        kept.worlds.iter().any(|w| w.id.to_string() == key)
    })?;
    {
        let conn = db.conn();
        for w in &kept.worlds {
            conn.execute(
                "INSERT OR REPLACE INTO worlds
                     (profile_id, id, name, seed, options_json, created_at, last_played_at,
                      favourite)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    profile_id,
                    w.id,
                    w.name,
                    w.seed,
                    serde_json::to_string(&w.options).expect("options serialize"),
                    w.created_at,
                    w.last_played_at,
                    w.favourite
                ],
            )?;
        }
    }
    set_active(db, profile_id, kept.active)?;
    tracing::info!("{profile_id}: brought back {} worlds", kept.worlds.len());
    Ok(())
}

/// The world played now, if any.
pub fn active_world(db: &Db, profile_id: &str) -> Result<Option<u32>> {
    active(db, profile_id)
}

#[cfg(test)]
mod tests;

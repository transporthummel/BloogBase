//! Variants of index mods (PLAN.md §9.7): ways to play one mod, such as
//! difficulty levels or a second part, each installed in its own folder
//! (`mods::variant_files`). Saves belong to a variant as they do to a
//! world: the variant played has its saves in the profile's `state/`, the
//! others' wait in `variants/<id>/` next to it ([`crate::slots`]).
//! Settings (DOSBox, the game's keys and sound, high scores) stay per mod.

use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::catalog::Game;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::mod_index::Variant;
use crate::paths::AppPaths;
use crate::profiles::{self, Layers};
use crate::slots::{self, ArchiveFiles, Owner, Slots};
use crate::{overlay_view, saves};

/// A variant as the Variants tab and the picker show it.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VariantState {
    pub variant: Variant,
    /// Its saves are in `state/`.
    pub active: bool,
    pub saves: u32,
    /// Its last start, Unix epoch seconds.
    pub last_played_at: Option<u32>,
}

/// What a profile of a mod with variants plays.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileVariants {
    /// In the entry's order.
    pub variants: Vec<VariantState>,
    pub active: Option<String>,
    /// The player chose `active`; until then "Play" asks.
    pub chosen: bool,
    /// The cover of every variant is the same, so the picker leaves out
    /// thumbnails; set by the command, which knows the covers.
    pub same_covers: bool,
}

/// The variants of a profile's mod; empty for most.
pub fn definition(db: &Db, profile_id: &str) -> Result<Vec<Variant>> {
    let json: Option<String> = db
        .conn()
        .query_row(
            "SELECT m.variants_json FROM profiles p JOIN mods m ON m.id = p.mod_id
             WHERE p.id = ?1",
            [profile_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(json
        .and_then(|j| match serde_json::from_str(&j) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("unreadable variants of {profile_id}: {e}");
                None
            }
        })
        .unwrap_or_default())
}

fn active(db: &Db, profile_id: &str) -> Result<Option<String>> {
    Ok(db.conn().query_row(
        "SELECT active_variant FROM profiles WHERE id = ?1",
        [profile_id],
        |r| r.get(0),
    )?)
}

/// The variant played now, for a mod with variants.
pub fn active_variant(db: &Db, profile_id: &str) -> Result<Option<String>> {
    match definition(db, profile_id)?.is_empty() {
        true => Ok(None),
        false => active(db, profile_id),
    }
}

/// Where the variants not played keep their saves.
pub fn dir(paths: &AppPaths, profile_id: &str) -> PathBuf {
    paths.profiles.join(profile_id).join("variants")
}

/// The folder holding a mod's variants, each in a folder named by its id;
/// `None` while the mod waits for its game or has no variant chosen.
fn installed(layers: &Layers) -> Option<&Path> {
    let files = &layers.mod_layer.as_ref()?.files;
    let parent = files.parent()?;
    parent
        .file_name()
        .is_some_and(|n| n == "variants")
        .then_some(parent)
}

/// The saves of a variant as DOS sees them: those it came with (`files`,
/// unless the player deleted them) and the player's own (`saved`, its
/// slot or `state/`). Variants keep saves in the drive root only.
fn save_count(game: &Game, files: &Path, saved: &Path) -> Result<u32> {
    Ok(overlay_view::list_root(files, saved)?
        .iter()
        .filter(|e| saves::is_save(game, &e.path))
        .count() as u32)
}

/// The game as variant `id` plays it: variants may name their saves each
/// their own way (Rescue of the Last's `.ROL` and `.RES`).
pub(crate) fn variant_game(db: &Db, profile_id: &str, id: &str) -> Result<&'static Game> {
    let (profile, layers) = profiles::get_as_variant(db, profile_id, id)?;
    saves::profile_game(&profile.game_id, &layers)
}

/// For the library card: the saves of the variants not played, those
/// they came with included.
pub(crate) fn others_save_count(
    db: &Db,
    profile_id: &str,
    layers: &Layers,
    active: &str,
) -> Result<u32> {
    let (Some(all), Some(profile)) = (installed(layers), layers.upper.parent()) else {
        return Ok(0);
    };
    let mut count = 0;
    for entry in std::fs::read_dir(all)? {
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_dir() && id != active {
            let game = variant_game(db, profile_id, &id)?;
            count += save_count(game, &entry.path(), &profile.join("variants").join(&id))?;
        }
    }
    Ok(count)
}

/// A profile's saves per variant: the game as it plays and `state/`.
struct Ctx<'a> {
    db: &'a Db,
    profile_id: &'a str,
    owner: Owner,
    dir: PathBuf,
    /// The folder of the mod's variants ([`installed`]).
    installed: Option<PathBuf>,
}

impl<'a> Ctx<'a> {
    fn new(paths: &'a AppPaths, db: &'a Db, profile_id: &'a str) -> Result<Self> {
        let (_, layers) = profiles::get(db, profile_id)?;
        Ok(Ctx {
            db,
            profile_id,
            owner: Owner::of(db, profile_id)?,
            dir: dir(paths, profile_id),
            installed: installed(&layers).map(Path::to_path_buf),
        })
    }
}

impl Slots for Ctx<'_> {
    type Key = String;

    fn dir(&self) -> &Path {
        &self.dir
    }

    fn owner(&self) -> &Owner {
        &self.owner
    }

    /// Saves belong to a variant, and so do the markers hiding the saves
    /// it came with; settings stay with the mod.
    fn belongs(&self, name: &str) -> bool {
        let game = self.owner.game;
        saves::is_save(game, name)
            || overlay_view::deleted_name(name).is_some_and(|n| saves::is_save(game, n))
    }

    fn save_count(&self, key: &String, active: bool) -> Result<u32> {
        let (saved, game) = match active {
            true => (self.owner.upper.clone(), self.owner.game),
            false => (
                self.slot_dir(key),
                variant_game(self.db, self.profile_id, key)?,
            ),
        };
        match &self.installed {
            Some(all) => save_count(game, &all.join(key), &saved),
            None => Ok(slots::file_names(&saved)?
                .iter()
                .filter(|n| saves::is_save(game, n))
                .count() as u32),
        }
    }

    fn active(&self) -> Result<Option<String>> {
        active(self.db, self.profile_id)
    }

    fn set_active(&self, key: Option<&String>) -> Result<()> {
        self.db.conn().execute(
            "UPDATE profiles SET active_variant = ?2, composed_hash = NULL WHERE id = ?1",
            params![self.profile_id, key],
        )?;
        Ok(())
    }
}

/// Makes `id` the variant played: its saves go into `state/`, those of the
/// one played before into its folder, and the mod is composed anew at the
/// next start. Not while the game runs: the caller holds the profile.
pub fn switch(paths: &AppPaths, db: &Db, profile_id: &str, id: &str) -> Result<()> {
    if !definition(db, profile_id)?.iter().any(|v| v.id == id) {
        return Err(Error::NotFound(format!("variant {id} of {profile_id}")));
    }
    Ctx::new(paths, db, profile_id)?.switch(Some(id.to_string()))
}

/// Finishes a switch cut short, before the game sees `state/`.
pub fn recover(paths: &AppPaths, db: &Db, profile_id: &str) -> Result<()> {
    if definition(db, profile_id)?.is_empty() {
        return Ok(());
    }
    Ctx::new(paths, db, profile_id)?.recover()
}

/// Records that the player chose the variant played.
pub fn set_chosen(db: &Db, profile_id: &str) -> Result<()> {
    db.conn().execute(
        "UPDATE profiles SET variant_chosen = 1 WHERE id = ?1",
        [profile_id],
    )?;
    Ok(())
}

/// A profile's variants, or `None` if its mod has none. Only reads.
pub fn list(paths: &AppPaths, db: &Db, profile_id: &str) -> Result<Option<ProfileVariants>> {
    let def = definition(db, profile_id)?;
    if def.is_empty() {
        return Ok(None);
    }
    let ctx = Ctx::new(paths, db, profile_id)?;
    let active = active(db, profile_id)?;
    let chosen: bool = db.conn().query_row(
        "SELECT variant_chosen FROM profiles WHERE id = ?1",
        [profile_id],
        |r| r.get(0),
    )?;
    let mut variants = Vec::new();
    for v in def {
        let is_active = active.as_deref() == Some(v.id.as_str());
        variants.push(VariantState {
            saves: ctx.save_count(&v.id, is_active)?,
            last_played_at: last_played(db, profile_id, &v.id)?,
            active: is_active,
            variant: v,
        });
    }
    Ok(Some(ProfileVariants {
        variants,
        active,
        chosen,
        same_covers: false,
    }))
}

/// The marker of a variant at the end of a session's setup
/// (`launcher::mod_setup`).
pub fn setup_suffix(id: &str) -> String {
    format!(";variant={id}")
}

fn last_played(db: &Db, profile_id: &str, id: &str) -> Result<Option<u32>> {
    Ok(db.conn().query_row(
        "SELECT MAX(started_at) FROM play_sessions
         WHERE profile_id = ?1 AND setup LIKE '%' || ?2",
        params![profile_id, setup_suffix(id)],
        |r| r.get(0),
    )?)
}

/// Where a removal backup keeps the saves of the variants not played:
/// `bloogbase-variants/<id>/<file>`.
pub const ARCHIVE_DIR: &str = "bloogbase-variants";

/// Which variant was played, in the backup made when the mod is removed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeptVariants {
    pub active: Option<String>,
}

/// For the backup made when a profile's mod is removed: the variant played,
/// and the saves of the others, as archive entries. `None` without
/// variants.
pub fn for_removal(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
) -> Result<Option<(KeptVariants, ArchiveFiles)>> {
    if definition(db, profile_id)?.is_empty() {
        return Ok(None);
    }
    Ctx::new(paths, db, profile_id)?.recover()?;
    let files = slots::pack(&dir(paths, profile_id), ARCHIVE_DIR, |_| true)?;
    let active = active(db, profile_id)?;
    Ok(Some((KeptVariants { active }, files)))
}

/// Brings back the saves of the variants not played that a removal backup
/// kept (archive entries under [`ARCHIVE_DIR`]), and plays the variant
/// played then, if the mod has it now; the played one's saves are restored
/// into `state/` with the rest of the backup.
pub fn bring_back(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    kept: &KeptVariants,
    files: &[(String, Vec<u8>)],
) -> Result<()> {
    let def = definition(db, profile_id)?;
    let has = |id: &str| def.iter().any(|v| v.id == id);
    let active = active(db, profile_id)?;
    slots::unpack(&dir(paths, profile_id), ARCHIVE_DIR, files, |_| true)?;
    if let Some(id) = &kept.active
        && has(id)
        && active.as_deref() != Some(id.as_str())
    {
        // Its saves come with the backup's others into `state/`; those of
        // the one played now wait in its folder.
        Ctx::new(paths, db, profile_id)?.switch(Some(id.clone()))?;
    }
    tracing::info!("{profile_id}: brought back the saves of its variants");
    Ok(())
}

#[cfg(test)]
mod tests;

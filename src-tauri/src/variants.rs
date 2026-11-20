//! Variants of index mods (PLAN.md §9.7): ways to play one mod, such as
//! difficulty levels or a second part, each installed in its own folder
//! (`mods::variant_files`). Saves belong to a variant as they do to a
//! world: the variant played has its saves in the profile's `state/`, the
//! others' wait in `variants/<id>/` next to it ([`crate::slots`]).
//! Settings (DOSBox, the game's keys and sound, high scores) stay per mod.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::catalog::Game;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::mod_index::Variant;
use crate::paths::AppPaths;
use crate::profiles;
use crate::saves;
use crate::slots::{Slots, file_names};

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

/// A profile's saves per variant: the game as it plays and `state/`.
struct Ctx<'a> {
    db: &'a Db,
    profile_id: &'a str,
    game: &'static Game,
    upper: PathBuf,
    dir: PathBuf,
}

impl<'a> Ctx<'a> {
    fn new(paths: &'a AppPaths, db: &'a Db, profile_id: &'a str) -> Result<Self> {
        let (profile, layers) = profiles::get(db, profile_id)?;
        let game = saves::profile_game(&profile.game_id, &layers)?;
        Ok(Ctx {
            db,
            profile_id,
            game,
            upper: layers.upper,
            dir: dir(paths, profile_id),
        })
    }

    fn save_count(&self, id: &str, active: bool) -> Result<u32> {
        let names = match active {
            true => file_names(&self.upper)?,
            false => file_names(&self.dir.join(id))?,
        };
        Ok(names
            .iter()
            .filter(|n| saves::is_save(self.game, n))
            .count() as u32)
    }
}

impl Slots for Ctx<'_> {
    type Key = String;

    fn dir(&self) -> &Path {
        &self.dir
    }

    fn upper(&self) -> &Path {
        &self.upper
    }

    /// Saves belong to a variant; settings stay with the mod.
    fn belongs(&self, name: &str) -> bool {
        saves::is_save(self.game, name)
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

/// Archive entries: path inside the archive, content.
pub type ArchiveFiles = Vec<(String, Vec<u8>)>;

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
    let dir = dir(paths, profile_id);
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            if !e.file_type()?.is_dir() {
                continue;
            }
            let id = e.file_name().to_string_lossy().into_owned();
            for name in file_names(&e.path())? {
                files.push((
                    format!("{ARCHIVE_DIR}/{id}/{name}"),
                    fs::read(e.path().join(&name))?,
                ));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
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
    for (name, data) in files {
        let Some((id, file)) = name
            .strip_prefix(ARCHIVE_DIR)
            .and_then(|r| r.strip_prefix('/'))
            .and_then(|r| r.split_once('/'))
        else {
            continue;
        };
        if file.contains('/') || id.is_empty() || id.contains(['.', '\\']) {
            continue;
        }
        let dir = dir(paths, profile_id).join(id);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(file), data)?;
    }
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

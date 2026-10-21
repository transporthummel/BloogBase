//! The save manager (PLAN.md §11): a profile's saved games as DOS sees them,
//! with what each save says about itself.

use std::fs;
use std::path::Path;

use serde::Serialize;
use specta::Type;

use crate::backups::{self, Contents, Reason};
use crate::catalog::{self, Engine, Game, glob_match};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::overlay_view::{self, Layer, OVERLAY_DELETED_FILE_PREFIX};
use crate::paths::AppPaths;
use crate::profiles::{self, Layers};

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveFile {
    /// File name in the drive root, spelled as on disk.
    pub file: String,
    /// The slot as the game numbers it.
    pub slot: Option<u32>,
    /// Unix epoch seconds.
    pub modified_at: Option<u32>,
    pub size: u32,
    /// `None` if the file couldn't be read as a save of this game.
    pub details: Option<SaveDetails>,
    /// Earlier versions of it kept in backups (§11.1); 0 for a save in a backup.
    pub versions: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveDetails {
    pub name: Option<String>,
    pub score: Option<i32>,
    pub lives: Option<i16>,
    pub ammo: Option<i16>,
    pub levels_done: Option<u16>,
    pub difficulty: Option<Difficulty>,
    pub on_world_map: Option<bool>,
    pub ship_parts: Option<u8>,
}

impl From<keen_saves::SaveInfo> for SaveDetails {
    fn from(i: keen_saves::SaveInfo) -> Self {
        SaveDetails {
            name: i.name,
            score: i.score,
            lives: i.lives,
            ammo: i.ammo,
            levels_done: i.levels_done,
            difficulty: i.difficulty.map(|d| match d {
                keen_saves::Difficulty::Easy => Difficulty::Easy,
                keen_saves::Difficulty::Normal => Difficulty::Normal,
                keen_saves::Difficulty::Hard => Difficulty::Hard,
            }),
            on_world_map: i.on_world_map,
            ship_parts: i.ship_parts,
        }
    }
}

pub fn is_save(game: &Game, name: &str) -> bool {
    !name.contains('/') && game.saves.iter().any(|p| glob_match(p, name))
}

/// The slot number the game shows for a save file name: Keen 1-3 count from
/// the digit in the name, the others number slots from 1 for file 0.
fn slot(game: &Game, name: &str) -> Option<u32> {
    let pattern = game.saves.iter().find(|p| glob_match(p, name))?;
    let at = pattern.find('?')?;
    let digit = name
        .as_bytes()
        .get(at)
        .and_then(|b| (*b as char).to_digit(10))?;
    Some(match game.engine {
        Engine::Vorticons => digit,
        Engine::Galaxy | Engine::Dreams => digit + 1,
    })
}

/// Describes one save from its bytes.
pub fn describe(game: &Game, file: &str, data: &[u8], modified_at: Option<u32>) -> SaveFile {
    let details =
        keen_saves::Episode::from_game_id(&game.id).and_then(|ep| {
            match keen_saves::parse(ep, data) {
                Ok(info) => Some(info.into()),
                Err(e) => {
                    tracing::debug!("{file} isn't a readable {} save: {e}", game.id);
                    None
                }
            }
        });
    SaveFile {
        file: file.to_string(),
        slot: slot(game, file),
        modified_at,
        size: data.len() as u32,
        details,
        versions: 0,
    }
}

pub(crate) fn game_of(game_id: &str) -> Result<&'static Game> {
    catalog::game(game_id).ok_or_else(|| Error::NotFound(format!("game {game_id}")))
}

/// Where a file seen through the overlay actually lives.
pub(crate) fn host_path(layers: &Layers, entry: &overlay_view::Entry) -> std::path::PathBuf {
    match entry.layer {
        Layer::Upper => layers.upper.join(&entry.path),
        Layer::Lower => layers.view_lower().join(&entry.path),
    }
}

/// A profile's saves, by slot.
pub fn list(db: &Db, profile_id: &str) -> Result<Vec<SaveFile>> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let game = game_of(&profile.game_id)?;
    let mut out = Vec::new();
    for entry in overlay_view::list(layers.view_lower(), &layers.upper)? {
        if !is_save(game, &entry.path) {
            continue;
        }
        let path = host_path(&layers, &entry);
        let data = fs::read(&path)?;
        let mut save = describe(game, &entry.path, &data, modified(&path));
        save.versions = backups::version_count(db, profile_id, &entry.path, &data)?;
        out.push(save);
    }
    out.sort_by(|a, b| (a.slot, &a.file).cmp(&(b.slot, &b.file)));
    Ok(out)
}

pub(crate) fn modified(path: &Path) -> Option<u32> {
    let time = fs::metadata(path).ok()?.modified().ok()?;
    let secs = time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    u32::try_from(secs).ok()
}

/// Deletes a save after backing it up. A save that comes with a mod is
/// hidden with a deletion marker, like DOSBox does it.
pub fn delete(paths: &AppPaths, db: &Db, profile_id: &str, file: &str) -> Result<()> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let game = game_of(&profile.game_id)?;
    let entries = overlay_view::list(layers.view_lower(), &layers.upper)?;
    let Some(entry) = entries
        .iter()
        .find(|e| e.path == file && is_save(game, &e.path))
    else {
        return Err(Error::NotFound(format!("save {file}")));
    };
    backups::create(
        paths,
        db,
        profile_id,
        Contents::Files(std::slice::from_ref(&entry.path)),
        Reason::BeforeDelete,
    )?;
    let upper = layers.upper.join(&entry.path);
    if upper.exists() {
        fs::remove_file(&upper)?;
    }
    // Is a file of that name left below, now showing through?
    let below = overlay_view::list(layers.view_lower(), &layers.upper)?
        .into_iter()
        .find(|e| e.path.eq_ignore_ascii_case(&entry.path));
    if let Some(lower) = below {
        fs::create_dir_all(&layers.upper)?;
        fs::write(
            layers.upper.join(format!(
                "{OVERLAY_DELETED_FILE_PREFIX}{}",
                lower.path.to_ascii_uppercase()
            )),
            b"empty",
        )?;
    }
    tracing::info!("deleted save {file} of {profile_id}");
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::library;
    use crate::source::testutil::*;

    pub fn setup() -> (tempfile::TempDir, AppPaths, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        let db = Db::open_in_memory().unwrap();
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        library::import(&paths, &db, &src).unwrap();
        (tmp, paths, db)
    }

    /// A Keen 4 save with the given name, as the game writes its header.
    pub fn keen4_save(name: &str) -> Vec<u8> {
        let mut data = vec![0u8; 0x100];
        data[..4].copy_from_slice(b"CK4\0");
        data[8..8 + name.len()].copy_from_slice(name.as_bytes());
        data[0x60..0x64].copy_from_slice(&500i32.to_le_bytes());
        data
    }

    pub fn state(paths: &AppPaths) -> std::path::PathBuf {
        paths.profiles.join("keen4-1/state")
    }

    #[test]
    fn lists_saves_by_slot_with_details() {
        let (_tmp, paths, db) = setup();
        // KEEN4 fixture brings SAVEGAM0.CK4 along, moved into state/ on import.
        fs::write(state(&paths).join("SAVEGAM0.CK4"), keen4_save("first")).unwrap();
        fs::write(state(&paths).join("savegam3.ck4"), keen4_save("fourth")).unwrap();
        fs::write(state(&paths).join("SAVEGAM5.CK4"), b"garbage").unwrap();
        fs::write(state(&paths).join("CONFIG.CK4"), b"c").unwrap();

        let saves = list(&db, "keen4-1").unwrap();
        let got: Vec<(&str, Option<u32>, Option<&str>)> = saves
            .iter()
            .map(|s| {
                (
                    s.file.as_str(),
                    s.slot,
                    s.details.as_ref().and_then(|d| d.name.as_deref()),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("SAVEGAM0.CK4", Some(1), Some("first")),
                ("savegam3.ck4", Some(4), Some("fourth")),
                ("SAVEGAM5.CK4", Some(6), None),
            ]
        );
        assert_eq!(saves[0].details.as_ref().unwrap().score, Some(500));
        assert!(saves[0].modified_at.is_some());
    }

    #[test]
    fn vorticons_slots_use_the_digit() {
        let k1 = catalog::game("keen1").unwrap();
        assert_eq!(slot(k1, "SAVED3.CK1"), Some(3));
        let kd = catalog::game("kdreams").unwrap();
        assert_eq!(slot(kd, "SAVEGM0.KDR"), Some(1));
        assert_eq!(slot(kd, "SAVEGAMx.KDR"), None);
    }

    #[test]
    fn delete_backs_up_and_hides_lower_saves() {
        let (_tmp, paths, db) = setup();
        let st = state(&paths);
        fs::write(st.join("SAVEGAM1.CK4"), keen4_save("mine")).unwrap();
        delete(&paths, &db, "keen4-1", "SAVEGAM1.CK4").unwrap();
        assert!(!st.join("SAVEGAM1.CK4").exists());
        assert!(
            !fs::read_dir(&st)
                .unwrap()
                .any(|e| overlay_view::is_marker(&e.unwrap().file_name().to_string_lossy()))
        );
        let b = backups::list(&db, "keen4-1").unwrap();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].files, ["SAVEGAM1.CK4"]);
        assert_eq!(b[0].reason, Some(Reason::BeforeDelete));

        // A save in the lower layer only gets a marker.
        let lower = paths.library.join("keen4-1/base");
        fs::write(lower.join("SAVEGAM2.CK4"), keen4_save("base")).unwrap();
        delete(&paths, &db, "keen4-1", "SAVEGAM2.CK4").unwrap();
        assert!(lower.join("SAVEGAM2.CK4").exists());
        assert!(st.join("DBOVERLAY_DEL_SAVEGAM2.CK4").exists());
        assert!(
            list(&db, "keen4-1")
                .unwrap()
                .iter()
                .all(|s| s.file != "SAVEGAM2.CK4")
        );

        assert!(matches!(
            delete(&paths, &db, "keen4-1", "CONFIG.CK4"),
            Err(Error::NotFound(_))
        ));
        assert!(matches!(
            delete(&paths, &db, "keen4-1", "../x"),
            Err(Error::NotFound(_))
        ));
    }
}

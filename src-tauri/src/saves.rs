//! The save manager (PLAN.md §11): a profile's saved games as DOS sees them,
//! with what each save says about itself.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use specta::Type;

use crate::backups::{self, Contents, Reason};
use crate::catalog::{self, Engine, Game, glob_match};
use crate::covers;
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

/// The game as a profile plays it. A mod's patch may write other names over
/// the ones the executable opens its saves and state files under: Keen Q
/// saves as `SAVED1.CKQ`, Norp the Yorp 2 keeps its settings in
/// `SETTINGS.NY2`. The catalog's patterns then follow.
pub(crate) fn profile_game(game_id: &str, layers: &Layers) -> Result<&'static Game> {
    let game = game_of(game_id)?;
    let Some(m) = &layers.mod_layer else {
        return Ok(game);
    };
    // A mod waiting for its game has no executable to read.
    let Ok(files) = covers::Files::new(&layers.sources()) else {
        return Ok(game);
    };
    // Reading the executable takes unpacking it, so the answer is kept for
    // as long as the files stay the same.
    static KNOWN: OnceLock<Mutex<HashMap<String, &'static Game>>> = OnceLock::new();
    let known = KNOWN.get_or_init(Default::default);
    let key = files.fingerprint(Some(&m.recipe));
    if let Some(g) = known.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Ok(g);
    }
    let texts = covers::patch_texts(game, &m.recipe, &files);
    let renamed = covers::renamer(game, &files, &texts);
    let rename = |patterns: &[String]| rename_patterns(patterns, &renamed);
    let (saves, state_files) = (rename(&game.saves), rename(&game.state_files));
    let played = if saves == game.saves && state_files == game.state_files {
        game
    } else {
        tracing::info!("{game_id} mod saves as {saves:?}, state {state_files:?}");
        // One per mod and change of its files while BloogBase runs.
        Box::leak(Box::new(Game {
            saves,
            state_files,
            ..game.clone()
        }))
    };
    known
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, played);
    Ok(played)
}

/// Each pattern under its new name, once: Version 1.21 Revision 2 names
/// `KDREAMS.CFG` like the `CONFIG.KDR` of later releases.
fn rename_patterns(patterns: &[String], renamed: impl Fn(&str) -> String) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(patterns.len());
    for p in patterns {
        let new = rename_pattern(p, &renamed);
        if !out.contains(&new) {
            out.push(new);
        }
    }
    out
}

/// `SAVED?.CK2` under the names `renamed` gives its parts: the whole name
/// (Keen Dreams' `CONFIG.KDR`), else the stem before the `?` and the
/// extension, each written over the executable on its own. Only what can be
/// a DOS name counts; a patch may start a sentence where a name was.
fn rename_pattern(pattern: &str, renamed: impl Fn(&str) -> String) -> String {
    let dos = |s: &str, max: usize| {
        !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };
    let part = |s: &str, max: usize| {
        let new = renamed(s).trim_end().to_ascii_uppercase();
        if dos(&new, max) { new } else { s.to_string() }
    };
    let Some((name, ext)) = pattern.rsplit_once('.') else {
        return pattern.to_string();
    };
    let whole = renamed(pattern);
    if whole != pattern
        && let Some((n, e)) = whole.trim_end().rsplit_once('.')
        && dos(n, 8)
        && dos(e, 3)
    {
        return whole.trim_end().to_ascii_uppercase();
    }
    let (stem, rest) = name.split_at(name.find('?').unwrap_or(name.len()));
    if stem.is_empty() {
        return pattern.to_string();
    }
    format!("{}{rest}.{}", part(stem, 8 - rest.len()), part(ext, 3))
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
    let game = profile_game(&profile.game_id, &layers)?;
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
/// hidden with a deletion marker, like DOSBox does it. Returns the backup
/// that holds it, to undo the deletion from.
pub fn delete(paths: &AppPaths, db: &Db, profile_id: &str, file: &str) -> Result<u32> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let game = profile_game(&profile.game_id, &layers)?;
    let entries = overlay_view::list(layers.view_lower(), &layers.upper)?;
    let Some(entry) = entries
        .iter()
        .find(|e| e.path == file && is_save(game, &e.path))
    else {
        return Err(Error::NotFound(format!("save {file}")));
    };
    let made = backups::create(
        paths,
        db,
        profile_id,
        Contents::Files(std::slice::from_ref(&entry.path)),
        Reason::BeforeDelete,
    )?;
    // Skipped because the newest backup holds exactly this save already.
    let backup = match made {
        Some(b) => b.id,
        None => backups::list(db, profile_id)?
            .first()
            .map(|b| b.id)
            .ok_or_else(|| Error::NotFound(format!("backup of {file}")))?,
    };
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
    Ok(backup)
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
        let id = delete(&paths, &db, "keen4-1", "SAVEGAM1.CK4").unwrap();
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
        assert_eq!(b[0].id, id);

        // Just backed up by hand: that backup is the one to undo from.
        fs::write(st.join("SAVEGAM3.CK4"), keen4_save("kept")).unwrap();
        let own = backups::create(
            &paths,
            &db,
            "keen4-1",
            Contents::Files(&["SAVEGAM3.CK4".into()]),
            backups::Origin::User(None),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            delete(&paths, &db, "keen4-1", "SAVEGAM3.CK4").unwrap(),
            own.id
        );
        assert_eq!(backups::list(&db, "keen4-1").unwrap().len(), 2);

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

    #[test]
    fn patterns_follow_names_written_over_the_executable() {
        let names = |pairs: &'static [(&'static str, &'static str)]| {
            move |s: &str| {
                pairs
                    .iter()
                    .find(|(from, _)| *from == s)
                    .map_or(s.to_string(), |(_, to)| to.to_string())
            }
        };
        // Stem and extension each on their own, as Norp the Yorp 2.
        let norp = names(&[("SAVED", "NSAVE"), ("CK2", "NY2"), ("CTLPANEL", "SETTINGS")]);
        assert_eq!(rename_pattern("SAVED?.CK2", norp), "NSAVE?.NY2");
        assert_eq!(rename_pattern("CTLPANEL.CK2", norp), "SETTINGS.NY2");
        // The whole name, as Another Helping of Veggies.
        let veggies = names(&[("KDREAMS.CFG", "CONFIG.KD2")]);
        assert_eq!(rename_pattern("KDREAMS.CFG", veggies), "CONFIG.KD2");
        // A new name another pattern already has counts once.
        let revision_2 = names(&[("KDREAMS.CFG", "CONFIG.KDR")]);
        let state = ["CONFIG.KDR".to_string(), "KDREAMS.CFG".to_string()];
        assert_eq!(rename_patterns(&state, revision_2), ["CONFIG.KDR"]);
        // Text that can't be a name leaves it.
        let text = names(&[("SAVED", "Saved games"), ("CK2", "CKQ ")]);
        assert_eq!(rename_pattern("SAVED?.CK2", text), "SAVED?.CKQ");
    }

    #[test]
    fn a_mods_patch_renames_its_saves() {
        use crate::recipe::Recipe;
        let tmp = tempfile::tempdir().unwrap();
        let (base, files) = (tmp.path().join("base"), tmp.path().join("mod"));
        // A header of two paragraphs; the load image starts after it.
        let image = b"SAVED\0CTLPANEL\0CK2\0";
        let exe = [b"MZ".as_slice(), &[0; 6], &[2, 0], &[0; 0x16], image].concat();
        write_folder(&base, &[("KEEN2.EXE", &exe)]);
        write_folder(
            &files,
            &[(
                "Q.PAT",
                b"%ext ck2\n%patch $F \"CKQ\"\n%patch $0 \"NSAVE\"\n%end\n",
            )],
        );
        let layers = |patch: &str| Layers {
            lower: Some(base.clone()),
            upper: tmp.path().join("state"),
            mod_layer: Some(profiles::ModLayer {
                files: files.clone(),
                base: None,
                recipe: Recipe::Patch {
                    patcher: "CK2PATCH.EXE".into(),
                    patch: patch.into(),
                    args: Vec::new(),
                },
            }),
        };
        let game = profile_game("keen2", &layers("Q.PAT")).unwrap();
        assert_eq!(game.saves, ["NSAVE?.CKQ"]);
        assert_eq!(game.state_files, ["CTLPANEL.CKQ", "SCORES.CKQ"]);
        assert!(is_save(game, "nsave1.ckq") && !is_save(game, "SAVED1.CK2"));
        // Without the patch, the catalog's.
        let plain = profile_game("keen2", &layers("NONE.PAT")).unwrap();
        assert_eq!(plain.saves, ["SAVED?.CK2"]);
    }
}

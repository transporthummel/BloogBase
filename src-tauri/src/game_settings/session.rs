//! A session with global settings: they go into the game's settings file
//! before it, and come out again after it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::format::{self, Edit, Format};
use super::template;
use crate::catalog::Game;
use crate::db::Db;
use crate::error::Result;
use crate::firing;
use crate::overlay_view::{self, Layer};
use crate::paths::AppPaths;
use crate::profiles;
use crate::recipe::Recipe;
use crate::saves;

/// What a session changed, kept beside it so a crash can't lose the player's
/// values.
#[derive(Debug, Serialize, Deserialize)]
struct Pending {
    /// As it's spelled in `state/`.
    file: String,
    origin: Origin,
    fields: Vec<Field>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Origin {
    /// In `state/` before the session.
    State,
    /// Only below it, in the game's or the mod's files.
    Lower,
    /// Nowhere: made for this session.
    Created,
}

#[derive(Debug, Serialize, Deserialize)]
struct Field {
    offset: usize,
    wide: bool,
    original: u16,
    written: u16,
}

/// Writes the global settings into the profile's settings file. Does nothing
/// if none are set, or the file is in a format we don't know.
pub fn apply(paths: &AppPaths, db: &Db, profile_id: &str) -> Result<()> {
    // The merge before this start failed: the file still holds the last
    // session's values, and the record is all that knows the player's.
    if sidecar(paths, profile_id).exists() {
        tracing::warn!("global settings: {profile_id} still has a session to merge, not applying");
        return Ok(());
    }
    let settings = super::get(db)?;
    if settings == super::GameSettings::default() {
        return Ok(());
    }
    let (profile, layers) = profiles::get(db, profile_id)?;
    let game = saves::profile_game(&profile.game_id, &layers)?;
    let Some(name) = game.state_files.first() else {
        return Ok(());
    };
    // The settings file comes first; Keen Dreams has two names for it, and a
    // source mod may add its own.
    let entries = overlay_view::list_root(layers.view_lower(), &layers.upper)?;
    let entry = game
        .state_files
        .iter()
        .find_map(|f| entries.iter().find(|e| e.path.eq_ignore_ascii_case(f)))
        .cloned();
    let (file, origin, mut data) = match entry {
        Some(entry) => {
            let (origin, source) = match entry.layer {
                Layer::Upper => (Origin::State, layers.upper.join(&entry.path)),
                Layer::Lower => (Origin::Lower, layers.view_lower().join(&entry.path)),
            };
            let data = fs::read(source)?;
            (entry.path, origin, data)
        }
        None => {
            let Some(data) = new_file(&profile, &layers, game) else {
                tracing::info!("global settings: no {name} yet for {profile_id}, first start");
                return Ok(());
            };
            (name.clone(), Origin::Created, data)
        }
    };
    let Some(file_format) = format::recognize(&profile.game_id, &data) else {
        tracing::info!("global settings: leaving {name} of {profile_id} alone, unknown format");
        return Ok(());
    };
    let edits = format::edits(file_format, &settings, &data);
    if edits.is_empty() {
        return Ok(());
    }
    let fields = fields(&data, &edits);
    for edit in &edits {
        format::write(&mut data, edit);
    }
    let pending = Pending {
        file,
        origin,
        fields,
    };
    write_pending(&sidecar(paths, profile_id), &pending)?;
    if origin == Origin::Created {
        // A file deleted in the game stays hidden from below by a marker.
        overlay_view::prepare_write(&layers.upper, &pending.file)?;
    }
    if let Err(e) = fs::write(layers.upper.join(&pending.file), &data) {
        let _ = fs::remove_file(sidecar(paths, profile_id));
        return Err(e.into());
    }
    Ok(())
}

/// Takes the global settings out of the file again, if a session left any:
/// a value still as written gets its original back, anything else in the file
/// is the game's or the player's and stays.
pub fn merge_pending(paths: &AppPaths, db: &Db, profile_id: &str) -> Result<()> {
    let path = sidecar(paths, profile_id);
    let text = match fs::read(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let pending: Pending = match serde_json::from_slice(&text) {
        Ok(pending) => pending,
        Err(e) => {
            tracing::warn!("dropping the unreadable global settings record of {profile_id}: {e}");
            fs::remove_file(&path)?;
            return Ok(());
        }
    };
    let (_, layers) = profiles::get(db, profile_id)?;
    let file = layers.upper.join(&pending.file);
    match fs::read(&file) {
        Ok(data) => restore(&file, data, &pending, layers.view_lower())?,
        // Deleted meanwhile, e.g. with the profile's saves.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    fs::remove_file(&path)?;
    Ok(())
}

fn restore(file: &Path, mut data: Vec<u8>, pending: &Pending, lower: &Path) -> Result<()> {
    let before = data.clone();
    for field in &pending.fields {
        if format::read(&data, field.offset, field.wide) == Some(field.written) {
            format::write(
                &mut data,
                &Edit {
                    offset: field.offset,
                    wide: field.wide,
                    value: field.original,
                },
            );
        }
    }
    // Nothing of the session is left: the file below does again.
    if pending.origin == Origin::Lower
        && fs::read(lower.join(&pending.file)).is_ok_and(|below| below == data)
    {
        fs::remove_file(file)?;
    } else if data != before {
        fs::write(file, data)?;
    }
    Ok(())
}

/// A new settings file for a profile without one; `None` where we can't tell
/// what it would hold. Then the game writes its own after the first session.
fn new_file(
    profile: &profiles::Profile,
    layers: &profiles::Layers,
    game: &Game,
) -> Option<Vec<u8>> {
    let file_format =
        template::format_for_version(&profile.game_id, profile.version_id.as_deref())?;
    if file_format == Format::Vorticons {
        return template::new_file(file_format, &profile.game_id, None);
    }
    // A source mod may keep its settings in a file of its own.
    if file_format != Format::Dreams && game.state_files.len() > 1 {
        return None;
    }
    let recipe = layers
        .mod_layer
        .as_ref()
        .map_or(&Recipe::Episode, |m| &m.recipe);
    let image = firing::image_as_dos_runs(game, recipe, &layers.sources())?;
    let scores = template::find_score_table(&image, template::table_records(file_format))?;
    template::new_file(file_format, &profile.game_id, Some(scores))
}

/// The fields `edits` change in `data`, with their values before.
fn fields(data: &[u8], edits: &[Edit]) -> Vec<Field> {
    edits
        .iter()
        .filter_map(|edit| {
            Some(Field {
                offset: edit.offset,
                wide: edit.wide,
                original: format::read(data, edit.offset, edit.wide)?,
                written: edit.value,
            })
        })
        .collect()
}

fn sidecar(paths: &AppPaths, profile_id: &str) -> PathBuf {
    paths.profile(profile_id).settings_session
}

/// In one step, so a crash leaves it whole or not at all.
fn write_pending(path: &Path, pending: &Pending) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let json = serde_json::to_vec(pending)
        .map_err(|e| crate::error::Error::InvalidSetting(e.to_string()))?;
    fs::write(&tmp, json)?;
    fs::rename(tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_settings::format::tests::file as sample;
    use crate::game_settings::{self, GameSettings};
    use crate::library;
    use crate::source::testutil::*;

    const SOUND: usize = 0x206;
    const MUSIC: usize = 0x208;
    const SCORE: usize = 0x10;

    fn setup() -> (tempfile::TempDir, AppPaths, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        let db = Db::open_in_memory().unwrap();
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        library::import(&paths, &db, &src).unwrap();
        (tmp, paths, db)
    }

    fn sound_off(db: &Db) {
        let settings = GameSettings {
            sound: Some(false),
            ..GameSettings::default()
        };
        game_settings::set(db, &settings).unwrap();
    }

    fn upper(db: &Db, profile: &str) -> PathBuf {
        profiles::get(db, profile).unwrap().1.upper
    }

    /// The import's `config.ck4`, now a real one.
    fn config(db: &Db) -> (PathBuf, Vec<u8>) {
        let path = upper(db, "keen4-1").join("config.ck4");
        let data = sample(Format::GalaxyNew);
        fs::write(&path, &data).unwrap();
        (path, data)
    }

    fn pending(paths: &AppPaths) -> PathBuf {
        sidecar(paths, "keen4-1")
    }

    #[test]
    fn applies_and_restores_a_file_in_state() {
        let (_tmp, paths, db) = setup();
        let (file, original) = config(&db);
        sound_off(&db);

        apply(&paths, &db, "keen4-1").unwrap();
        assert_eq!(fs::read(&file).unwrap()[SOUND], 0);
        assert!(pending(&paths).is_file());

        merge_pending(&paths, &db, "keen4-1").unwrap();
        assert_eq!(fs::read(&file).unwrap(), original);
        assert!(!pending(&paths).exists());
    }

    #[test]
    fn keeps_high_scores_and_in_game_changes() {
        let (_tmp, paths, db) = setup();
        let (file, original) = config(&db);
        let settings = GameSettings {
            sound: Some(false),
            music: Some(false),
            ..GameSettings::default()
        };
        game_settings::set(&db, &settings).unwrap();
        apply(&paths, &db, "keen4-1").unwrap();

        // The game wrote a new score, and the player put sound on PC Speaker.
        let mut played = fs::read(&file).unwrap();
        played[SCORE] = 0x77;
        played[SOUND] = 1;
        fs::write(&file, &played).unwrap();

        merge_pending(&paths, &db, "keen4-1").unwrap();
        let merged = fs::read(&file).unwrap();
        assert_eq!(merged[SCORE], 0x77);
        assert_eq!(merged[SOUND], 1);
        assert_eq!(merged[MUSIC], original[MUSIC], "as written: restored");
    }

    #[test]
    fn file_from_the_mod_goes_back_to_the_mod() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("mod");
        let data = sample(Format::GalaxyNew);
        write_folder(
            &src,
            &[
                ("PLAY.BAT", b"ck4patch mod.pat"),
                ("CK4PATCH.EXE", b"p"),
                ("MOD.PAT", b"%ext ck4"),
                ("CONFIG.CK4", &data),
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
        sound_off(&db);
        let copy = upper(&db, &m.profile_id).join("CONFIG.CK4");

        apply(&paths, &db, &m.profile_id).unwrap();
        assert_eq!(fs::read(&copy).unwrap()[SOUND], 0);
        merge_pending(&paths, &db, &m.profile_id).unwrap();
        assert!(!copy.exists(), "unchanged: the mod's file shows again");

        apply(&paths, &db, &m.profile_id).unwrap();
        let mut played = fs::read(&copy).unwrap();
        played[SCORE] = 0x77;
        fs::write(&copy, &played).unwrap();
        merge_pending(&paths, &db, &m.profile_id).unwrap();
        let kept = fs::read(&copy).unwrap();
        assert_eq!((kept[SCORE], kept[SOUND]), (0x77, data[SOUND]));
    }

    #[test]
    fn recovers_on_next_start() {
        let (_tmp, paths, db) = setup();
        let (file, original) = config(&db);
        sound_off(&db);
        apply(&paths, &db, "keen4-1").unwrap();

        // DOSBox died; the next start merges first, so the record holds the
        // player's value and not the one written last time.
        merge_pending(&paths, &db, "keen4-1").unwrap();
        apply(&paths, &db, "keen4-1").unwrap();
        merge_pending(&paths, &db, "keen4-1").unwrap();
        assert_eq!(fs::read(&file).unwrap(), original);
    }

    #[test]
    fn broken_sidecar_is_dropped() {
        let (_tmp, paths, db) = setup();
        let (file, original) = config(&db);
        fs::create_dir_all(pending(&paths).parent().unwrap()).unwrap();
        fs::write(pending(&paths), "{").unwrap();

        merge_pending(&paths, &db, "keen4-1").unwrap();
        assert!(!pending(&paths).exists());
        assert_eq!(fs::read(&file).unwrap(), original);
    }

    #[test]
    fn settings_changed_during_session_dont_matter() {
        let (_tmp, paths, db) = setup();
        let (file, original) = config(&db);
        sound_off(&db);
        apply(&paths, &db, "keen4-1").unwrap();
        game_settings::set(&db, &GameSettings::default()).unwrap();

        merge_pending(&paths, &db, "keen4-1").unwrap();
        assert_eq!(fs::read(&file).unwrap(), original);
    }

    #[test]
    fn restored_backup_wins() {
        let (_tmp, paths, db) = setup();
        let (file, _) = config(&db);
        sound_off(&db);
        apply(&paths, &db, "keen4-1").unwrap();

        let mut restored = sample(Format::GalaxyNew);
        restored[SOUND] = 1;
        restored[SCORE] = 0x55;
        fs::write(&file, &restored).unwrap();

        merge_pending(&paths, &db, "keen4-1").unwrap();
        assert_eq!(fs::read(&file).unwrap(), restored);
    }

    #[test]
    fn file_gone_or_changed_format() {
        let (_tmp, paths, db) = setup();
        let (file, _) = config(&db);
        sound_off(&db);

        apply(&paths, &db, "keen4-1").unwrap();
        fs::remove_file(&file).unwrap();
        merge_pending(&paths, &db, "keen4-1").unwrap();
        assert!(!pending(&paths).exists());
        assert!(!file.exists());

        config(&db);
        apply(&paths, &db, "keen4-1").unwrap();
        fs::write(&file, [1; 10]).unwrap();
        merge_pending(&paths, &db, "keen4-1").unwrap();
        assert!(!pending(&paths).exists());
        assert_eq!(fs::read(&file).unwrap(), [1; 10]);
    }

    #[test]
    fn unknown_format_is_left_alone() {
        let (_tmp, paths, db) = setup();
        let file = upper(&db, "keen4-1").join("config.ck4");
        sound_off(&db);

        apply(&paths, &db, "keen4-1").unwrap();
        assert!(!pending(&paths).exists());
        assert_eq!(fs::read(&file).unwrap(), b"cfg");
    }

    #[test]
    fn nothing_set_touches_nothing() {
        let (_tmp, paths, db) = setup();
        let (file, original) = config(&db);

        apply(&paths, &db, "keen4-1").unwrap();
        assert!(!pending(&paths).exists());
        assert_eq!(fs::read(&file).unwrap(), original);
    }

    #[test]
    fn unwritable_file_is_an_error_and_leaves_no_record() {
        let (_tmp, paths, db) = setup();
        let (file, original) = config(&db);
        sound_off(&db);
        let mut permissions = fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file, permissions.clone()).unwrap();

        assert!(apply(&paths, &db, "keen4-1").is_err());
        assert!(!pending(&paths).exists());
        assert_eq!(fs::read(&file).unwrap(), original);

        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        fs::set_permissions(&file, permissions).unwrap();
    }

    #[test]
    fn missing_file_is_created_and_stays() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("k1");
        write_folder(
            &src,
            &[
                ("KEEN1.EXE", b"exe"),
                ("EGAHEAD.CK1", b"h"),
                ("EGALATCH.CK1", b"l"),
                ("EGASPRIT.CK1", b"s"),
                ("LEVEL01.CK1", b"1"),
                ("SOUNDS.CK1", b"a"),
            ],
        );
        let profile = library::import(&paths, &db, &src).unwrap()[0].id.clone();
        let file = upper(&db, &profile).join("CTLPANEL.CK1");
        sound_off(&db);

        apply(&paths, &db, &profile).unwrap();
        assert_eq!(fs::read(&file).unwrap()[0], 0);
        merge_pending(&paths, &db, &profile).unwrap();
        assert!(!sidecar(&paths, &profile).exists());
        let merged = fs::read(&file).unwrap();
        assert_eq!(merged[0], 1, "the default again");
        assert_eq!(merged[0x2A], 0x1D);
    }

    #[test]
    fn keen_dreams_later_name_is_used() {
        let (tmp, paths, db) = setup();
        let src = tmp.path().join("kd");
        write_folder(
            &src,
            &[
                ("KDREAMS.EXE", b"exe"),
                ("KDREAMS.MAP", b"m"),
                ("KDREAMS.AUD", b"a"),
                ("KDREAMS.CFG", &sample(Format::Dreams)),
            ],
        );
        let profile = library::import(&paths, &db, &src).unwrap()[0].id.clone();
        sound_off(&db);

        apply(&paths, &db, &profile).unwrap();
        let file = upper(&db, &profile).join("KDREAMS.CFG");
        assert_eq!(fs::read(&file).unwrap()[0x280], 0);
        assert!(!upper(&db, &profile).join("CONFIG.KDR").exists());
    }

    #[test]
    fn no_table_no_file() {
        let (_tmp, paths, db) = setup();
        let file = upper(&db, "keen4-1").join("config.ck4");
        fs::remove_file(&file).unwrap();
        sound_off(&db);

        // The test game's program holds no high scores.
        apply(&paths, &db, "keen4-1").unwrap();
        assert!(!pending(&paths).exists());
        assert!(!file.exists());
        assert!(!upper(&db, "keen4-1").join("CONFIG.CK4").exists());
    }

    #[test]
    fn an_unmerged_record_is_never_replaced() {
        let (_tmp, paths, db) = setup();
        let (file, original) = config(&db);
        sound_off(&db);
        apply(&paths, &db, "keen4-1").unwrap();

        // The merge failed; the player changes a setting and starts again.
        let settings = GameSettings {
            music: Some(false),
            ..GameSettings::default()
        };
        game_settings::set(&db, &settings).unwrap();
        apply(&paths, &db, "keen4-1").unwrap();

        merge_pending(&paths, &db, "keen4-1").unwrap();
        assert_eq!(fs::read(&file).unwrap(), original);
    }
}

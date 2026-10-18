//! Play profiles: an install, optionally plus a mod, with its own persistent
//! state directory (PLAN.md §4).

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, Row};
use serde::Serialize;
use specta::Type;

use crate::catalog::{self, Game, glob_match};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::mods;
use crate::overlay_view;
use crate::recipe::Recipe;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    /// `None` for a mod waiting for its game.
    pub install_id: Option<String>,
    pub game_id: String,
    pub version_id: Option<String>,
    pub mod_id: Option<String>,
    pub label: String,
    pub session_count: u32,
    /// Total play time of finished sessions.
    pub play_seconds: u32,
    /// Unix epoch seconds.
    pub last_played_at: Option<u32>,
    /// Unix epoch seconds.
    pub added_at: u32,
    pub save_count: u32,
    pub running: bool,
    /// The game a mod waits for, e.g. `Commander Keen 4 (v1.4 EGA)`.
    pub missing_game: Option<String>,
}

/// Where a profile's files live on disk.
#[derive(Debug, Clone)]
pub struct Layers {
    /// What DOSBox mounts as C: below the overlay. For a mod profile, the
    /// install's base files, which still need the mod composed on top;
    /// `None` while a mod waits for its game.
    pub lower: Option<PathBuf>,
    /// The overlay: the profile's persistent `state/` directory.
    pub upper: PathBuf,
    pub mod_layer: Option<ModLayer>,
}

impl Layers {
    /// The lower layer as far as files the game writes are concerned: for
    /// mods, the mod's own files stand in for the composed layer, which may
    /// not exist yet; base files never hold saves (§4.4).
    pub fn view_lower(&self) -> &Path {
        match &self.mod_layer {
            Some(m) => &m.files,
            None => self
                .lower
                .as_deref()
                .expect("game profiles have an install"),
        }
    }

    /// The folders DOS sees in the drive root, top first: the mod's files
    /// over the base game's, as far as there are any.
    pub fn sources(&self) -> Vec<&Path> {
        self.mod_layer
            .as_ref()
            .map(|m| m.files.as_path())
            .into_iter()
            .chain(self.lower.as_deref())
            .collect()
    }
}

/// The mod of a mod profile.
#[derive(Debug, Clone)]
pub struct ModLayer {
    pub files: PathBuf,
    pub recipe: Recipe,
}

const SELECT: &str = "
    SELECT p.id, p.install_id, COALESCE(i.game_id, m.game_id), i.version_id, p.mod_id,
           COALESCE(m.name, i.label), s.session_count, s.play_seconds, s.last_played_at,
           p.state_path, i.path, m.path, m.recipe_json, p.created_at, m.requires_json
    FROM profiles p
    LEFT JOIN installs i ON i.id = p.install_id
    LEFT JOIN mods m ON m.id = p.mod_id
    JOIN profile_stats s ON s.profile_id = p.id";

struct ProfileRow {
    profile: Profile,
    layers: Layers,
}

fn from_row(r: &Row<'_>) -> rusqlite::Result<ProfileRow> {
    let install_path: Option<String> = r.get(10)?;
    let game_id: String = r.get(2)?;
    let requires: Option<String> = r.get(14)?;
    let missing_game = match (&install_path, catalog::game(&game_id)) {
        (Some(_), _) => None,
        (None, Some(game)) => Some(mods::needed_game(
            game,
            mods::parse_requires(requires.as_deref()).as_deref(),
        )),
        (None, None) => Some(game_id.clone()),
    };
    let state_path: String = r.get(9)?;
    let mod_path: Option<String> = r.get(11)?;
    let recipe: Option<String> = r.get(12)?;
    let mod_layer = mod_path.zip(recipe).map(|(path, recipe)| ModLayer {
        files: Path::new(&path).join("files"),
        // Written by `mods::install`; an unreadable one falls back to the episode.
        recipe: serde_json::from_str(&recipe).unwrap_or(Recipe::Episode),
    });
    Ok(ProfileRow {
        profile: Profile {
            id: r.get(0)?,
            install_id: r.get(1)?,
            game_id,
            version_id: r.get(3)?,
            mod_id: r.get(4)?,
            label: r.get(5)?,
            session_count: r.get(6)?,
            play_seconds: r.get(7)?,
            last_played_at: r.get(8)?,
            added_at: r.get(13)?,
            save_count: 0,
            running: false,
            missing_game,
        },
        layers: Layers {
            lower: install_path.map(|p| Path::new(&p).join("base")),
            upper: PathBuf::from(state_path),
            mod_layer,
        },
    })
}

/// All profiles, most recently played first. `is_running` marks the ones that
/// have a DOSBox process right now.
pub fn list(db: &Db, is_running: impl Fn(&str) -> bool) -> Result<Vec<Profile>> {
    let rows: Vec<ProfileRow> = {
        let conn = db.conn();
        let mut stmt = conn.prepare(&format!(
            "{SELECT} ORDER BY s.last_played_at IS NULL, s.last_played_at DESC, p.created_at, p.id"
        ))?;
        stmt.query_map([], from_row)?
            .collect::<rusqlite::Result<_>>()?
    };
    rows.into_iter()
        .map(|row| {
            let mut p = row.profile;
            p.running = is_running(&p.id);
            if let Some(game) = catalog::game(&p.game_id) {
                p.save_count = count_saves(game, &row.layers).unwrap_or_else(|e| {
                    tracing::warn!("could not scan saves of {}: {e}", p.id);
                    0
                });
            }
            Ok(p)
        })
        .collect()
}

pub fn get(db: &Db, id: &str) -> Result<(Profile, Layers)> {
    let row = db
        .conn()
        .query_row(&format!("{SELECT} WHERE p.id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("profile {id}")))?;
    Ok((row.profile, row.layers))
}

/// Saves as DOS sees them: through the overlay, in the drive root.
fn count_saves(game: &Game, layers: &Layers) -> Result<u32> {
    Ok(overlay_view::list(layers.view_lower(), &layers.upper)?
        .iter()
        .filter(|e| !e.path.contains('/') && game.saves.iter().any(|p| glob_match(p, &e.path)))
        .count() as u32)
}

/// The program to start inside `dir`, spelled as on disk: the version's own
/// start program if it has one, else the episode executable. Catalog order
/// decides when several exist (e.g. EGA before CGA).
pub fn find_executable(game: &Game, version_id: Option<&str>, dir: &Path) -> Result<String> {
    let names: Vec<String> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let start = version_id
        .and_then(|id| game.versions.iter().find(|v| v.id == id))
        .and_then(|v| v.start.as_ref());
    start
        .into_iter()
        .chain(&game.executables)
        .find_map(|exe| names.iter().find(|n| n.eq_ignore_ascii_case(exe)))
        .cloned()
        .ok_or_else(|| Error::NotFound(format!("{} executable in {}", game.id, dir.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library;
    use crate::paths::AppPaths;
    use crate::source::testutil::*;

    #[test]
    fn lists_imported_profile_with_stats_and_saves() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        let db = Db::open_in_memory().unwrap();
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        library::import(&paths, &db, &src).unwrap();

        db.conn()
            .execute(
                "INSERT INTO play_sessions (profile_id, started_at, ended_at) VALUES ('keen4-1', 10, 70)",
                [],
            )
            .unwrap();
        // A second save, and a deleted one that must not count.
        let state = paths.profiles.join("keen4-1/state");
        fs::write(state.join("SAVEGAM1.CK4"), b"s").unwrap();
        fs::write(state.join("DBOVERLAY_DEL_SAVEGAM1.CK4"), b"empty").unwrap();
        fs::write(state.join("savegam2.ck4"), b"s").unwrap();

        let list = list(&db, |id| id == "keen4-1").unwrap();
        assert_eq!(list.len(), 1);
        let p = &list[0];
        assert_eq!(
            (
                p.id.as_str(),
                p.game_id.as_str(),
                p.play_seconds,
                p.last_played_at
            ),
            ("keen4-1", "keen4", 60, Some(10))
        );
        assert_eq!(p.save_count, 3);
        assert!(p.running);

        let (_, layers) = get(&db, "keen4-1").unwrap();
        assert_eq!(layers.lower, Some(paths.library.join("keen4-1/base")));
        assert_eq!(layers.upper, state);
        assert!(matches!(get(&db, "nope"), Err(Error::NotFound(_))));
    }

    #[test]
    fn finds_executable_case_insensitively_in_catalog_order() {
        let tmp = tempfile::tempdir().unwrap();
        let k4 = catalog::game("keen4").unwrap();
        write_folder(tmp.path(), &[("keen4c.exe", b""), ("KEEN4E.EXE", b"")]);
        assert_eq!(find_executable(k4, None, tmp.path()).unwrap(), "KEEN4E.EXE");
        fs::remove_file(tmp.path().join("KEEN4E.EXE")).unwrap();
        assert_eq!(find_executable(k4, None, tmp.path()).unwrap(), "keen4c.exe");
        fs::remove_file(tmp.path().join("keen4c.exe")).unwrap();
        assert!(find_executable(k4, None, tmp.path()).is_err());
    }

    #[test]
    fn prefers_the_versions_start_program() {
        let tmp = tempfile::tempdir().unwrap();
        let kd = catalog::game("kdreams").unwrap();
        write_folder(tmp.path(), &[("KDREAMS.EXE", b""), ("start.exe", b"")]);
        assert_eq!(
            find_executable(kd, Some("kdreams-1.13"), tmp.path()).unwrap(),
            "start.exe"
        );
        assert_eq!(
            find_executable(kd, None, tmp.path()).unwrap(),
            "KDREAMS.EXE"
        );
        fs::remove_file(tmp.path().join("start.exe")).unwrap();
        assert_eq!(
            find_executable(kd, Some("kdreams-1.13"), tmp.path()).unwrap(),
            "KDREAMS.EXE"
        );
    }
}

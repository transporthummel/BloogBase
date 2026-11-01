//! Which copy of its game a mod plays on (one rule with `bestFit` in
//! `src/catalog.ts`), and moving mods when copies come and go.

use std::fs;

use rusqlite::{OptionalExtension, params};

use crate::catalog::{self, Game, GameVersion};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::library::{self, Install};
use crate::paths::AppPaths;
use crate::recipe::{self, Recipe};

use super::analyze::version_ids;

/// An index entry's `requires` as stored: empty means any version.
pub(super) fn requires(versions: &[String]) -> Option<Vec<String>> {
    (!versions.is_empty()).then(|| versions.to_vec())
}

/// Whether a mod that plays on the `requires` versions (`None`: any) can be
/// played on `install`. Like [`check_install`], unknown versions get the
/// benefit of the doubt.
pub fn fits(game: &Game, requires: Option<&[String]>, install: &Install) -> bool {
    install.game_id == game.id
        && match (requires, &install.version_id) {
            (None, _) | (_, None) => true,
            (Some(r), Some(v)) => fits_version(game, r, v),
        }
}

fn version<'a>(game: &'a Game, version_id: &str) -> Option<&'a GameVersion> {
    game.versions.iter().find(|v| v.id == version_id)
}

/// `version_id` is one of `requires`, or a build of one.
fn fits_version(game: &Game, requires: &[String], version_id: &str) -> bool {
    requires.iter().any(|r| r == version_id)
        || version(game, version_id)
            .and_then(|v| v.original.as_ref())
            .is_some_and(|o| requires.contains(o))
}

/// Lower is better, among installs a mod [`fits`]: a release, a
/// third-party build, an unknown version; then the game's preferred
/// versions. `bestFit` in catalog.ts mirrors it.
pub(super) fn fit_rank(game: &Game, install: &Install) -> (u8, usize) {
    match &install.version_id {
        Some(v) => (
            u8::from(version(game, v).is_some_and(GameVersion::modified)),
            game.preference(v),
        ),
        None => (2, 0),
    }
}

/// The install a mod fits best, if any; on a tie the one added first.
pub(super) fn best_install(
    db: &Db,
    game: &Game,
    requires: Option<&[String]>,
) -> Result<Option<Install>> {
    Ok(library::list(db)?
        .into_iter()
        .filter(|i| fits(game, requires, i))
        .min_by_key(|i| fit_rank(game, i)))
}

/// e.g. `Commander Keen 4: Secret of the Oracle (v1.4 EGA)`.
pub fn needed_game(game: &Game, requires: Option<&[String]>) -> String {
    let labels: Vec<&str> = game
        .versions
        .iter()
        .filter(|v| requires.is_some_and(|r| r.contains(&v.id)))
        .map(|v| v.label.as_str())
        .collect();
    if labels.is_empty() {
        game.title.clone()
    } else {
        format!("{} ({})", game.title, labels.join(", "))
    }
}

/// Reads `mods.requires_json`.
pub fn parse_requires(json: Option<&str>) -> Option<Vec<String>> {
    json.and_then(|j| serde_json::from_str(j).ok())
}

/// Puts mods without a game, or with `except` gone, on the install they
/// fit best; those without one wait for it. Returns the profiles that moved.
pub fn rehome(db: &Db, except: Option<&str>) -> Result<Vec<String>> {
    let waiting: Vec<(String, String, Option<String>)> = {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT p.id, m.game_id, m.requires_json
             FROM profiles p JOIN mods m ON m.id = p.mod_id
             WHERE p.install_id IS NULL OR p.install_id IS ?1",
        )?;
        stmt.query_map([except], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let installs: Vec<Install> = library::list(db)?
        .into_iter()
        .filter(|i| Some(i.id.as_str()) != except)
        .collect();
    let mut moved = Vec::new();
    for (profile_id, game_id, requires) in waiting {
        let Some(game) = catalog::game(&game_id) else {
            continue;
        };
        let requires = parse_requires(requires.as_deref());
        let target = installs
            .iter()
            .filter(|i| fits(game, requires.as_deref(), i))
            .min_by_key(|i| fit_rank(game, i));
        let target = target.map(|i| i.id.as_str());
        let changed = db.conn().execute(
            "UPDATE profiles SET install_id = ?2, composed_hash = NULL
             WHERE id = ?1 AND install_id IS NOT ?2",
            params![profile_id, target],
        )?;
        if changed > 0 {
            tracing::info!("mod profile {profile_id} now plays on {target:?}");
            moved.push(profile_id);
        }
    }
    Ok(moved)
}

/// Gives mods whose patch names versions the catalog didn't know the
/// catalog versions they now match, e.g. after an update taught the catalog
/// what CKPatch calls a release. Those that turn out to name none, as
/// earlier `%version all` did, fit every version.
pub fn identify_requirements(paths: &AppPaths, db: &Db) -> Result<()> {
    let unknown: Vec<(String, String, String)> = {
        let conn = db.conn();
        let mut stmt =
            conn.prepare("SELECT id, game_id, recipe_json FROM mods WHERE requires_json = '[]'")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut found = false;
    for (mod_id, game_id, recipe) in unknown {
        let Some(game) = catalog::game(&game_id) else {
            continue;
        };
        let Some(text) = serde_json::from_str::<Recipe>(&recipe)
            .ok()
            .and_then(|r| r.patch_file().map(str::to_string))
            .and_then(|p| fs::read(paths.mods.join(&mod_id).join("files").join(p)).ok())
        else {
            continue;
        };
        let versions = recipe::patch_versions(&String::from_utf8_lossy(&text));
        let ids = version_ids(game, &versions);
        if ids.is_empty() && !versions.is_empty() {
            continue;
        }
        let requires =
            (!versions.is_empty()).then(|| serde_json::to_string(&ids).expect("strings serialize"));
        db.conn().execute(
            "UPDATE mods SET requires_json = ?2 WHERE id = ?1",
            params![mod_id, requires],
        )?;
        tracing::info!("{mod_id} plays on {ids:?}");
        found = true;
    }
    if found {
        rehome(db, None)?;
    }
    Ok(())
}

/// The installs `mod_id` can be played on, in library order.
pub fn fitting_installs(db: &Db, mod_id: &str) -> Result<Vec<Install>> {
    let (game_id, requires): (String, Option<String>) = db
        .conn()
        .query_row(
            "SELECT game_id, requires_json FROM mods WHERE id = ?1",
            [mod_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))?;
    let Some(game) = catalog::game(&game_id) else {
        return Ok(Vec::new());
    };
    let requires = parse_requires(requires.as_deref());
    Ok(library::list(db)?
        .into_iter()
        .filter(|i| fits(game, requires.as_deref(), i))
        .collect())
}

/// Plays `mod_id` on `install_id` from now on. Saves stay with the mod;
/// the composed layer is rebuilt on the next launch.
pub fn set_install(db: &Db, mod_id: &str, install_id: &str) -> Result<()> {
    if !fitting_installs(db, mod_id)?
        .iter()
        .any(|i| i.id == install_id)
    {
        return Err(Error::IncompatibleInstall(format!(
            "{mod_id} can't be played on {install_id}"
        )));
    }
    db.conn().execute(
        "UPDATE profiles SET install_id = ?2, composed_hash = NULL
         WHERE mod_id = ?1 AND install_id IS NOT ?2",
        params![mod_id, install_id],
    )?;
    tracing::info!("mod {mod_id} now plays on {install_id}");
    Ok(())
}

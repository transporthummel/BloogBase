//! Copies a mod's own launcher leaves in `state/` (PLAN.md §17, question 9).
//! Keen Q's `KEENQ.COM` renames `GEEL.PAT`, `KEEN2.EXE` and others on every
//! run. DOS writes those renames into the overlay like anything else, and
//! from then on the copies hide the composed layer below: a mod update or
//! one-button firing never reaches the profile. So when that layer changes,
//! the files the launcher names go, after a backup, and the launcher starts
//! over as after a fresh install.
//!
//! What goes is decided by where a file comes from, not by its content: a
//! setting the player reset to the mod's default looks just like a copy.
//! Saves and the game's other state files always stay.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::backups::{self, Contents, Reason};
use crate::catalog::{Game, glob_match};
use crate::db::Db;
use crate::error::Result;
use crate::overlay_view;
use crate::paths::AppPaths;
use crate::recipe::{self, Recipe, has_ext};

/// Removes from `upper`, the profile's `state/`, what the launcher in
/// `composed` left there, after backing `state/` up. Returns the names
/// removed.
pub fn drop(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    game: &Game,
    recipe: &Recipe,
    composed: &Path,
    upper: &Path,
) -> Result<Vec<String>> {
    let Recipe::Program { program, .. } = recipe else {
        return Ok(Vec::new());
    };
    let Some(path) = find(composed, program)? else {
        return Ok(Vec::new());
    };
    let names = root_files(upper)?;
    let gone = stale(game, &fs::read(path)?, &names);
    if gone.is_empty() {
        return Ok(gone);
    }
    backups::create(
        paths,
        db,
        profile_id,
        Contents::State,
        Reason::LauncherReset,
    )?;
    for name in &gone {
        fs::remove_file(upper.join(name))?;
    }
    tracing::info!("dropped launcher copies of {profile_id}: {gone:?}");
    Ok(gone)
}

/// The files among `names` (in `state/`) that come from the launcher
/// `program`: those it names, deletion markers for those, and the `.OLZ`
/// backups UNLZEXE keeps of the executables it unpacks.
fn stale(game: &Game, program: &[u8], names: &[String]) -> Vec<String> {
    let named = |name: &str| recipe::names_file(program, name);
    names
        .iter()
        .filter(|n| {
            let file = overlay_view::deleted_name(n).unwrap_or(n);
            if overlay_view::is_marker(file) || kept(game, file) {
                return false;
            }
            named(file)
                || (has_ext(file, "OLZ")
                    && ["EXE", "COM"].iter().any(|e| {
                        let stem = &file[..file.len() - 4];
                        named(&format!("{stem}.{e}"))
                    }))
        })
        .cloned()
        .collect()
}

/// Saves and other state files of the game stay, under any extension, as
/// mods give the episode's files their own (Keen Q's `CTLPANEL.CKQ`).
fn kept(game: &Game, name: &str) -> bool {
    let stem = |s: &str| s.rsplit_once('.').map_or(s, |(s, _)| s).to_string();
    game.saves
        .iter()
        .chain(&game.state_files)
        .any(|p| glob_match(&format!("{}.*", stem(p)), name) || glob_match(p, name))
}

/// The launcher in `dir`, spelled as on disk; DOS adds `.COM` or `.EXE`.
fn find(dir: &Path, program: &str) -> Result<Option<PathBuf>> {
    let program = program.trim_start_matches(".\\").trim_start_matches("C:\\");
    let wanted = [
        program.to_string(),
        format!("{program}.COM"),
        format!("{program}.EXE"),
    ];
    Ok(root_files(dir)?
        .into_iter()
        .find(|n| wanted.iter().any(|w| w.eq_ignore_ascii_case(n)))
        .map(|n| dir.join(n)))
}

fn root_files(dir: &Path) -> Result<Vec<String>> {
    let iter = match fs::read_dir(dir) {
        Ok(iter) => iter,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for entry in iter {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            out.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    fn keen2() -> &'static Game {
        catalog::game("keen2").unwrap()
    }

    /// The commands of Keen Q's `KEENQ.COM`, a batch file compiled with
    /// BAT2EXEC, as they show among its code.
    const KEENQ: &[u8] = b"\0/C RENAME GEEL.PAT GEEL2.PAT\0/C RENAME GEEL3.PAT GEEL.PAT\0\
        /C RENAME GEEL2.PAT GEEL3.PAT\0 /C RENAME LEVEL80.CKQ LEVELY.CKQ\0\
        /C RENAME LEVELX.CKQ LEVEL80.CKQ\0/C RENAME LEVELY.CKQ LEVELX.CKQ\0\
        !/C RENAME UNLZEXE.CKQ UNLZEXE.EXE\0keen2.exe\0UNLZEXE\0 KEEN2.EXE\0\
        KEEN2.CKQ\0/C RENAME KEEN2.CKQ KEEN2.EXE\0CK2PATCH.CKQ\0\
        #/C RENAME CK2PATCH.CKQ CK2PATCH.EXE\0CK2patch.exe\0 geel.pat\0";

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn files_the_launcher_names_go_saves_and_settings_stay() {
        // Keen Q's state/ after its first run, plus a save.
        let state = names(&[
            "CK2PATCH.CKQ",
            "CTLPANEL.CKQ",
            "DBOVERLAY_DEL_CK2PATCH.EXE",
            "DBOVERLAY_DEL_KEEN2.EXE",
            "GEEL.PAT",
            "GEEL3.PAT",
            "KEEN2.CKQ",
            "KEEN2.OLZ",
            "LEVEL80.CKQ",
            "LEVELX.CKQ",
            "SAVED1.CKQ",
            "SCORES.CKQ",
            "UNLZEXE.CKQ",
        ]);
        assert_eq!(
            stale(keen2(), KEENQ, &state),
            names(&[
                "CK2PATCH.CKQ",
                "DBOVERLAY_DEL_CK2PATCH.EXE",
                "DBOVERLAY_DEL_KEEN2.EXE",
                "GEEL.PAT",
                "GEEL3.PAT",
                "KEEN2.CKQ",
                "KEEN2.OLZ",
                "LEVEL80.CKQ",
                "LEVELX.CKQ",
                "UNLZEXE.CKQ",
            ])
        );
    }

    #[test]
    fn named_state_files_and_other_names_stay() {
        let program = b"COPY DEFAULT.CFG CTLPANEL.CKQ\0COPY XGEEL.PAT GEEL.PATCH\0";
        let state = names(&["CTLPANEL.CKQ", "GEEL.PAT", "OTHER.OLZ", "DBOVERLAY_RMD_SUB"]);
        assert!(stale(keen2(), program, &state).is_empty());
    }
}

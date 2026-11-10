//! Custom patches: patch file lines a player enters for a game copy, added
//! at launch to the game and to every mod played on it, after the mod's own
//! lines. Patch files get them in a `%version` section for the copy's
//! version; an episode executable run directly gets their `%patch` and
//! `%patchfile` writes.

use std::fs;
use std::path::Path;

use keen_graphics::{lzexe, pklite};
use rusqlite::OptionalExtension;
use serde::Serialize;
use specta::Type;

use crate::catalog;
use crate::catalog::Game;
use crate::db::Db;
use crate::error::Result;
use crate::patchfile::{Files, Target, apply_writes, number, targets, tokens, value, with_lines};
use crate::profiles;
use crate::recipe::Recipe;

/// Directives BloogBase writes itself around the player's lines.
const RESERVED: [&str; 3] = ["ext", "end", "version"];

/// The custom patches of a game copy, and the version they're for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Custom {
    /// What patch files call the copy's version in `%version`.
    pub version: String,
    pub text: String,
}

/// How a profile takes the custom patches of its game copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CustomStatus {
    Applied,
    /// The mod runs a program of its own, not the episode's.
    OwnProgram,
    /// The episode's executable runs directly, and the lines have directives
    /// only the patcher understands.
    PatcherOnly,
    /// The episode's executable runs directly and is packed in a way
    /// BloogBase can't unpack.
    Packed,
}

/// A line the patcher would stop at, or that BloogBase writes itself. The
/// patcher reads every line, also in sections for other versions, so one
/// bad value would stop every mod on the copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BadLine {
    /// 1-based.
    pub line: u32,
    pub problem: Problem,
    /// The word in question as written, e.g. `%end` or `$1G`.
    pub word: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Problem {
    /// `%ext`, `%end` or `%version`.
    Reserved,
    /// An offset or value that isn't a number the patcher takes.
    Value,
}

/// The custom patches as a profile page shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProfilePatches {
    /// A game profile: its copy's patches, which it can edit, and how the
    /// game takes them, if it has some.
    Game {
        text: String,
        status: Option<CustomStatus>,
    },
    /// A game profile whose version patches aren't made for.
    Unpatchable,
    /// A mod on a copy with custom patches: how it takes them, and the
    /// game profile they're set in.
    Mod {
        status: CustomStatus,
        game_profile_id: String,
        game_label: String,
    },
}

/// What `profile_id`'s page shows; `None` for a mod on a copy without
/// custom patches, or without a copy.
pub fn profile_view(db: &Db, profile_id: &str) -> Result<Option<ProfilePatches>> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let (Some(install), Some(game)) = (&profile.install_id, catalog::game(&profile.game_id)) else {
        return Ok(None);
    };
    let Some(mod_layer) = &layers.mod_layer else {
        let patchable = profile
            .version_id
            .as_deref()
            .is_some_and(|v| patch_version(game, v).is_some());
        return Ok(Some(if patchable {
            let text = text(db, install)?.unwrap_or_default();
            let status = (!text.is_empty())
                .then(|| status(game, &Recipe::Episode, &layers.sources(), &text));
            ProfilePatches::Game { text, status }
        } else {
            ProfilePatches::Unpatchable
        }));
    };
    let Some(custom) = for_install(db, game, install, profile.version_id.as_deref())? else {
        return Ok(None);
    };
    let game_profile_id: Option<String> = db
        .conn()
        .query_row(
            "SELECT id FROM profiles WHERE install_id = ?1 AND mod_id IS NULL",
            [install],
            |r| r.get(0),
        )
        .optional()?;
    let Some(game_profile_id) = game_profile_id else {
        return Ok(None);
    };
    let game_label = profiles::get(db, &game_profile_id)?.0.label;
    Ok(Some(ProfilePatches::Mod {
        status: status(game, &mod_layer.recipe, &layers.sources(), &custom.text),
        game_profile_id,
        game_label,
    }))
}

/// Stores `text` as the custom patches of the copy below `profile_id`, a
/// game profile.
pub fn set_for_profile(db: &Db, profile_id: &str, text: &str) -> Result<Option<BadLine>> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    match (&profile.install_id, &layers.mod_layer) {
        (Some(install), None) => set(db, install, text),
        _ => Err(crate::error::Error::NotFound(format!(
            "game copy of {profile_id}"
        ))),
    }
}

/// The text of `install_id`'s custom patches, if it has any.
pub fn text(db: &Db, install_id: &str) -> Result<Option<String>> {
    Ok(db
        .conn()
        .query_row(
            "SELECT custom_patches FROM installs WHERE id = ?1",
            [install_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

/// Stores `text` as `install_id`'s custom patches, none if it's blank;
/// refused with the first bad line.
pub fn set(db: &Db, install_id: &str, text: &str) -> Result<Option<BadLine>> {
    if let Some(bad) = check(text) {
        return Ok(Some(bad));
    }
    let text = (!text.trim().is_empty()).then_some(text);
    db.conn().execute(
        "UPDATE installs SET custom_patches = ?2 WHERE id = ?1",
        rusqlite::params![install_id, text],
    )?;
    Ok(None)
}

/// The custom patches of `install_id`, a copy of `version_id`, where it has
/// some and its version can be patched.
pub fn for_install(
    db: &Db,
    game: &Game,
    install_id: &str,
    version_id: Option<&str>,
) -> Result<Option<Custom>> {
    let Some(version) = version_id.and_then(|v| patch_version(game, v)) else {
        return Ok(None);
    };
    Ok(text(db, install_id)?.map(|text| Custom {
        version: version.to_owned(),
        text,
    }))
}

/// What patch files call `version_id` in `%version`: its own name, or its
/// original's for a build with the same game files.
pub fn patch_version<'a>(game: &'a Game, version_id: &str) -> Option<&'a str> {
    let version = game.versions.iter().find(|v| v.id == version_id)?;
    version.patch_version.as_deref().or_else(|| {
        let original = version.original.as_deref().filter(|_| version.repackaged)?;
        game.versions
            .iter()
            .find(|v| v.id == original)?
            .patch_version
            .as_deref()
    })
}

/// The first line of `text` the patcher would stop at or BloogBase writes
/// itself: reserved directives, and offsets and values of `%patch` and
/// `%patchfile` lines (values may go on over the following lines).
pub fn check(text: &str) -> Option<BadLine> {
    let mut patch = false;
    for (i, line) in text.lines().enumerate() {
        let bad = |problem, word: &str| {
            Some(BadLine {
                line: i as u32 + 1,
                problem,
                word: word.to_owned(),
            })
        };
        let words = tokens(line.split('#').next().unwrap_or(""));
        let Some(first) = words.first() else { continue };
        let values = match first.strip_prefix('%') {
            Some(name) => {
                let name = name.to_ascii_lowercase();
                if RESERVED.contains(&name.as_str()) {
                    return bad(Problem::Reserved, first);
                }
                patch = name == "patch";
                if patch || name == "patchfile" {
                    match words.get(1) {
                        Some(offset) if number(offset).is_some() => {}
                        offset => return bad(Problem::Value, offset.unwrap_or(first)),
                    }
                }
                if patch { &words[2..] } else { &[][..] }
            }
            None if patch => &words[..],
            None => continue,
        };
        if let Some(word) = values.iter().find(|w| value(w).is_none()) {
            return bad(Problem::Value, word);
        }
    }
    None
}

/// The directive a line starts with, e.g. `%patch`.
fn directive(line: &str) -> Option<&str> {
    let code = line.split('#').next().unwrap_or("");
    code.split_whitespace()
        .next()
        .filter(|w| w.starts_with('%'))
}

/// Whether the lines only write bytes, so BloogBase can apply them to an
/// executable itself.
fn writes_only(text: &str) -> bool {
    text.lines()
        .filter_map(directive)
        .all(|d| d.eq_ignore_ascii_case("%patch") || d.eq_ignore_ascii_case("%patchfile"))
}

/// Whether `target` is the episode's own executable.
fn episode_exe(game: &Game, exe: &Path) -> bool {
    exe.file_name().is_some_and(|n| {
        game.executables
            .iter()
            .any(|e| n.to_string_lossy().eq_ignore_ascii_case(e))
    })
}

fn judge(game: &Game, target: &Target, text: &str) -> CustomStatus {
    match target {
        Target::Patch(_) => CustomStatus::Applied,
        Target::Exe(exe) if !episode_exe(game, exe) => CustomStatus::OwnProgram,
        Target::Exe(_) if !writes_only(text) => CustomStatus::PatcherOnly,
        Target::Exe(exe) => match fs::read(exe) {
            Ok(data) if !pklite::is_pklite(&data) && lzexe::unpack_exe(&data).is_ok() => {
                CustomStatus::Applied
            }
            _ => CustomStatus::Packed,
        },
    }
}

/// How a profile running `recipe` takes custom patches with `text`; `dirs`
/// are its files, topmost first. Applied only if everything it runs takes
/// them, else the first reason it doesn't.
pub fn status(game: &Game, recipe: &Recipe, dirs: &[&Path], text: &str) -> CustomStatus {
    let files = Files(dirs);
    let targets = targets(game, recipe, &files);
    if targets.is_empty() {
        return CustomStatus::OwnProgram;
    }
    targets
        .iter()
        .map(|t| judge(game, t, text))
        .find(|s| *s != CustomStatus::Applied)
        .unwrap_or(CustomStatus::Applied)
}

/// Applies `custom` to the files in `dir`, a disposable copy, if all that
/// the profile runs takes it: each patch file gets the lines, an episode
/// executable run directly is rewritten unpacked with their writes.
pub fn apply(game: &Game, recipe: &Recipe, dir: &Path, custom: &Custom) -> Result<CustomStatus> {
    let dirs = [dir];
    let status = status(game, recipe, &dirs, &custom.text);
    if status != CustomStatus::Applied {
        return Ok(status);
    }
    let files = Files(&dirs);
    let lines = format!("\n# Custom patches, added by BloogBase\n{}", custom.text);
    for target in targets(game, recipe, &files) {
        match target {
            Target::Patch(path) => {
                let text = fs::read(&path)?;
                fs::write(&path, with_lines(&text, &lines, &custom.version, true))?;
            }
            Target::Exe(path) => {
                let Ok(mut exe) = lzexe::unpack_exe(&fs::read(&path)?) else {
                    continue;
                };
                let header = usize::from(u16::from_le_bytes([exe[8], exe[9]])) * 16;
                apply_writes(&mut exe[header..], &custom.text, &files, &custom.version);
                fs::write(&path, exe)?;
            }
        }
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use crate::source::testutil::write_folder;

    fn keen4() -> &'static Game {
        catalog::game("keen4").unwrap()
    }

    fn custom(text: &str) -> Custom {
        Custom {
            version: "1.4".into(),
            text: text.into(),
        }
    }

    /// An unpacked executable with a 32-byte header and 16 zero bytes.
    fn exe() -> Vec<u8> {
        let mut exe = vec![0u8; 0x30];
        exe[..2].copy_from_slice(b"MZ");
        exe[8] = 2;
        exe
    }

    fn patch(name: &str) -> Recipe {
        Recipe::Patch {
            patcher: "CK4PATCH.EXE".into(),
            patch: name.into(),
            args: Vec::new(),
        }
    }

    #[test]
    fn bad_lines_are_refused_with_their_line() {
        let bad = |line, problem, word: &str| {
            Some(BadLine {
                line,
                problem,
                word: word.into(),
            })
        };
        assert_eq!(
            check("%patch $10 $01 $0203W \"a b\"\n  $04 # $ZZ\n# %end\n%egahead X\n"),
            None
        );
        assert_eq!(
            check("%patch $10 $01\n\n  %End  # done\n%ext ck4"),
            bad(3, Problem::Reserved, "%End")
        );
        assert_eq!(check("%VERSION 1.4"), bad(1, Problem::Reserved, "%VERSION"));
        assert_eq!(check("%patch $10 $1G"), bad(1, Problem::Value, "$1G"));
        // CKPatch 0.10 wants a W for a word.
        assert_eq!(
            check("%patch $10\n $01 $100"),
            bad(2, Problem::Value, "$100")
        );
        assert_eq!(check("%patch"), bad(1, Problem::Value, "%patch"));
        assert_eq!(check("%patchfile x.bin"), bad(1, Problem::Value, "x.bin"));
        assert_eq!(check("%patchfile $10 \"x.bin\""), None);
    }

    #[test]
    fn stored_per_install() {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO installs (id, game_id, version_id, label, source, path, added_at)
                 VALUES ('keen4-1', 'keen4', 'keen4-1.4-ega', 'Keen 4', 'folder', 'x', 0)",
                [],
            )
            .unwrap();
        assert_eq!(text(&db, "keen4-1").unwrap(), None);
        assert_eq!(set(&db, "keen4-1", "%patch $10 $01\n").unwrap(), None);
        let stored = for_install(&db, keen4(), "keen4-1", Some("keen4-1.4-ega")).unwrap();
        assert_eq!(stored, Some(custom("%patch $10 $01\n")));
        // Not for a version patches aren't made for.
        assert_eq!(for_install(&db, keen4(), "keen4-1", None).unwrap(), None);

        assert!(set(&db, "keen4-1", "%end").unwrap().is_some());
        assert_eq!(
            text(&db, "keen4-1").unwrap().as_deref(),
            Some("%patch $10 $01\n")
        );
        set(&db, "keen4-1", " \n").unwrap();
        assert_eq!(text(&db, "keen4-1").unwrap(), None);
    }

    #[test]
    fn repackaged_builds_take_their_originals_version() {
        for game in catalog::games() {
            for v in &game.versions {
                let expected = v.patch_version.as_deref().or_else(|| {
                    let o = v.original.as_deref().filter(|_| v.repackaged)?;
                    game.versions
                        .iter()
                        .find(|x| x.id == o)?
                        .patch_version
                        .as_deref()
                });
                assert_eq!(patch_version(game, &v.id), expected, "{}", v.id);
            }
        }
        assert_eq!(patch_version(keen4(), "keen4-1.4-ega"), Some("1.4"));
        assert!(
            catalog::games()
                .iter()
                .flat_map(|g| g.versions.iter().map(move |v| (g, v)))
                .any(|(g, v)| v.repackaged && patch_version(g, &v.id).is_some()),
            "some repackaged build can be patched"
        );
    }

    #[test]
    fn patch_files_get_the_lines_in_their_own_section() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_folder(
            dir,
            &[
                ("KEEN4E.EXE", &exe()),
                ("MOD.PAT", b"%ext ck4\n%patch $10 $01\n%end\n"),
            ],
        );
        let status = apply(keen4(), &patch("MOD.PAT"), dir, &custom("%patch $20 $02")).unwrap();
        assert_eq!(status, CustomStatus::Applied);
        assert_eq!(
            fs::read_to_string(dir.join("MOD.PAT")).unwrap(),
            "%ext ck4\n%patch $10 $01\n\n%version 1.4\n\
             # Custom patches, added by BloogBase\n%patch $20 $02\n%end\n"
        );
        // The executable is the patcher's to change.
        assert_eq!(fs::read(dir.join("KEEN4E.EXE")).unwrap(), exe());
    }

    #[test]
    fn batch_files_get_them_in_each_patch_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_folder(
            dir,
            &[
                ("KEEN4E.EXE", &exe()),
                ("CK4PATCH.EXE", b"MZ"),
                ("A.PAT", b"%ext ck4\n%end\n"),
                ("B.PAT", b"%ext ck4\n%end\n"),
                ("PLAY.BAT", b"ck4patch a.pat\r\nck4patch b\r\n"),
            ],
        );
        let recipe = Recipe::Batch {
            file: "PLAY.BAT".into(),
        };
        let status = apply(keen4(), &recipe, dir, &custom("%egahead EGAHEAD.CK4")).unwrap();
        assert_eq!(status, CustomStatus::Applied);
        for name in ["A.PAT", "B.PAT"] {
            let text = fs::read_to_string(dir.join(name)).unwrap();
            assert!(text.contains("%version 1.4\n"), "{name}: {text}");
            assert!(
                text.ends_with("%egahead EGAHEAD.CK4\n%end\n"),
                "{name}: {text}"
            );
        }
    }

    #[test]
    fn the_plain_game_gets_the_writes() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_folder(dir, &[("KEEN4E.EXE", &exe()), ("DATA.BIN", b"xy")]);
        let text = "# keys\n%patch $1 $AB\n  $CD\n%patchfile $4 DATA.BIN\n";
        let status = apply(keen4(), &Recipe::Episode, dir, &custom(text)).unwrap();
        assert_eq!(status, CustomStatus::Applied);
        let patched = fs::read(dir.join("KEEN4E.EXE")).unwrap();
        assert_eq!(patched[0x20..0x26], [0, 0xAB, 0xCD, 0, b'x', b'y']);
        assert_eq!(patched[..0x20], exe()[..0x20]);

        // Directives only the patcher knows: nothing is applied.
        write_folder(dir, &[("KEEN4E.EXE", &exe())]);
        let status = apply(
            keen4(),
            &Recipe::Episode,
            dir,
            &custom("%patch $1 $AB\n%egahead EGAHEAD.CK4\n"),
        )
        .unwrap();
        assert_eq!(status, CustomStatus::PatcherOnly);
        assert_eq!(fs::read(dir.join("KEEN4E.EXE")).unwrap(), exe());
    }

    #[test]
    fn own_programs_are_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_folder(dir, &[("KEEN4E.EXE", &exe()), ("MYKEEN.EXE", &exe())]);
        let recipe = Recipe::Program {
            program: "MYKEEN.EXE".into(),
            args: Vec::new(),
        };
        let status = apply(keen4(), &recipe, dir, &custom("%patch $1 $AB")).unwrap();
        assert_eq!(status, CustomStatus::OwnProgram);
        assert_eq!(fs::read(dir.join("MYKEEN.EXE")).unwrap(), exe());
        assert_eq!(fs::read(dir.join("KEEN4E.EXE")).unwrap(), exe());
    }

    #[test]
    fn profile_pages_show_the_copys_patches() {
        use crate::paths::AppPaths;
        use crate::source::testutil::KEEN4;
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        let db = Db::open_in_memory().unwrap();
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        crate::library::import(&paths, &db, &src).unwrap();
        let modsrc = tmp.path().join("mod");
        write_folder(&modsrc, &[("MOD.PAT", b"%ext ck4\n%end\n")]);
        let m = crate::mods::install(
            &paths,
            &db,
            &crate::mods::InstallModRequest {
                path: modsrc.display().to_string(),
                name: "Patched".into(),
                recipe: patch("MOD.PAT"),
                install_id: Some("keen4-1".into()),
                keenwiki: None,
                extends: None,
                replace: None,
            },
        )
        .unwrap();
        let view = |id: &str| profile_view(&db, id).unwrap();

        // The fake files aren't a version the catalog knows.
        assert_eq!(view("keen4-1"), Some(ProfilePatches::Unpatchable));
        assert_eq!(view(&m.profile_id), None);
        db.conn()
            .execute("UPDATE installs SET version_id = 'keen4-1.4-ega'", [])
            .unwrap();
        assert_eq!(
            view("keen4-1"),
            Some(ProfilePatches::Game {
                text: String::new(),
                status: None,
            })
        );
        assert_eq!(view(&m.profile_id), None, "nothing set");

        assert_eq!(
            set_for_profile(&db, "keen4-1", "%patch $1 $01").unwrap(),
            None
        );
        assert!(set_for_profile(&db, &m.profile_id, "%patch $1 $01").is_err());
        // The fake KEEN4E.EXE isn't a program BloogBase can unpack.
        assert_eq!(
            view("keen4-1"),
            Some(ProfilePatches::Game {
                text: "%patch $1 $01".into(),
                status: Some(CustomStatus::Packed),
            })
        );
        assert_eq!(
            view(&m.profile_id),
            Some(ProfilePatches::Mod {
                status: CustomStatus::Applied,
                game_profile_id: "keen4-1".into(),
                game_label: profiles::get(&db, "keen4-1").unwrap().0.label,
            })
        );
    }

    #[test]
    fn status_looks_through_the_mod_to_the_install() {
        let tmp = tempfile::tempdir().unwrap();
        let (modf, base) = (tmp.path().join("mod"), tmp.path().join("base"));
        write_folder(&base, &[("KEEN4E.EXE", &exe())]);
        write_folder(&modf, &[("MOD.PAT", b"%ext ck4\n%end\n")]);
        let dirs = [modf.as_path(), base.as_path()];
        let text = "%egahead EGAHEAD.CK4";
        assert_eq!(
            status(keen4(), &patch("mod.pat"), &dirs, text),
            CustomStatus::Applied
        );
        assert_eq!(
            status(keen4(), &Recipe::Episode, &dirs, text),
            CustomStatus::PatcherOnly
        );
        assert_eq!(
            status(keen4(), &Recipe::Episode, &dirs, "%patch $1 $01"),
            CustomStatus::Applied
        );
    }
}

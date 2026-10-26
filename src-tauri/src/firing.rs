//! One-button firing for Keen 1-3: Space shoots instead of Ctrl and Alt
//! together, and Enter opens the status window instead of Space. The patch
//! is KeenWiki's "One button firing in Vorticons" for version 1.31.
//!
//! Many mods already patch the controls this way, so the patch is applied
//! only where the code still is as the episode shipped it. The executable is
//! read as DOS would run it: with the changes the mod's patch files make
//! when the game starts.

use std::fs;
use std::path::{Path, PathBuf};

use keen_graphics::lzexe;
use serde::Serialize;
use specta::Type;

use crate::catalog::Game;
use crate::db::Db;
use crate::error::Result;
use crate::recipe::{self, Recipe};
use crate::settings;

const SETTING_KEY: &str = "games.one_button_firing";

pub fn enabled(db: &Db) -> Result<bool> {
    Ok(settings::get(db, SETTING_KEY)?.is_some_and(|v| v == "1"))
}

pub fn set_enabled(db: &Db, on: bool) -> Result<()> {
    settings::set(db, SETTING_KEY, if on { "1" } else { "0" })
}

/// Where a game or mod stands with the patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Firing {
    /// Fires with Ctrl and Alt; the patch fits.
    Patchable,
    /// Already fires with Space.
    BuiltIn,
    /// The code isn't as the patch expects: another version, or a mod that
    /// changed it differently.
    Unavailable,
    /// The game's own executable is another version than 1.31, the one the
    /// patch is for.
    OtherVersion,
}

/// The patch's addresses in one episode, as offsets into the load image.
struct Episode {
    game: &'static str,
    /// Where the status window's key is checked.
    status: [u32; 2],
    /// Key state addresses.
    space: u16,
    enter: u16,
    /// The flags for the two fire buttons, Ctrl and Alt by default.
    fire: [u16; 2],
    /// Where Keen checks both buttons before shooting.
    shots: [u32; 6],
    /// A jump past one of these checks.
    skip: u32,
}

const EPISODES: [Episode; 3] = [
    Episode {
        game: "keen1",
        status: [0x1251, 0x7FA3],
        space: 0x5065,
        enter: 0x5048,
        fire: [0x7FCC, 0x7FCE],
        shots: [0x3B23, 0x3C4E, 0x3DB9, 0x3E6D, 0x3FFC, 0x40AA],
        skip: 0x3DCC,
    },
    Episode {
        game: "keen2",
        status: [0x120B, 0x83A2],
        space: 0x5041,
        enter: 0x5024,
        fire: [0x946C, 0x946E],
        shots: [0x6965, 0x6A90, 0x6BFB, 0x6CAF, 0x6E3E, 0x6EEC],
        skip: 0x6C0E,
    },
    Episode {
        game: "keen3",
        status: [0x11AF, 0x8D6E],
        space: 0x5291,
        enter: 0x5274,
        fire: [0x96BC, 0x96BE],
        shots: [0x71C9, 0x72FA, 0x7461, 0x7515, 0x76A1, 0x7750],
        skip: 0x7474,
    },
];

/// The jump after the first button check at each of the `shots`; the same
/// in all three episodes.
const SHOT_JUMPS: [[u8; 2]; 6] = [
    [0x74, 0x41],
    [0x74, 0x33],
    [0x74, 0x41],
    [0x75, 0x0D],
    [0x74, 0x33],
    [0x74, 0x33],
];
const SKIP_JUMP: [u8; 2] = [0x75, 0x33];

/// Bytes the patch changes: as shipped, and patched.
struct Site {
    offset: usize,
    original: Vec<u8>,
    patched: Vec<u8>,
}

impl Episode {
    fn sites(&self) -> Vec<Site> {
        let le = |w: u16| w.to_le_bytes();
        let mut sites: Vec<Site> = self
            .status
            .iter()
            .map(|&offset| Site {
                offset: offset as usize,
                original: le(self.space).to_vec(),
                patched: le(self.enter).to_vec(),
            })
            .collect();
        for (&offset, jump) in self.shots.iter().zip(SHOT_JUMPS) {
            // `cmp word [fire1], 0; j.. ; cmp word [fire2], 0` becomes
            // `cmp byte [space], 0` after the jump is gone.
            let [a, b] = self.fire.map(le);
            let mut original = vec![0x83, 0x3E, a[0], a[1], 0x00];
            original.extend(jump);
            original.extend([0x83, 0x3E, b[0], b[1], 0x00]);
            let mut patched = vec![0x90; 7];
            patched.extend([0x80, 0x3E]);
            patched.extend(le(self.space));
            patched.push(0x00);
            sites.push(Site {
                offset: offset as usize,
                original,
                patched,
            });
        }
        sites.push(Site {
            offset: self.skip as usize,
            original: SKIP_JUMP.to_vec(),
            patched: vec![0x90; 2],
        });
        sites
    }

    /// The patch as patch file lines.
    fn patch_lines(&self) -> String {
        let mut out = String::from("\n# One-button firing, added by BloogBase\n");
        for site in self.sites() {
            out.push_str(&format!("%patch ${:04X}", site.offset));
            for b in &site.patched {
                out.push_str(&format!(" ${b:02X}"));
            }
            out.push('\n');
        }
        out
    }
}

fn episode(game_id: &str) -> Option<&'static Episode> {
    EPISODES.iter().find(|e| e.game == game_id)
}

/// Whether the patch exists for the game at all.
pub fn supports(game_id: &str) -> bool {
    episode(game_id).is_some()
}

fn classify(episode: &Episode, image: &[u8]) -> Firing {
    let mut original = true;
    let mut patched = true;
    for site in episode.sites() {
        let bytes = image.get(site.offset..site.offset + site.original.len());
        original &= bytes == Some(&site.original[..]);
        patched &= bytes == Some(&site.patched[..]);
    }
    if patched {
        Firing::BuiltIn
    } else if original {
        Firing::Patchable
    } else {
        Firing::Unavailable
    }
}

/// What a profile runs: patch files for the episode's patcher, or an
/// executable directly.
#[derive(Debug, PartialEq, Eq)]
enum Target {
    Patch(PathBuf),
    Exe(PathBuf),
}

/// Files as DOS sees them in a stack of directories: the first one that
/// has a name wins, case-insensitively, long names also by their 8.3 alias.
struct Files<'a>(&'a [&'a Path]);

impl Files<'_> {
    fn find(&self, name: &str) -> Option<PathBuf> {
        let name = name.trim_start_matches(".\\").trim_start_matches("C:\\");
        self.0.iter().find_map(|dir| {
            let files: Vec<(String, PathBuf)> = fs::read_dir(dir)
                .ok()?
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                .collect();
            let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
            files
                .iter()
                .position(|(n, _)| n.eq_ignore_ascii_case(name))
                .or_else(|| {
                    recipe::short_names(&names)
                        .iter()
                        .position(|s| s.eq_ignore_ascii_case(name))
                })
                .map(|i| files[i].1.clone())
        })
    }

    /// `name`, or `name` with one of `exts` added, as batch files spell it.
    fn find_with(&self, name: &str, exts: &[&str]) -> Option<PathBuf> {
        self.find(name)
            .or_else(|| exts.iter().find_map(|e| self.find(&format!("{name}.{e}"))))
    }

    fn episode_exe(&self, game: &Game) -> Option<PathBuf> {
        game.executables.iter().find_map(|e| self.find(e))
    }
}

/// What `recipe` runs; empty if it can't be told.
fn targets(game: &Game, recipe: &Recipe, files: &Files) -> Vec<Target> {
    match recipe {
        Recipe::Patch { patch, .. } => files.find(patch).map(Target::Patch).into_iter().collect(),
        Recipe::Program { program, .. } => {
            files.find(program).map(Target::Exe).into_iter().collect()
        }
        Recipe::Episode => files
            .episode_exe(game)
            .map(Target::Exe)
            .into_iter()
            .collect(),
        Recipe::Batch { file } => {
            let Some(text) = files.find(file).and_then(|p| fs::read(p).ok()) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for cmd in recipe::batch_commands(&String::from_utf8_lossy(&text)) {
                let program = files.find_with(&cmd[0], &["EXE", "COM"]);
                let name = program.as_ref().and_then(|p| p.file_name());
                let name = name.map(|n| n.to_string_lossy().into_owned());
                if program.as_deref().is_some_and(recipe::is_patcher_at) {
                    out.extend(
                        cmd.get(1)
                            .and_then(|p| files.find_with(p, &["PAT"]))
                            .map(Target::Patch),
                    );
                } else if game
                    .executables
                    .iter()
                    .any(|e| name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(e)))
                {
                    out.extend(program.map(Target::Exe));
                }
            }
            out
        }
    }
}

/// The load image of an executable.
fn image(exe: &Path) -> Option<Vec<u8>> {
    lzexe::load_image(&fs::read(exe).ok()?).ok()
}

/// How `target` leaves the episode's code.
fn judge(episode: &Episode, game: &Game, target: &Target, files: &Files) -> Firing {
    let image = match target {
        Target::Exe(exe) => image(exe),
        Target::Patch(patch) => files.episode_exe(game).and_then(|exe| {
            let mut image = image(&exe)?;
            let text = fs::read(patch).ok()?;
            apply_writes(&mut image, &String::from_utf8_lossy(&text), files);
            Some(image)
        }),
    };
    image.map_or(Firing::Unavailable, |i| classify(episode, &i))
}

/// Where a profile stands with the patch; `None` for games without it.
/// `dirs` are its files, topmost first, e.g. the mod's over the install's.
pub fn status(game: &Game, recipe: &Recipe, dirs: &[&Path]) -> Option<Firing> {
    let episode = episode(&game.id)?;
    let files = Files(dirs);
    let targets = targets(game, recipe, &files);
    let judged: Vec<Firing> = targets
        .iter()
        .map(|t| judge(episode, game, t, &files))
        .collect();
    Some(
        if judged.is_empty() || judged.contains(&Firing::Unavailable) {
            Firing::Unavailable
        } else if judged.iter().all(|f| *f == Firing::BuiltIn) {
            Firing::BuiltIn
        } else {
            Firing::Patchable
        },
    )
}

/// Like `status` for a profile on an install of `version_id`, the last of
/// `dirs`: where the episode's executable is the install's own and not
/// version 1.31, that's why the patch doesn't fit.
pub fn profile_status(
    game: &Game,
    recipe: &Recipe,
    dirs: &[&Path],
    version_id: Option<&str>,
) -> Option<Firing> {
    let status = status(game, recipe, dirs)?;
    let other_version = version_id.is_some_and(|v| v != format!("{}-1.31", game.id));
    let install_exe = Files(dirs)
        .episode_exe(game)
        .is_some_and(|exe| dirs.last().is_some_and(|d| exe.starts_with(d)));
    Some(
        if status == Firing::Unavailable && other_version && install_exe {
            Firing::OtherVersion
        } else {
            status
        },
    )
}

/// Applies the patch to the files in `dir`, a disposable copy, wherever
/// what the profile runs fits it. Patch files get the patch's lines; an
/// executable run directly is rewritten unpacked and patched.
pub fn apply(game: &Game, recipe: &Recipe, dir: &Path) -> Result<Firing> {
    let dirs = [dir];
    let Some(status) = status(game, recipe, &dirs) else {
        return Ok(Firing::Unavailable);
    };
    if status != Firing::Patchable {
        return Ok(status);
    }
    let episode = episode(&game.id).expect("status said it exists");
    let files = Files(&dirs);
    for target in targets(game, recipe, &files) {
        if judge(episode, game, &target, &files) != Firing::Patchable {
            continue;
        }
        match target {
            Target::Patch(path) => {
                let text = fs::read(&path)?;
                fs::write(&path, with_lines(&text, &episode.patch_lines()))?;
            }
            Target::Exe(path) => {
                let Ok(mut exe) = lzexe::unpack_exe(&fs::read(&path)?) else {
                    continue;
                };
                let header = usize::from(u16::from_le_bytes([exe[8], exe[9]])) * 16;
                for site in episode.sites() {
                    let at = header + site.offset;
                    exe[at..at + site.patched.len()].copy_from_slice(&site.patched);
                }
                fs::write(&path, exe)?;
            }
        }
    }
    Ok(Firing::Patchable)
}

/// `text` with `lines` added before its `%end`. In a patch file with
/// `%version` sections they get one for 1.31, the version they're for.
fn with_lines(text: &[u8], lines: &str) -> Vec<u8> {
    let mut at = text.len();
    let mut versions = false;
    let mut start = 0;
    for line in text.split_inclusive(|&b| b == b'\n') {
        let first = String::from_utf8_lossy(line);
        let first = first.split_whitespace().next().unwrap_or("");
        if first.eq_ignore_ascii_case("%end") {
            at = start;
            break;
        }
        versions |= first.eq_ignore_ascii_case("%version");
        start += line.len();
    }
    let mut block = String::new();
    if at > 0 && text[at - 1] != b'\n' {
        block.push('\n');
    }
    if versions {
        block.push_str("\n%version 1.31");
    }
    block.push_str(lines);
    block.push('\n');
    let mut out = text[..at].to_vec();
    out.extend(block.as_bytes());
    out.extend(&text[at..]);
    out
}

/// Writes what a patch file's `%patch` and `%patchfile` lines change into
/// `image`, as the patcher would for version 1.31.
fn apply_writes(image: &mut [u8], text: &str, files: &Files) {
    let mut write = |offset: usize, bytes: &[u8]| {
        for (i, b) in bytes.iter().enumerate() {
            if let Some(slot) = image.get_mut(offset + i) {
                *slot = *b;
            }
        }
    };
    // Directives with the text up to the next one: values may go on over
    // several lines.
    let text = text.split('\u{1A}').next().unwrap_or("");
    let mut version: Option<String> = None;
    for directive in text.split('%').skip(1) {
        let tokens = tokens(directive);
        let Some(name) = tokens.first() else { continue };
        let name = name.to_ascii_lowercase();
        if name == "end" {
            break;
        }
        if name == "version" {
            version = tokens.get(1).map(|v| v.to_ascii_uppercase());
            continue;
        }
        if version
            .as_deref()
            .is_some_and(|v| v != "1.31" && v != "ALL")
        {
            continue;
        }
        let Some(offset) = tokens.get(1).and_then(|t| number(t)) else {
            continue;
        };
        match name.as_str() {
            "patch" => {
                let mut bytes = Vec::new();
                for token in &tokens[2..] {
                    let Some(value) = value(token) else { break };
                    bytes.extend(value);
                }
                write(offset as usize, &bytes);
            }
            "patchfile" => {
                if let Some(data) = tokens
                    .get(2)
                    .and_then(|n| files.find(n.trim_matches('"')))
                    .and_then(|p| fs::read(p).ok())
                {
                    write(offset as usize, &data);
                }
            }
            _ => {}
        }
    }
}

/// Words, with quoted strings kept whole (quotes included).
fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            let mut s = String::from(chars.next().unwrap());
            for c in chars.by_ref() {
                s.push(c);
                if c == '"' {
                    break;
                }
            }
            out.push(s);
        } else {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                s.push(c);
                chars.next();
            }
            out.push(s);
        }
    }
    out
}

/// `$1F2B`, `0x1F2B` or decimal.
fn number(token: &str) -> Option<u32> {
    if let Some(hex) = token
        .strip_prefix('$')
        .or_else(|| token.strip_prefix("0x"))
        .or_else(|| token.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        token.parse().ok()
    }
}

/// A `%patch` value: a byte, a word (`$1234W`), a long (`$12345678L`) or a
/// quoted string.
fn value(token: &str) -> Option<Vec<u8>> {
    if let Some(s) = token.strip_prefix('"') {
        return Some(s.strip_suffix('"').unwrap_or(s).bytes().collect());
    }
    let upper = token.to_ascii_uppercase();
    if let Some(n) = upper.strip_suffix('W') {
        return Some(u16::try_from(number(n)?).ok()?.to_le_bytes().to_vec());
    }
    if let Some(n) = upper.strip_suffix('L') {
        return Some(number(n)?.to_le_bytes().to_vec());
    }
    Some(vec![u8::try_from(number(token)?).ok()?])
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::catalog;
    use crate::source::testutil::write_folder;

    fn keen1() -> &'static Game {
        catalog::game("keen1").unwrap()
    }

    /// An unpacked KEEN1.EXE whose code is as shipped at the patch's sites.
    pub fn exe() -> Vec<u8> {
        let mut image = vec![0u8; 0x8000];
        for site in EPISODES[0].sites() {
            image[site.offset..site.offset + site.original.len()].copy_from_slice(&site.original);
        }
        let mut exe = vec![0u8; 0x20];
        exe[..2].copy_from_slice(b"MZ");
        exe[8] = 2; // header paragraphs
        exe.extend(image);
        exe
    }

    fn patch(name: &str) -> Recipe {
        Recipe::Patch {
            patcher: "CK1PATCH.EXE".into(),
            patch: name.into(),
            args: Vec::new(),
        }
    }

    /// The patch's lines as a mod would write them, with words for the addresses.
    const MOD_LINES: &str = "\
%patch $1251  $5048W
%patch $7FA3  $48
%patch $3B23  $90 $90 $90 $90 $90 $90 $90 $80 $3E $5065W $00
%patch $3C4E  $90 $90 $90 $90 $90 $90 $90 $80 $3E $5065W $00
%patch $3DB9  $90 $90 $90 $90 $90 $90 $90
              $80 $3E $65 $50 $00
%patch $3DCC  $90 $90
%patch $3E6D  $90 $90 $90 $90 $90 $90 $90 $80 $3E $5065W $00
%patch $3FFC  $90 $90 $90 $90 $90 $90 $90 $80 $3E $5065W $00
%patch $40AA  $90 $90 $90 $90 $90 $90 $90 $80 $3E $5065W $00
";

    #[test]
    fn patch_files_get_the_lines_unless_the_mod_has_them() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let plain = "%ext ck1\n%version 1.31\n%patch $10 $01\n%end\n# notes\n";
        let built_in = format!("%ext ck1\n#Space fires\n{MOD_LINES}%end\n");
        let other = "%ext ck1\n%patch $3B23 $EB $10\n%end\n";
        // For another version only: doesn't count.
        let elsewhere = "%ext ck1\n%version 1.0\n%patch $3B23 $EB $10\n%end\n";
        write_folder(
            dir,
            &[
                ("KEEN1.EXE", &exe()),
                ("plain.pat", plain.as_bytes()),
                ("BUILTIN.PAT", built_in.as_bytes()),
                ("other.pat", other.as_bytes()),
                ("elsewhere.pat", elsewhere.as_bytes()),
            ],
        );
        let status = |p: &str| status(keen1(), &patch(p), &[dir]);
        assert_eq!(status("plain.pat"), Some(Firing::Patchable));
        assert_eq!(status("builtin.pat"), Some(Firing::BuiltIn));
        assert_eq!(status("other.pat"), Some(Firing::Unavailable));
        assert_eq!(status("elsewhere.pat"), Some(Firing::Patchable));
        assert_eq!(status("missing.pat"), Some(Firing::Unavailable));

        assert_eq!(
            apply(keen1(), &patch("plain.pat"), dir).unwrap(),
            Firing::Patchable
        );
        let text = fs::read_to_string(dir.join("plain.pat")).unwrap();
        assert!(text.starts_with("%ext ck1\n%version 1.31\n%patch $10 $01\n\n%version 1.31\n"));
        assert!(text.contains("%patch $3B23 $90 $90 $90 $90 $90 $90 $90 $80 $3E $65 $50 $00\n"));
        assert!(text.ends_with("%patch $3DCC $90 $90\n\n%end\n# notes\n"));
        assert_eq!(status("plain.pat"), Some(Firing::BuiltIn));
        // The executable is left alone; the patcher changes it.
        assert_eq!(fs::read(dir.join("KEEN1.EXE")).unwrap(), exe());

        assert_eq!(
            apply(keen1(), &patch("other.pat"), dir).unwrap(),
            Firing::Unavailable
        );
        assert_eq!(fs::read_to_string(dir.join("other.pat")).unwrap(), other);
    }

    #[test]
    fn patch_files_without_end_or_versions() {
        let out = with_lines(b"%ext ck1\r\n%patch $10 $01", "\n# x\n");
        assert_eq!(out, b"%ext ck1\r\n%patch $10 $01\n\n# x\n\n");
    }

    #[test]
    fn executables_run_directly_are_patched() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write_folder(dir, &[("keen1.exe", &exe()), ("MENU.EXE", b"MZ")]);
        assert_eq!(
            status(keen1(), &Recipe::Episode, &[dir]),
            Some(Firing::Patchable)
        );
        let menu = Recipe::Program {
            program: "MENU.EXE".into(),
            args: Vec::new(),
        };
        assert_eq!(status(keen1(), &menu, &[dir]), Some(Firing::Unavailable));

        assert_eq!(
            apply(keen1(), &Recipe::Episode, dir).unwrap(),
            Firing::Patchable
        );
        let patched = fs::read(dir.join("keen1.exe")).unwrap();
        assert_eq!(&patched[0x20 + 0x1251..][..2], &[0x48, 0x50]);
        assert_eq!(&patched[0x20 + 0x3DCC..][..2], &[0x90, 0x90]);
        assert_eq!(
            status(keen1(), &Recipe::Episode, &[dir]),
            Some(Firing::BuiltIn)
        );
        assert_eq!(
            apply(keen1(), &Recipe::Episode, dir).unwrap(),
            Firing::BuiltIn
        );
    }

    #[test]
    fn batch_files_count_every_patch_they_run() {
        let tmp = tempfile::tempdir().unwrap();
        let (base, modf) = (tmp.path().join("base"), tmp.path().join("mod"));
        write_folder(&base, &[("KEEN1.EXE", &exe())]);
        let built_in = format!("{MOD_LINES}%end\n");
        write_folder(
            &modf,
            &[
                ("CK1PATCH.EXE", b"p"),
                ("EASY.PAT", built_in.as_bytes()),
                ("HARD.PAT", built_in.as_bytes()),
                ("PLAIN.PAT", b"%patch $10 $01\n"),
                (
                    "both.bat",
                    b"@echo off\r\nck1patch easy\r\nCK1PATCH.EXE hard.pat\r\n",
                ),
                (
                    "mixed.bat",
                    b"ck1patch easy.pat\r\nck1patch plain.pat\r\nkeen1\r\n",
                ),
                ("menu.bat", b"choice\r\ngoto end\r\n"),
                // A renamed patcher and patch, as Norp the Yorp 3 ships.
                ("NORPER.EXE", b"MZ\0\0\0\0\0\0\x02\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0Usage: CK1PATCH {patchfile}"),
                ("EASY.NY1", built_in.as_bytes()),
                ("renamed.bat", b"norper easy.ny1\r\n"),
                // A long name called by its DOS alias, as Merry Keenmas does.
                ("dunepatch.EXE", b"MZ\0\0\0\0\0\0\x02\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0Usage: CK1PATCH {patchfile}"),
                ("short.bat", b"dunepa~1 easy.ny1\r\n"),
            ],
        );
        let dirs = [modf.as_path(), base.as_path()];
        let batch = |f: &str| Recipe::Batch { file: f.into() };
        assert_eq!(
            status(keen1(), &batch("both.bat"), &dirs),
            Some(Firing::BuiltIn)
        );
        assert_eq!(
            status(keen1(), &batch("mixed.bat"), &dirs),
            Some(Firing::Patchable)
        );
        assert_eq!(
            status(keen1(), &batch("menu.bat"), &dirs),
            Some(Firing::Unavailable)
        );
        assert_eq!(
            status(keen1(), &batch("renamed.bat"), &dirs),
            Some(Firing::BuiltIn)
        );
        assert_eq!(
            status(keen1(), &batch("short.bat"), &dirs),
            Some(Firing::BuiltIn)
        );
        assert_eq!(
            status(catalog::game("keen4").unwrap(), &Recipe::Episode, &dirs),
            None
        );
    }

    #[test]
    fn other_versions_of_the_game_are_named() {
        let tmp = tempfile::tempdir().unwrap();
        let (base, modf) = (tmp.path().join("base"), tmp.path().join("mod"));
        // A beta's executable: the code is elsewhere.
        write_folder(&base, &[("KEEN1.EXE", b"MZ       ")]);
        write_folder(&modf, &[("LEVEL01.CK1", b"l")]);
        let dirs = [modf.as_path(), base.as_path()];
        let status = |v| profile_status(keen1(), &Recipe::Episode, &dirs, v);
        assert_eq!(status(Some("keen1-beta-dec")), Some(Firing::OtherVersion));
        assert_eq!(status(Some("keen1-1.31")), Some(Firing::Unavailable));
        assert_eq!(status(None), Some(Firing::Unavailable));

        // With its own executable the mod doesn't depend on the version.
        write_folder(&modf, &[("KEEN1.EXE", b"MZ       ")]);
        assert_eq!(status(Some("keen1-beta-dec")), Some(Firing::Unavailable));
        write_folder(&modf, &[("KEEN1.EXE", &exe())]);
        assert_eq!(status(Some("keen1-beta-dec")), Some(Firing::Patchable));
    }

    #[test]
    fn patch_values() {
        assert_eq!(value("$5065W"), Some(vec![0x65, 0x50]));
        assert_eq!(value("$12345678L"), Some(vec![0x78, 0x56, 0x34, 0x12]));
        assert_eq!(value("0x1F"), Some(vec![0x1F]));
        assert_eq!(value("\"Hi\""), Some(b"Hi".to_vec()));
        assert_eq!(value("$100"), None);
        assert_eq!(value("#comment"), None);
        assert_eq!(
            tokens("patch $1 \"a b\" $2"),
            ["patch", "$1", "\"a b\"", "$2"]
        );
    }

    /// The real episodes in `$KEEN_GAMES/keen1` ... `keen3` take the patch.
    ///
    /// `KEEN_GAMES=<absolute path> cargo test firing::tests::real -- --ignored`
    #[test]
    #[ignore]
    fn real_episodes() {
        let root = PathBuf::from(std::env::var("KEEN_GAMES").expect("KEEN_GAMES"));
        for episode in &EPISODES {
            let tmp = tempfile::tempdir().unwrap();
            let game = catalog::game(episode.game).unwrap();
            let exe = &game.executables[0];
            fs::copy(root.join(episode.game).join(exe), tmp.path().join(exe)).unwrap();
            let dirs = [tmp.path()];
            assert_eq!(
                status(game, &Recipe::Episode, &dirs),
                Some(Firing::Patchable)
            );
            apply(game, &Recipe::Episode, tmp.path()).unwrap();
            assert_eq!(status(game, &Recipe::Episode, &dirs), Some(Firing::BuiltIn));
        }
    }
}

//! Patch files as the episodes' patchers read them, and the files a profile
//! runs them on: shared by one-button firing and custom patches.

use std::fs;
use std::path::{Path, PathBuf};

use keen_graphics::lzexe;

use crate::catalog::Game;
use crate::recipe::{self, Recipe};

/// What a profile runs: patch files for the episode's patcher, or an
/// executable directly.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    Patch(PathBuf),
    Exe(PathBuf),
}

/// Files as DOS sees them in a stack of directories: the first one that
/// has a name wins, case-insensitively, long names also by their 8.3 alias.
pub struct Files<'a>(pub &'a [&'a Path]);

impl Files<'_> {
    pub fn find(&self, name: &str) -> Option<PathBuf> {
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
    pub fn find_with(&self, name: &str, exts: &[&str]) -> Option<PathBuf> {
        self.find(name)
            .or_else(|| exts.iter().find_map(|e| self.find(&format!("{name}.{e}"))))
    }

    pub fn names(&self) -> Vec<String> {
        self.0
            .iter()
            .filter_map(|dir| fs::read_dir(dir).ok())
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    }

    pub fn episode_exe(&self, game: &Game) -> Option<PathBuf> {
        game.executables.iter().find_map(|e| self.find(e))
    }
}

/// What `recipe` runs; empty if it can't be told.
pub fn targets(game: &Game, recipe: &Recipe, files: &Files) -> Vec<Target> {
    match recipe {
        Recipe::Patch { patch, .. } => files.find(patch).map(Target::Patch).into_iter().collect(),
        // A launcher that runs the patcher, as Keen Q's KEENQ.COM, gets the
        // patch in each patch file it names: it may swap them around.
        Recipe::Program { program, .. } => {
            let Some(path) = files.find(program) else {
                return Vec::new();
            };
            let names = files.names();
            let patches = fs::read(&path)
                .map(|p| recipe::patches_named_in(&p, names.iter().map(String::as_str)))
                .unwrap_or_default();
            if patches.is_empty() {
                vec![Target::Exe(path)]
            } else {
                patches
                    .into_iter()
                    .filter_map(|n| files.find(n).map(Target::Patch))
                    .collect()
            }
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
pub fn image(exe: &Path) -> Option<Vec<u8>> {
    lzexe::load_image(&fs::read(exe).ok()?).ok()
}

/// `text` with `lines` added before its `%end`, in a `%version` section
/// for `version`, the version they're for: always with `own_section`, else
/// only in a patch file with `%version` sections.
pub fn with_lines(text: &[u8], lines: &str, version: &str, own_section: bool) -> Vec<u8> {
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
    if versions || own_section {
        block.push_str("\n%version ");
        block.push_str(version);
    }
    block.push_str(lines);
    block.push('\n');
    let mut out = text[..at].to_vec();
    out.extend(block.as_bytes());
    out.extend(&text[at..]);
    out
}

/// Writes what a patch file's `%patch` and `%patchfile` lines change into
/// `image`, as the patcher would for `version`.
pub fn apply_writes(image: &mut [u8], text: &str, files: &Files, version: &str) {
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
    let wanted = version.to_ascii_uppercase();
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
            .is_some_and(|v| *v != wanted && v != "ALL")
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
pub fn tokens(text: &str) -> Vec<String> {
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
pub fn number(token: &str) -> Option<u32> {
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
pub fn value(token: &str) -> Option<Vec<u8>> {
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
mod tests {
    use super::*;
    use crate::source::testutil::write_folder;

    #[test]
    fn patch_files_without_end_or_versions() {
        let out = with_lines(b"%ext ck1\r\n%patch $10 $01", "\n# x\n", "1.4", false);
        assert_eq!(out, b"%ext ck1\r\n%patch $10 $01\n\n# x\n\n");
    }

    #[test]
    fn lines_get_a_section_for_their_version() {
        let plain = b"%ext ck4\n%patch $10 $01\n%end\n";
        assert_eq!(
            with_lines(plain, "\n%patch $20 $02", "1.4", true),
            b"%ext ck4\n%patch $10 $01\n\n%version 1.4\n%patch $20 $02\n%end\n"
        );
        let sections = b"%ext ck4\n%version 1.0\n%patch $10 $01\n%end\n";
        assert_eq!(
            with_lines(sections, "\n%patch $20 $02", "1.4", false),
            b"%ext ck4\n%version 1.0\n%patch $10 $01\n\n%version 1.4\n%patch $20 $02\n%end\n"
        );
    }

    #[test]
    fn writes_follow_the_version() {
        let tmp = tempfile::tempdir().unwrap();
        write_folder(tmp.path(), &[("DATA.BIN", b"xy")]);
        let dirs = [tmp.path()];
        let text = "%ext ck4\n%patch $0 $01\n%version 1.0\n%patch $1 $02\n\
            %version 1.4\n%patch $2 $03 $04W\n%patchfile $5 DATA.BIN\n\
            %version all\n%patch $7 \"z\"\n%end\n%patch $8 $09\n";
        let mut image = vec![0u8; 9];
        apply_writes(&mut image, text, &Files(&dirs), "1.4");
        assert_eq!(image, [1, 0, 3, 4, 0, b'x', b'y', b'z', 0]);
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
}

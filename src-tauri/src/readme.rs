//! README files shipped with mods, for the detail page.
//!
//! Mods name their documentation freely (`README.TXT`, `Read Me.txt`,
//! `READ.ME`, `CHEEKY README.rtf`, `LIESMICH.TXT`, …), so candidates are
//! ranked by name; without one, other text documents are shown instead
//! (`changes.txt`, `story.txt`). The text goes to the UI as plain text only: DOS code page
//! 437 or Windows-1252 is decoded, RTF is reduced to its text, control
//! characters are dropped. HTML is never shown in the app: web pages are
//! listed without text and open in the browser.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use serde::Serialize;
use specta::Type;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::paths::AppPaths;
use crate::source::{Source, split_dir};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Readme {
    /// Path of the file inside the mod or its archive.
    pub file: String,
    /// Empty for a web page.
    pub text: String,
    /// A web page, which opens in the browser instead.
    pub html: bool,
}

/// Larger files are hardly a README.
const MAX_BYTES: usize = 1 << 20;
/// Deeper files belong to tools or sources shipped along.
const MAX_DEPTH: usize = 2;
/// More would hardly fit a picker; the best-ranked come first anyway.
const MAX_FILES: usize = 20;
/// Of documents; files without an extension count too.
const EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "nfo", "doc", "rtf", "asc", "htm", "html",
];

/// The documents of an installed mod, the README first; empty when it has
/// none. The kept archive is searched, so files outside the installed mod
/// folder count too.
pub fn for_mod(paths: &AppPaths, db: &Db, mod_id: &str) -> Result<Vec<Readme>> {
    match mod_source(paths, db, mod_id)? {
        Some(source) => find(&source),
        None => Ok(Vec::new()),
    }
}

/// The web page `file` of a mod on disk, for the browser. From an archive,
/// it is copied to the cache with the files in its folder (pictures, style
/// sheets).
pub fn html_path(paths: &AppPaths, db: &Db, mod_id: &str, file: &str) -> Result<PathBuf> {
    let not_found = || Error::NotFound(format!("{file} of mod {mod_id}"));
    let source = mod_source(paths, db, mod_id)?.ok_or_else(not_found)?;
    let files = source.files()?;
    if !(candidate(file).is_some_and(|(_, html, _)| html) && files.iter().any(|f| f == file)) {
        return Err(not_found());
    }
    match &source {
        Source::Folder(root) => Ok(root.join(file)),
        Source::Zip(_) => {
            let dest = paths.cache.join("readmes").join(mod_id);
            // An update of the mod may have changed the page; a missing
            // folder or a file still open elsewhere doesn't matter.
            let _ = fs::remove_dir_all(&dest);
            let prefix = match split_dir(file).0 {
                "" => String::new(),
                dir => format!("{dir}/"),
            };
            for f in files.iter().filter(|f| f.starts_with(&prefix)) {
                source.copy_to(f, &dest.join(f))?;
            }
            Ok(dest.join(file))
        }
    }
}

/// The kept archive of a mod, else its installed files.
fn mod_source(paths: &AppPaths, db: &Db, mod_id: &str) -> Result<Option<Source>> {
    db.conn()
        .query_row("SELECT 1 FROM mods WHERE id = ?1", [mod_id], |_| Ok(()))
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))?;
    let dir = paths.mods.join(mod_id);
    Ok(match archive(&dir.join("source")) {
        Some(zip) => Some(Source::Zip(zip)),
        None if dir.join("files").is_dir() => Some(Source::Folder(dir.join("files"))),
        None => None,
    })
}

fn archive(dir: &Path) -> Option<std::path::PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")))
}

/// The readable candidates in `source`, best-named first; web pages after
/// text documents of the same rank, as only text shows in the app.
pub fn find(source: &Source) -> Result<Vec<Readme>> {
    let mut candidates: Vec<((u8, bool, usize), String)> = source
        .files()?
        .into_iter()
        .filter_map(|f| Some((candidate(&f)?, f)))
        .collect();
    candidates.sort();
    let mut found = Vec::new();
    for ((_, html, _), file) in candidates {
        if found.len() == MAX_FILES {
            break;
        }
        if html {
            found.push(Readme {
                file,
                text: String::new(),
                html,
            });
            continue;
        }
        let bytes = match source.read(&file) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("could not read {file}: {e}");
                continue;
            }
        };
        if bytes.len() > MAX_BYTES {
            continue;
        }
        if let Some(text) = to_text(&bytes)
            && !text.trim().is_empty()
            && !is_patch_script(&text)
        {
            found.push(Readme { file, text, html });
        }
    }
    Ok(found)
}

/// Rank, whether it is a web page, and depth of a document; `None` for
/// other files.
fn candidate(rel: &str) -> Option<(u8, bool, usize)> {
    let depth = rel.matches('/').count();
    let name = split_dir(rel).1;
    let html = name
        .rsplit_once('.')
        .is_some_and(|(_, e)| e.eq_ignore_ascii_case("htm") || e.eq_ignore_ascii_case("html"));
    (depth <= MAX_DEPTH).then_some((rank(name)?, html, depth))
}

/// Lower is better; `None` for files that are no README.
fn rank(name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    let (stem, ext) = match lower.rsplit_once('.') {
        Some((s, e)) if EXTENSIONS.contains(&e) => (s, e),
        // `READ.ME`, `README.1ST`
        Some((_, "me" | "1st")) => (lower.as_str(), ""),
        Some(_) => return None,
        None => (lower.as_str(), ""),
    };
    let key: String = stem.chars().filter(char::is_ascii_alphanumeric).collect();
    const EXACT: &[&str] = &[
        "readme",
        "readmefirst",
        "readme1st",
        "readthis",
        "readfirst",
        "liesmich",
        "lisezmoi",
        "leame",
        "leesmij",
    ];
    const OTHER: &[&str] = &[
        "info",
        "about",
        "help",
        "manual",
        "instructions",
        "docs",
        "documentation",
        "anleitung",
    ];
    // Without a README, any text document beats an empty tab: change logs
    // after the rest, spoilers (hints, walkthroughs) last.
    const CHANGES: &[&str] = &["change", "history", "whatsnew", "news", "version"];
    const SPOILERS: &[&str] = &[
        "hint",
        "solution",
        "walkthrough",
        "cheat",
        "secret",
        "spoiler",
    ];
    // CKPatch's own manual comes along with the patcher in many mods.
    if key.starts_with("ckpatch") {
        None
    } else if EXACT.contains(&key.as_str()) {
        Some(0)
    } else if key.contains("readme") || key.contains("readthis") || key.contains("liesmich") {
        Some(1)
    } else if !ext.is_empty() && (OTHER.contains(&key.as_str()) || ext == "nfo") {
        Some(2)
    } else if ext.is_empty() {
        None
    } else if SPOILERS.iter().any(|w| key.contains(w)) {
        Some(5)
    } else if CHANGES.iter().any(|w| key.contains(w)) {
        Some(4)
    } else {
        Some(3)
    }
}

/// CKPatch scripts are plain text too, but no reading matter.
fn is_patch_script(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .is_some_and(|l| l.starts_with('%'))
}

/// Plain text, or `None` for binary files (e.g. a Word `.doc`).
fn to_text(bytes: &[u8]) -> Option<String> {
    let text = if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        String::from_utf8_lossy(rest).into_owned()
    } else if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        utf16(rest, u16::from_le_bytes)
    } else if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        utf16(rest, u16::from_be_bytes)
    } else if bytes.starts_with(b"{\\rtf") {
        // WordPad ends its files with a NUL.
        rtf_to_text(bytes)
    } else if bytes
        .iter()
        .rposition(|&b| b != 0 && b != 0x1A)
        .is_some_and(|end| bytes[..end].contains(&0))
    {
        return None;
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        s.to_string()
    } else if looks_like_cp437(bytes) {
        bytes.iter().map(|&b| cp437(b)).collect()
    } else {
        bytes.iter().map(|&b| cp1252(b)).collect()
    };
    Some(clean(&text))
}

fn utf16(bytes: &[u8], f: fn([u8; 2]) -> u16) -> String {
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|&c| f(c)).collect();
    String::from_utf16_lossy(&units)
}

/// DOS text has box drawing (0xB0–0xDF) and accented letters below 0xB0;
/// Windows text has accented small letters (0xE0–0xFF) and typographic
/// punctuation. Bytes Windows-1252 leaves undefined settle it.
fn looks_like_cp437(bytes: &[u8]) -> bool {
    const UNDEFINED: &[u8] = &[0x81, 0x8D, 0x8F, 0x90, 0x9D];
    const PUNCTUATION: &[u8] = &[0x85, 0x91, 0x92, 0x93, 0x94, 0x96, 0x97];
    let (mut dos, mut win) = (0, 0);
    for b in bytes {
        match b {
            b if UNDEFINED.contains(b) => return true,
            b if PUNCTUATION.contains(b) => win += 1,
            0x80..=0xDF => dos += 1,
            0xE0..=0xFF => win += 1,
            _ => {}
        }
    }
    dos > win
}

/// Unix line ends, no DOS end-of-file marker, no terminal escapes or other
/// control characters except tabs.
fn clean(text: &str) -> String {
    let text = text.split('\u{1A}').next().unwrap_or_default();
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // ANSI escape sequence, e.g. ESC[1;33m
            '\u{1B}' => {
                if chars.next_if_eq(&'[').is_some() {
                    while chars.next().is_some_and(|c| !c.is_ascii_alphabetic()) {}
                }
            }
            '\u{0C}' => out.push('\n'),
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.trim_end().to_string()
}

/// The text of an RTF document: paragraphs, tabs, `\'hh` (Windows-1252) and
/// `\uN` escapes. Font tables, pictures and other destinations are skipped.
fn rtf_to_text(bytes: &[u8]) -> String {
    const SKIP: &[&str] = &[
        "fonttbl",
        "colortbl",
        "stylesheet",
        "info",
        "pict",
        "object",
        "header",
        "footer",
        "headerl",
        "headerr",
        "footerl",
        "footerr",
        "listtable",
        "listoverridetable",
        "rsidtbl",
        "generator",
        "xmlnstbl",
        "themedata",
        "colorschememapping",
        "latentstyles",
        "datastore",
        "fldinst",
    ];
    let mut out = String::new();
    // Per group: skipped, and characters to drop after `\uN` (`\ucN`).
    let mut stack: Vec<(bool, usize)> = Vec::new();
    let (mut skip, mut uc) = (false, 1usize);
    let mut pending = 0usize;
    let mut i = 0;
    let at = |i: usize| bytes.get(i).copied();
    while let Some(b) = at(i) {
        i += 1;
        match b {
            b'{' => {
                stack.push((skip, uc));
                if at(i) == Some(b'\\') && at(i + 1) == Some(b'*') {
                    skip = true;
                }
            }
            b'}' => (skip, uc) = stack.pop().unwrap_or((false, 1)),
            b'\\' => {
                let Some(c) = at(i) else { break };
                if c.is_ascii_alphabetic() {
                    let start = i;
                    while at(i).is_some_and(|c| c.is_ascii_alphabetic()) {
                        i += 1;
                    }
                    let word = std::str::from_utf8(&bytes[start..i]).unwrap_or_default();
                    let num_start = i;
                    if at(i) == Some(b'-') {
                        i += 1;
                    }
                    while at(i).is_some_and(|c| c.is_ascii_digit()) {
                        i += 1;
                    }
                    let num: Option<i32> = std::str::from_utf8(&bytes[num_start..i])
                        .ok()
                        .and_then(|n| n.parse().ok());
                    if at(i) == Some(b' ') {
                        i += 1;
                    }
                    if SKIP.contains(&word) {
                        skip = true;
                    }
                    if skip {
                        continue;
                    }
                    match word {
                        "par" | "line" | "sect" | "page" => out.push('\n'),
                        "tab" => out.push('\t'),
                        "emdash" => out.push('—'),
                        "endash" => out.push('–'),
                        "bullet" => out.push('•'),
                        "lquote" => out.push('‘'),
                        "rquote" => out.push('’'),
                        "ldblquote" => out.push('“'),
                        "rdblquote" => out.push('”'),
                        "uc" => uc = num.unwrap_or(1).max(0) as usize,
                        "u" => {
                            let n = num.unwrap_or(0);
                            let n = if n < 0 { n + 65536 } else { n } as u32;
                            out.push(char::from_u32(n).unwrap_or('\u{FFFD}'));
                            pending = uc;
                        }
                        _ => {}
                    }
                    continue;
                }
                i += 1;
                if skip {
                    if c == b'\'' {
                        i += 2;
                    }
                    continue;
                }
                match c {
                    b'\'' => {
                        let hex = bytes
                            .get(i..i + 2)
                            .and_then(|h| std::str::from_utf8(h).ok());
                        i += 2;
                        if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                            if pending > 0 {
                                pending -= 1;
                            } else {
                                out.push(cp1252(v));
                            }
                        }
                    }
                    b'~' => out.push('\u{A0}'),
                    b'\n' | b'\r' => out.push('\n'),
                    b'\\' | b'{' | b'}' => out.push(c as char),
                    _ => {}
                }
            }
            b'\r' | b'\n' => {}
            _ if skip => {}
            _ if pending > 0 => pending -= 1,
            _ => {
                // Plain bytes outside escapes are ASCII in practice.
                out.push(cp1252(b));
            }
        }
    }
    out
}

fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

fn cp437(b: u8) -> char {
    const HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»\
                        ░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀\
                        αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{A0}";
    if b < 0x80 {
        b as char
    } else {
        HIGH.chars().nth((b - 0x80) as usize).unwrap_or('\u{FFFD}')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::testutil::*;

    #[test]
    fn ranks_readme_names() {
        for name in [
            "README.TXT",
            "readme.md",
            "Read Me.txt",
            "READ_ME.txt",
            "READ.ME",
            "README",
            "README.1ST",
            "READTHIS!.rtf",
            "LIESMICH.TXT",
        ] {
            assert_eq!(rank(name), Some(0), "{name}");
        }
        assert_eq!(rank("CHEEKY README.rtf"), Some(1));
        assert_eq!(rank("Aurora_readme_en.txt"), Some(1));
        assert_eq!(rank("Help.txt"), Some(2));
        assert_eq!(rank("FIREB.NFO"), Some(2));
        assert_eq!(rank("story.txt"), Some(3));
        assert_eq!(rank("changes.txt"), Some(4));
        assert_eq!(rank("ChangeLog15Y.txt"), Some(4));
        assert_eq!(rank("Hints Sheet.txt"), Some(5));
        assert_eq!(rank("HINTS..txt"), Some(5));
        assert_eq!(rank("README.HTM"), Some(0));
        assert_eq!(rank("story.html"), Some(3));
        for name in [
            "README.EXE",
            "readme.pdf",
            "HELP",
            "INFO.CK4",
            "KEEN4.PAT",
            "CKPATCH.TXT",
        ] {
            assert_eq!(rank(name), None, "{name}");
        }
    }

    fn names(source: &Source) -> Vec<String> {
        find(source).unwrap().into_iter().map(|r| r.file).collect()
    }

    #[test]
    fn lists_the_best_name_then_the_shallowest_first() {
        let tmp = tempfile::tempdir().unwrap();
        write_folder(
            tmp.path(),
            &[
                ("Mod/Help.txt", b"help"),
                ("Mod/tools/src/README.md", b"deep"),
                ("Mod/docs/readme.txt", b"docs"),
                ("readme.txt", b"top"),
                ("readme.html", b"<script>alert(1)</script>"),
                ("Mod/GAMEMAPS.CK4", b"maps"),
            ],
        );
        let found = find(&Source::Folder(tmp.path().into())).unwrap();
        assert_eq!(
            found[0],
            Readme {
                file: "readme.txt".into(),
                text: "top".into(),
                html: false
            }
        );
        assert_eq!(
            names(&Source::Folder(tmp.path().into())),
            [
                "readme.txt",
                "Mod/docs/readme.txt",
                "readme.html",
                "Mod/Help.txt"
            ]
        );
    }

    #[test]
    fn skips_empty_and_binary_candidates() {
        let tmp = tempfile::tempdir().unwrap();
        write_folder(
            tmp.path(),
            &[
                ("README.TXT", b"\r\n  \r\n\x1a\0\0"),
                ("readme.doc", b"\xD0\xCF\x11\xE0\0\0binary"),
                ("INFO.TXT", b"info"),
            ],
        );
        assert_eq!(names(&Source::Folder(tmp.path().into())), ["INFO.TXT"]);
    }

    #[test]
    fn lists_web_pages_without_their_text() {
        let tmp = tempfile::tempdir().unwrap();
        write_folder(tmp.path(), &[("README.HTM", b"<script>alert(1)</script>")]);
        assert_eq!(
            find(&Source::Folder(tmp.path().into())).unwrap(),
            [Readme {
                file: "README.HTM".into(),
                text: String::new(),
                html: true
            }]
        );
    }

    #[test]
    fn web_pages_open_from_disk_with_their_pictures() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO mods (id, game_id, name, authors, recipe_json, path, source, added_at)
                 VALUES ('shadow', 'keen2', 'Shadow', '[]', '{}', '/x', 'manual', 0)",
                [],
            )
            .unwrap();
        let dir = paths.mods.join("shadow");
        write_folder(&dir.join("files"), &[("README.HTM", b"<p>hi")]);
        assert_eq!(
            html_path(&paths, &db, "shadow", "README.HTM").unwrap(),
            dir.join("files/README.HTM")
        );

        fs::create_dir_all(dir.join("source")).unwrap();
        write_zip(
            &dir.join("source/shadow.zip"),
            &[
                ("Files/README.HTM", b"<img src=README_files/a.gif>"),
                ("Files/README_files/a.gif", b"GIF"),
                ("Files/README.TXT", b"text"),
                ("Menu.exe", b"MZ"),
            ],
        );
        let page = html_path(&paths, &db, "shadow", "Files/README.HTM").unwrap();
        let copy = paths.cache.join("readmes/shadow/Files");
        assert_eq!(page, copy.join("README.HTM"));
        assert_eq!(fs::read(copy.join("README_files/a.gif")).unwrap(), b"GIF");
        assert!(!paths.cache.join("readmes/shadow/Menu.exe").exists());

        // Only the mod's web pages open.
        for file in ["Files/README.TXT", "Menu.exe", "README.HTM", "../x.htm"] {
            assert!(matches!(
                html_path(&paths, &db, "shadow", file),
                Err(Error::NotFound(_))
            ));
        }
        assert!(matches!(
            html_path(&paths, &db, "other", "README.HTM"),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn falls_back_to_other_text_files() {
        // Commander Cool 1 ships only these two.
        let tmp = tempfile::tempdir().unwrap();
        write_folder(
            tmp.path(),
            &[
                ("Hints Sheet.txt", b"Level 1: the key is behind the wall"),
                ("changes.txt", b"v1.1: fixed a crash"),
                ("patch.txt", b"\r\n%ext ck4\r\n%version 1.4\r\n"),
            ],
        );
        assert_eq!(
            names(&Source::Folder(tmp.path().into())),
            ["changes.txt", "Hints Sheet.txt"]
        );

        let tmp = tempfile::tempdir().unwrap();
        write_folder(tmp.path(), &[("patch.txt", b"%ext ck4\n%end\n")]);
        assert_eq!(find(&Source::Folder(tmp.path().into())).unwrap(), []);
    }

    #[test]
    fn finds_readmes_in_zips() {
        let tmp = tempfile::tempdir().unwrap();
        let zip = tmp.path().join("mod.zip");
        write_zip(
            &zip,
            &[
                ("Aurora/GAMEMAPS.CK4", b"maps"),
                ("Aurora/READ ME.txt", b"hi there"),
                ("__MACOSX/Aurora/._READ ME.txt", b"junk"),
            ],
        );
        let found = find(&Source::Zip(zip)).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, "Aurora/READ ME.txt");
        assert_eq!(found[0].text, "hi there");
    }

    #[test]
    fn keeps_to_the_first_files_and_skips_huge_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let huge = vec![b'x'; MAX_BYTES + 1];
        let mut files: Vec<(String, &[u8])> = vec![("README.TXT".into(), &huge)];
        files.extend((0..MAX_FILES + 5).map(|i| (format!("doc{i:02}.txt"), &b"text"[..])));
        let files: Vec<(&str, &[u8])> = files.iter().map(|(n, d)| (n.as_str(), *d)).collect();
        write_folder(tmp.path(), &files);
        let found = names(&Source::Folder(tmp.path().into()));
        assert_eq!(found.len(), MAX_FILES);
        assert_eq!(found[0], "doc00.txt");
    }

    #[test]
    fn reads_an_installed_mod_from_its_archive_or_files() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        let db = Db::open_in_memory().unwrap();
        assert!(matches!(
            for_mod(&paths, &db, "aurora"),
            Err(Error::NotFound(_))
        ));
        db.conn()
            .execute(
                "INSERT INTO mods (id, game_id, name, authors, recipe_json, path, source, added_at)
                 VALUES ('aurora', 'keen4', 'Aurora', '[]', '{}', '/x', 'manual', 0)",
                [],
            )
            .unwrap();
        assert_eq!(for_mod(&paths, &db, "aurora").unwrap(), []);

        // The installed files only, without an archive.
        let dir = paths.mods.join("aurora");
        write_folder(&dir.join("files"), &[("README.TXT", b"installed")]);
        let texts = |db: &Db| -> Vec<String> {
            for_mod(&paths, db, "aurora")
                .unwrap()
                .into_iter()
                .map(|r| r.text)
                .collect()
        };
        assert_eq!(texts(&db), ["installed"]);

        // The kept archive has files the installed folder lacks.
        fs::create_dir_all(dir.join("source")).unwrap();
        fs::write(dir.join("source/notes.bin"), b"no zip").unwrap();
        write_zip(
            &dir.join("source/Aurora.ZIP"),
            &[
                ("Aurora/README.TXT", b"archived"),
                ("Extras/story.txt", b"story"),
            ],
        );
        assert_eq!(texts(&db), ["archived", "story"]);
    }

    #[test]
    fn code_page_437_has_all_characters() {
        assert_eq!(cp437(0x80), 'Ç');
        assert_eq!(cp437(0xFE), '■');
        assert_eq!(cp437(0xFF), '\u{A0}');
    }

    #[test]
    fn decodes_dos_text() {
        // Box drawing in code page 437, CRLF, end-of-file marker.
        let text = to_text(b"\xC9\xCD\xBB\r\n\xBA\x81\xBA\r\n\xC8\xCD\xBC\r\n\x1agarbage");
        assert_eq!(text.as_deref(), Some("╔═╗\n║ü║\n╚═╝"));
    }

    #[test]
    fn decodes_windows_text() {
        let text = to_text(b"Gr\xFC\xDFe \x93Keen\x94 \x96 caf\xE9");
        assert_eq!(text.as_deref(), Some("Grüße “Keen” – café"));
        let text = to_text(b"It\x92s");
        assert_eq!(text.as_deref(), Some("It’s"));
        // ü and ß in code page 437
        let text = to_text(b"Gr\x81\xE1e");
        assert_eq!(text.as_deref(), Some("Grüße"));
    }

    #[test]
    fn decodes_utf8_and_utf16() {
        assert_eq!(
            to_text("\u{FEFF}Grüße ═".as_bytes()).as_deref(),
            Some("Grüße ═")
        );
        let mut le = vec![0xFF, 0xFE];
        le.extend("Hé".encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(to_text(&le).as_deref(), Some("Hé"));
    }

    #[test]
    fn drops_control_characters_and_escapes() {
        let text = to_text(b"\x1b[1;33mKeen\x1b[0m\x07\tgo\x0cnext\rline");
        assert_eq!(text.as_deref(), Some("Keen\tgo\nnext\nline"));
    }

    #[test]
    fn reduces_rtf_to_text() {
        let rtf = br"{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0\fnil\fcharset0 Courier New;}}
{\colortbl ;\red0\green0\blue255;}
{\*\generator Riched20 10.0.19041}\viewkind4\uc1
\pard\f0\fs22\lang1033 Commander Keen in...\tab\b Canmore\rquote s\b0\par
Gr\'fc\'dfe \{braces\} \u8364?uro\par
{\pict\wmetafile8 0102abcdef}\par
}";
        assert_eq!(
            to_text(&[&rtf[..], b"\0"].concat()).as_deref(),
            Some("Commander Keen in...\tCanmore’s\nGrüße {braces} €uro")
        );
    }
}

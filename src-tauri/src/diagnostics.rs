//! The diagnostics players paste into a GitHub issue: versions, the library
//! at a glance, the last played profile and the ends of the logs, with
//! personal paths taken out.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use rusqlite::OptionalExtension;

use crate::catalog;
use crate::db::Db;
use crate::error::Result;
use crate::library;
use crate::paths::AppPaths;
use crate::profiles;
use crate::runtime;

/// Lines from the end of BloogBase's own log.
const APP_LOG_LINES: usize = 80;
/// Lines from the end of the last played profile's DOSBox log.
const DOSBOX_LOG_LINES: usize = 40;
/// Longer log lines are cut; GitHub takes at most 65536 characters.
const MAX_LINE: usize = 300;

/// What the report says about the app itself.
pub struct App {
    pub version: String,
    pub webview: Option<String>,
    /// The data folder isn't the default one.
    pub moved: bool,
}

/// The report as Markdown.
pub fn report(paths: &AppPaths, db: &Db, app: &App) -> Result<String> {
    let mut out = String::new();
    let _ = writeln!(out, "### BloogBase diagnostics\n");
    let _ = writeln!(out, "- BloogBase {}", app.version);
    let _ = writeln!(out, "- {}, {}", system(), std::env::consts::ARCH);
    if let Some(webview) = &app.webview {
        let _ = writeln!(out, "- WebView2 {webview}");
    }
    let rt = runtime::status(db)?;
    let _ = match &rt.active {
        Some(r) if r.managed => writeln!(out, "- DOSBox Staging {} (managed)", r.version),
        Some(r) => writeln!(out, "- DOSBox {} (own executable)", r.version),
        None => writeln!(out, "- DOSBox not installed"),
    };
    let _ = writeln!(
        out,
        "- Data folder: {}",
        if app.moved { "moved" } else { "default" }
    );

    let _ = writeln!(out, "\n**Games**\n");
    let installs = library::list(db)?;
    if installs.is_empty() {
        let _ = writeln!(out, "- none");
    }
    for i in &installs {
        let _ = writeln!(
            out,
            "- {} · {} · {}",
            i.game_id,
            version_label(&i.game_id, i.version_id.as_deref()),
            i.source.as_str()
        );
    }

    let mods: Vec<(String, u32)> = {
        let conn = db.conn();
        let mut stmt =
            conn.prepare("SELECT source, COUNT(*) FROM mods GROUP BY source ORDER BY source")?;
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let total: u32 = mods.iter().map(|(_, n)| n).sum();
    let by_source: Vec<String> = mods.iter().map(|(s, n)| format!("{n} {s}")).collect();
    let _ = if total > 0 {
        writeln!(out, "\n**Mods:** {total} ({})", by_source.join(", "))
    } else {
        writeln!(out, "\n**Mods:** 0")
    };

    if let Some(id) = last_played(db)? {
        last_profile(&mut out, paths, db, &id)?;
    }

    let log = paths.logs.join("bloogbase.log");
    log_section(&mut out, "BloogBase log", &log, APP_LOG_LINES);
    Ok(anonymize(&out, paths))
}

fn last_played(db: &Db) -> Result<Option<String>> {
    Ok(db
        .conn()
        .query_row(
            "SELECT profile_id FROM profile_stats WHERE last_played_at IS NOT NULL
             ORDER BY last_played_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?)
}

fn last_profile(out: &mut String, paths: &AppPaths, db: &Db, id: &str) -> Result<()> {
    let (profile, layers) = profiles::get(db, id)?;
    let _ = writeln!(out, "\n**Last played:** {}\n", profile.label);
    let _ = writeln!(
        out,
        "- Game: {} · {}",
        profile.game_id,
        version_label(&profile.game_id, profile.version_id.as_deref())
    );
    if let Some(mod_id) = &profile.mod_id {
        let (version, index_id, source): (Option<String>, Option<String>, String) =
            db.conn().query_row(
                "SELECT version, index_id, source FROM mods WHERE id = ?1",
                [mod_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
        let mut line = format!("- Mod: {mod_id}");
        if let Some(v) = version {
            let _ = write!(line, " · {v}");
        }
        let _ = write!(line, " · {source}");
        if let Some(index_id) = index_id.filter(|i| i != mod_id) {
            let _ = write!(line, " ({index_id})");
        }
        let _ = writeln!(out, "{line}");
    }
    if let Some(m) = &layers.mod_layer {
        let start = m.recipe.command().unwrap_or_else(|| "the episode".into());
        let _ = writeln!(out, "- Start: `{start}`");
    }
    if let Some(over) = &profile.played_over {
        let _ = writeln!(out, "- Played over: {}", over.name);
    }
    if let Some(missing) = &profile.missing_game {
        let _ = writeln!(out, "- Waiting for: {missing}");
    }
    let _ = writeln!(out, "- Sessions: {}", profile.session_count);
    let log = paths.logs.join(format!("dosbox-{id}.log"));
    log_section(out, "DOSBox log", &log, DOSBOX_LOG_LINES);
    Ok(())
}

fn version_label(game_id: &str, version_id: Option<&str>) -> String {
    let Some(version_id) = version_id else {
        return "unknown version".into();
    };
    catalog::game(game_id)
        .and_then(|g| g.versions.iter().find(|v| v.id == version_id))
        .map_or_else(|| version_id.to_string(), |v| v.label.clone())
}

/// The end of a log in a folded block; nothing if it can't be read.
fn log_section(out: &mut String, title: &str, path: &Path, lines: usize) {
    let Ok(data) = fs::read(path) else {
        return;
    };
    let text = String::from_utf8_lossy(&data);
    let tail = tail(&text, lines);
    if tail.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "\n<details><summary>{title} (last {lines} lines)</summary>\n\n````text\n{tail}\n````\n</details>"
    );
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    all[all.len().saturating_sub(lines)..]
        .iter()
        .map(|l| match l.char_indices().nth(MAX_LINE) {
            Some((at, _)) => format!("{} …", &l[..at]),
            None => (*l).to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Takes out the data folder, the home folder and any other user name in
/// `Users\<name>` paths.
fn anonymize(text: &str, paths: &AppPaths) -> String {
    let mut text = text.to_string();
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    let known = [
        (paths.root.display().to_string(), "<data>"),
        (
            home.map(|h| Path::new(&h).display().to_string())
                .unwrap_or_default(),
            "~",
        ),
    ];
    for (path, with) in known {
        if path.len() < 3 {
            continue;
        }
        text = replace_path(&text, &path, with);
        text = replace_path(&text, &path.replace('\\', "/"), with);
    }
    user_names(&text)
}

/// Replaces `path` case-insensitively, as Windows paths compare.
fn replace_path(text: &str, path: &str, with: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let needle = path.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (at, _) in lower.match_indices(&needle) {
        if at < last {
            continue;
        }
        out.push_str(&text[last..at]);
        out.push_str(with);
        last = at + needle.len();
    }
    out.push_str(&text[last..]);
    out
}

fn user_names(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut at = 0;
    while let Some(found) = ["\\users\\", "/users/", "/home/"]
        .iter()
        .filter_map(|s| lower[at..].find(s).map(|i| (at + i, s.len())))
        .min()
    {
        let (start, len) = found;
        let name_start = start + len;
        let name_end = text[name_start..]
            .find(['\\', '/', '"', '\'', ' ', '\n', ')', ']'])
            .map_or(text.len(), |i| name_start + i);
        let name = &lower[name_start..name_end];
        if !name.is_empty() && !matches!(name, "public" | "default" | "all users") {
            out.push_str(&text[last..name_start]);
            out.push_str("<user>");
            last = name_end;
        }
        at = name_end.max(name_start);
    }
    out.push_str(&text[last..]);
    out
}

/// The Windows version from the registry, e.g. `Windows 11 25H2 (build 26200.6901)`.
#[cfg(windows)]
fn system() -> String {
    use windows_sys::Win32::System::Registry::{
        HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
    };
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let key = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let string = |name: &str| -> Option<String> {
        let mut buf = [0u16; 128];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: the buffer and its size in bytes match.
        let res = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                wide(name).as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        (res == 0).then(|| {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            String::from_utf16_lossy(&buf[..len])
        })
    };
    let mut ubr = 0u32;
    let mut size = 4u32;
    // SAFETY: a DWORD value into a u32.
    let has_ubr = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            wide("UBR").as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut ubr as *mut u32).cast(),
            &mut size,
        )
    } == 0;
    let Some(build) = string("CurrentBuildNumber") else {
        return "Windows".into();
    };
    // The product name still says Windows 10 on Windows 11.
    let name = match build.parse::<u32>() {
        Ok(b) if b >= 22000 => "Windows 11",
        _ => "Windows 10",
    };
    let release = string("DisplayVersion")
        .map(|r| format!(" {r}"))
        .unwrap_or_default();
    let build = if has_ubr {
        format!("{build}.{ubr}")
    } else {
        build
    };
    format!("{name}{release} (build {build})")
}

#[cfg(not(windows))]
fn system() -> String {
    std::env::consts::OS.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saves::tests::setup;

    fn app() -> App {
        App {
            version: "1.2.3".into(),
            webview: None,
            moved: false,
        }
    }

    #[test]
    fn reports_games_the_last_profile_and_logs() {
        let (_tmp, paths, db) = setup();
        db.conn()
            .execute(
                "INSERT INTO play_sessions (profile_id, started_at, ended_at) VALUES ('keen4-1', 10, 20)",
                [],
            )
            .unwrap();
        let root = paths.root.display().to_string();
        fs::write(
            paths.logs.join("bloogbase.log"),
            format!("early\n\nlaunching from {root}\\profiles\\keen4-1\n"),
        )
        .unwrap();
        fs::write(paths.logs.join("dosbox-keen4-1.log"), "DOSBox says hi\n").unwrap();

        let report = report(&paths, &db, &app()).unwrap();
        assert!(report.contains("- BloogBase 1.2.3"), "{report}");
        assert!(report.contains("- DOSBox not installed"), "{report}");
        assert!(
            report.contains("- keen4 · unknown version · folder"),
            "{report}"
        );
        assert!(report.contains("**Mods:** 0"), "{report}");
        assert!(report.contains("**Last played:**"), "{report}");
        assert!(report.contains("DOSBox says hi"), "{report}");
        assert!(
            report.contains("launching from <data>\\profiles\\keen4-1"),
            "{report}"
        );
        assert!(!report.contains(&root), "{report}");
    }

    #[test]
    fn user_names_go() {
        assert_eq!(
            user_names(r#"at D:\Users\Florian\Games and "C:/users/fb/x" and C:\Users\Public"#),
            r#"at D:\Users\<user>\Games and "C:/users/<user>/x" and C:\Users\Public"#
        );
    }

    #[test]
    fn paths_go_whatever_their_case() {
        assert_eq!(
            replace_path(r"C:\DATA\x and c:\data\y", r"C:\Data", "<data>"),
            r"<data>\x and <data>\y"
        );
    }

    #[test]
    fn tail_keeps_the_last_lines_and_cuts_long_ones() {
        let long = "x".repeat(MAX_LINE + 5);
        let text = format!("a\nb\n\nc\n{long}\n");
        let got = tail(&text, 2);
        assert_eq!(got, format!("c\n{} …", "x".repeat(MAX_LINE)));
    }
}

//! Finds Commander Keen installs from Steam and GOG (PLAN.md §5). Only the
//! stores' library folders are looked at, never the whole disk.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::db::Db;
use crate::detect::detect;
use crate::error::Result;
use crate::library::{self, InstallSource};
use crate::source::Source;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Store {
    Steam,
    Gog,
}

impl Store {
    pub fn install_source(self) -> InstallSource {
        match self {
            Store::Steam => InstallSource::Steam,
            Store::Gog => InstallSource::Gog,
        }
    }
}

/// A store folder with one or more episodes in it.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StoreInstall {
    pub store: Store,
    pub path: String,
    pub games: Vec<FoundGame>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FoundGame {
    pub game_id: String,
    pub version_id: Option<String>,
    /// Already imported from this folder.
    pub imported: bool,
}

/// Scans the usual Steam and GOG locations of this computer.
pub fn scan(db: &Db) -> Result<Vec<StoreInstall>> {
    let home = std::env::home_dir();
    let mut candidates = Vec::new();
    for lib in steam_libraries(home.as_deref()) {
        candidates.extend(keen_dirs(
            &lib.join("steamapps").join("common"),
            Store::Steam,
        ));
    }
    for root in gog_roots(home.as_deref()) {
        candidates.extend(keen_dirs(&root, Store::Gog));
    }
    #[cfg(windows)]
    candidates.extend(
        windows::gog_registry_paths()
            .into_iter()
            .map(|p| (Store::Gog, p)),
    );
    inspect(db, candidates)
}

fn inspect(db: &Db, candidates: Vec<(Store, PathBuf)>) -> Result<Vec<StoreInstall>> {
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    for (store, dir) in candidates {
        let key = fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if !dir.is_dir() || !seen.insert(key) {
            continue;
        }
        let detections = match Source::open(&dir).and_then(|s| detect(&s)) {
            Ok(d) => d,
            Err(err) => {
                tracing::warn!("could not scan {}: {err}", dir.display());
                continue;
            }
        };
        if detections.is_empty() {
            continue;
        }
        let path = dir.display().to_string();
        let mut games = Vec::new();
        for d in detections {
            // A store folder may hold the same episode twice (e.g. a backup copy).
            if games.iter().any(|g: &FoundGame| g.game_id == d.game_id) {
                continue;
            }
            games.push(FoundGame {
                imported: library::has_origin(db, &d.game_id, &path)?,
                game_id: d.game_id,
                version_id: d.version_id,
            });
        }
        tracing::info!("found {} game(s) in {}", games.len(), path);
        found.push(StoreInstall { store, path, games });
    }
    Ok(found)
}

/// Subfolders of `parent` that look like a Keen install by name.
fn keen_dirs(parent: &Path, store: Store) -> Vec<(Store, PathBuf)> {
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("keen")
        })
        .map(|e| (store, e.path()))
        .collect()
}

/// Steam's root folders plus every extra library listed in `libraryfolders.vdf`.
fn steam_libraries(home: Option<&Path>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        roots.extend(windows::steam_registry_path());
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(dir) = std::env::var_os(var) {
                roots.push(PathBuf::from(dir).join("Steam"));
            }
        }
    }
    if let Some(home) = home {
        if cfg!(target_os = "macos") {
            roots.push(home.join("Library/Application Support/Steam"));
        } else if cfg!(target_os = "linux") {
            roots.push(home.join(".steam/steam"));
            roots.push(home.join(".local/share/Steam"));
            roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
        }
    }
    let mut libraries = roots.clone();
    for root in &roots {
        if let Ok(vdf) = fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
            libraries.extend(library_folders(&vdf).into_iter().map(PathBuf::from));
        }
    }
    libraries
}

/// The `"path"` values of Steam's `libraryfolders.vdf`.
fn library_folders(vdf: &str) -> Vec<String> {
    vdf.lines()
        .filter_map(|line| {
            let mut quoted = line.split('"').skip(1).step_by(2);
            match (quoted.next(), quoted.next()) {
                (Some(key), Some(value)) if key.eq_ignore_ascii_case("path") => {
                    Some(value.replace("\\\\", "\\"))
                }
                _ => None,
            }
        })
        .collect()
}

/// Folders GOG Galaxy, the GOG installers and Heroic install games into.
fn gog_roots(home: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        roots.push(PathBuf::from(r"C:\GOG Games"));
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(dir) = std::env::var_os(var) {
                roots.push(PathBuf::from(dir).join("GOG Galaxy").join("Games"));
            }
        }
    }
    if let Some(home) = home {
        roots.push(home.join("GOG Games"));
        roots.push(home.join("Games").join("Heroic"));
        if cfg!(target_os = "macos") {
            roots.push(PathBuf::from("/Applications"));
        }
    }
    roots
}

#[cfg(windows)]
mod windows {
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    use crate::runtime::hide_console;

    /// Runs `reg query` and returns the data of all string values it prints.
    fn reg_query(args: &[&str]) -> Vec<String> {
        let mut cmd = Command::new("reg");
        cmd.arg("query")
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut cmd);
        let Ok(out) = cmd.output() else {
            return Vec::new();
        };
        super::reg_string_values(&String::from_utf8_lossy(&out.stdout))
    }

    pub fn steam_registry_path() -> Option<PathBuf> {
        reg_query(&[r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
            .into_iter()
            .next()
            .map(PathBuf::from)
    }

    /// Install folders of GOG games whose path mentions Keen.
    pub fn gog_registry_paths() -> Vec<PathBuf> {
        reg_query(&[
            r"HKLM\SOFTWARE\WOW6432Node\GOG.com\Games",
            "/s",
            "/v",
            "path",
        ])
        .into_iter()
        .filter(|p| p.to_ascii_lowercase().contains("keen"))
        .map(PathBuf::from)
        .collect()
    }
}

/// Parses `reg query` output lines like `    path    REG_SZ    C:\Games\Keen`.
#[cfg_attr(not(windows), allow(dead_code))]
fn reg_string_values(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let (_, value) = line.split_once("REG_SZ")?;
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::testutil::*;

    #[test]
    fn parses_library_folders() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"9180"		"2341235"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
	}
}"#;
        assert_eq!(
            library_folders(vdf),
            [r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]
        );
    }

    #[test]
    fn parses_reg_output() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\GOG.com\\Games\\1207658704\r\n    path    REG_SZ    C:\\GOG Games\\Commander Keen\r\n\r\nEnd of search: 1 match(es) found.\r\n";
        assert_eq!(reg_string_values(out), [r"C:\GOG Games\Commander Keen"]);
    }

    #[test]
    fn finds_keen_folders_and_marks_imported_games() {
        let tmp = tempfile::tempdir().unwrap();
        let common = tmp.path().join("steamapps/common");
        let keen = common.join("Commander Keen");
        write_folder(&keen.join("keen4"), KEEN4);
        write_folder(&common.join("Other Game/x"), KEEN4);

        let dirs = keen_dirs(&common, Store::Steam);
        assert_eq!(dirs.len(), 1);

        let db = Db::open_in_memory().unwrap();
        // The same folder listed twice shows up once.
        let mut candidates = dirs.clone();
        candidates.extend(dirs);
        let found = inspect(&db, candidates.clone()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].store, Store::Steam);
        assert_eq!(found[0].games.len(), 1);
        assert_eq!(found[0].games[0].game_id, "keen4");
        assert!(!found[0].games[0].imported);

        let paths = crate::paths::AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        library::import_from(
            &paths,
            &db,
            &keen,
            InstallSource::Steam,
            &found[0].path,
            Some(&["keen4".to_string()]),
        )
        .unwrap();
        assert!(inspect(&db, candidates).unwrap()[0].games[0].imported);
    }
}

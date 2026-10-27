//! Finds games inside an import source (PLAN.md §5).

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use sha2::{Digest, Sha256};
use specta::Type;

use crate::catalog::{self, Game};
use crate::error::Result;
use crate::source::{Source, split_dir};

/// One game found in a source directory.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub game_id: String,
    /// Directory inside the source, `""` for the root.
    pub dir: String,
    /// `None` if the executable hash is not in the catalog.
    pub version_id: Option<String>,
    /// Pristine game files (paths relative to the source).
    pub base_files: Vec<String>,
    /// Saves, config and score files found next to the game.
    pub state_files: Vec<String>,
}

pub fn detect(source: &Source) -> Result<Vec<Detection>> {
    // Directory → files directly inside it. Games are flat, so subdirectories
    // are only scanned as candidates of their own.
    let mut dirs: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let files = source.files()?;
    for rel in &files {
        dirs.entry(split_dir(rel).0).or_default().push(rel);
    }

    let mut found = Vec::new();
    for (dir, files) in dirs {
        let by_name: HashMap<String, &str> = files
            .iter()
            .map(|rel| (split_dir(rel).1.to_ascii_uppercase(), *rel))
            .collect();
        let has = |names: &str| {
            names
                .split('|')
                .any(|n| by_name.contains_key(&n.to_ascii_uppercase()))
        };

        let games: Vec<&Game> = catalog::games()
            .iter()
            .filter(|g| {
                g.executables.iter().any(|e| has(e)) && g.required_files.iter().all(|f| has(f))
            })
            .collect();
        if games.is_empty() {
            continue;
        }

        let is_state = |rel: &str| {
            catalog::games()
                .iter()
                .any(|g| g.is_state_file(split_dir(rel).1))
        };
        let base_files: Vec<String> = files
            .iter()
            .filter(|r| !is_state(r))
            .map(|r| r.to_string())
            .collect();
        for game in games {
            found.push(Detection {
                game_id: game.id.clone(),
                dir: dir.to_string(),
                version_id: identify_version(source, game, &by_name)?,
                base_files: base_files.clone(),
                state_files: files
                    .iter()
                    .filter(|r| game.is_state_file(split_dir(r).1))
                    .map(|r| r.to_string())
                    .collect(),
            });
        }
    }
    Ok(found)
}

/// The catalog version of `game` whose files lie at the root of `source`.
pub fn identify(source: &Source, game: &Game) -> Result<Option<String>> {
    let files = source.files()?;
    let by_name: HashMap<String, &str> = files
        .iter()
        .filter(|rel| split_dir(rel).0.is_empty())
        .map(|rel| (rel.to_ascii_uppercase(), rel.as_str()))
        .collect();
    identify_version(source, game, &by_name)
}

fn identify_version(
    source: &Source,
    game: &Game,
    by_name: &HashMap<String, &str>,
) -> Result<Option<String>> {
    let mut hashes: HashMap<&str, String> = HashMap::new();
    for version in &game.versions {
        let mut matches = true;
        for (name, expected) in &version.files {
            let Some(rel) = by_name.get(&name.to_ascii_uppercase()) else {
                matches = false;
                break;
            };
            if !hashes.contains_key(rel) {
                hashes.insert(rel, sha256_hex(&source.read(rel)?));
            }
            if hashes[rel] != *expected {
                matches = false;
                break;
            }
        }
        if matches {
            return Ok(Some(version.id.clone()));
        }
    }
    Ok(None)
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// Lowercase hex, the format used for hashes in all manifests.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::testutil::*;

    fn prefixed(prefix: &str, files: &[(&str, &'static [u8])]) -> Vec<(String, &'static [u8])> {
        files
            .iter()
            .map(|(n, d)| (format!("{prefix}{n}"), *d))
            .collect()
    }

    #[test]
    fn detects_keen4_and_splits_state() {
        let tmp = tempfile::tempdir().unwrap();
        write_folder(tmp.path(), KEEN4);
        let found = detect(&Source::open(tmp.path()).unwrap()).unwrap();
        assert_eq!(found.len(), 1);
        let d = &found[0];
        assert_eq!(
            (d.game_id.as_str(), d.dir.as_str(), d.version_id.as_deref()),
            ("keen4", "", None)
        );
        assert_eq!(
            d.base_files,
            ["AUDIO.CK4", "EGAGRAPH.CK4", "GAMEMAPS.CK4", "KEEN4E.EXE"]
        );
        assert_eq!(d.state_files, ["SAVEGAM0.CK4", "config.ck4"]);
    }

    #[test]
    fn missing_required_file_is_not_detected() {
        let tmp = tempfile::tempdir().unwrap();
        write_folder(tmp.path(), &KEEN4[..2]);
        assert!(
            detect(&Source::open(tmp.path()).unwrap())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn detects_several_games_in_a_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let k5: &[(&str, &[u8])] = &[
            ("keen5e.exe", b"x"),
            ("GAMEMAPS.CK5", b""),
            ("AUDIO.CK5", b""),
        ];
        let mut files = prefixed("Keens/K4/", KEEN4);
        files.extend(prefixed("Keens/K5/", k5));
        let refs: Vec<(&str, &[u8])> = files.iter().map(|(n, d)| (n.as_str(), *d)).collect();
        let zip = tmp.path().join("keens.zip");
        write_zip(&zip, &refs);

        let found = detect(&Source::open(&zip).unwrap()).unwrap();
        let ids: Vec<_> = found
            .iter()
            .map(|d| (d.game_id.as_str(), d.dir.as_str()))
            .collect();
        assert_eq!(ids, [("keen4", "Keens/K4"), ("keen5", "Keens/K5")]);
    }

    #[test]
    fn identifies_known_version_by_hash() {
        let game = catalog::game("keen5").unwrap();
        let version = &game.versions[0];
        let exe = b"pretend";
        let mut patched = game.clone();
        patched.versions[0]
            .files
            .insert("KEEN5E.EXE".into(), sha256_hex(exe));

        let tmp = tempfile::tempdir().unwrap();
        write_folder(tmp.path(), &[("KEEN5E.EXE", exe)]);
        let source = Source::open(tmp.path()).unwrap();
        let by_name = HashMap::from([("KEEN5E.EXE".to_string(), "KEEN5E.EXE")]);
        assert_eq!(
            identify_version(&source, &patched, &by_name).unwrap(),
            Some(version.id.clone())
        );
        assert_eq!(identify_version(&source, game, &by_name).unwrap(), None);
    }

    #[test]
    fn other_files_tell_apart_releases_sharing_an_executable() {
        let mut game = catalog::game("kdreams").unwrap().clone();
        let exe: &[u8] = b"kdreams";
        let shell: &[u8] = b"shell";
        for v in &mut game.versions {
            for (name, hash) in &mut v.files {
                *hash = sha256_hex(if name == "KDREAMS.EXE" { exe } else { shell });
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        write_folder(tmp.path(), &[("KDREAMS.EXE", exe), ("DOCSHELL.EXE", shell)]);
        let source = Source::open(tmp.path()).unwrap();
        let mut by_name = HashMap::from([("KDREAMS.EXE".to_string(), "KDREAMS.EXE")]);
        let id = |game: &Game, by_name: &HashMap<String, &str>| {
            identify_version(&source, game, by_name).unwrap()
        };
        // Every version now has the same executable: the first one with
        // nothing else to check wins.
        assert_eq!(id(&game, &by_name).as_deref(), Some("kdreams-1.0"));
        game.versions.retain(|v| v.id.starts_with("kdreams-1.93"));
        assert_eq!(id(&game, &by_name).as_deref(), Some("kdreams-1.93"));
        by_name.insert("DOCSHELL.EXE".into(), "DOCSHELL.EXE");
        assert_eq!(
            id(&game, &by_name).as_deref(),
            Some("kdreams-1.93-anthology")
        );
    }

    #[test]
    fn required_files_may_have_alternatives() {
        // Keen Dreams v1.0 names its data files like the later games.
        let tmp = tempfile::tempdir().unwrap();
        write_folder(
            tmp.path(),
            &[
                ("kdreams.exe", b"x"),
                ("gamemaps.kdr", b""),
                ("audio.kdr", b""),
            ],
        );
        let found = detect(&Source::open(tmp.path()).unwrap()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].game_id, "kdreams");
    }

    #[test]
    fn sha256() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// Detects every folder in `$KEEN_GAMES/versions` (default
    /// `.local/games`, a checkout of the private games repo) and checks it is
    /// recognized as the catalog version its name says.
    ///
    /// `cargo test --lib detect::tests::real_versions -- --ignored`
    #[test]
    #[ignore = "needs the real games"]
    fn real_versions() {
        let expected = [
            ("Commander Keen 1 November Beta", "keen1-beta-nov"),
            ("Commander Keen 1 December Beta", "keen1-beta-dec"),
            ("Commander Keen 1 v1.1", "keen1-1.1"),
            ("Commander Keen 1 v1.3", "keen1-1.3"),
            ("Commander Keen 1 v1.31", "keen1-1.31"),
            ("Commander Keen 1 v1.32", "keen1-1.32"),
            ("Commander Keen 1 v1.34", "keen1-1.34"),
            ("Commander Keen 2 v1.0", "keen2-1.0"),
            ("Commander Keen 2 v.1.31", "keen2-1.31"),
            ("Commander Keen 3 v1.0", "keen3-1.0"),
            ("Commander Keen 3 v1.31", "keen3-1.31"),
            ("Commander Keen 4 Special Demo Version", "keen4-demo"),
            ("Commander Keen 4 v1.0", "keen4-1.0-ega"),
            ("Commander Keen 4 v1.0 CGA", "keen4-1.0-cga"),
            ("Commander Keen 4 v1.1", "keen4-1.1-ega"),
            ("Commander Keen 4 v1.1 CGA", "keen4-1.1-cga"),
            ("Commander Keen 4 v1.2", "keen4-1.2-ega"),
            ("Commander Keen 4 v1.4", "keen4-1.4-ega"),
            ("Commander Keen 4 v1.4 CGA", "keen4-1.4-cga"),
            (
                "Commander Keen 4 v1.4 FormGen Corporation EGA",
                "keen4-1.4-ega-formgen",
            ),
            (
                "Commander Keen 4 v1.4 FormGen Corporation CGA",
                "keen4-1.4-cga-formgen",
            ),
            (
                "Commander Keen 4 v1.4 GT Interactive Software",
                "keen4-1.4-ega-gt",
            ),
            ("Commander Keen 5 v1.0", "keen5-1.0-ega"),
            ("Commander Keen 5 v1.4", "keen5-1.4-ega"),
            ("Commander Keen 5 v1.4 CGA", "keen5-1.4-cga"),
            (
                "Commander Keen 5 v1.4 GT Interactive Software",
                "keen5-1.4-ega-gt",
            ),
            ("Commander Keen 6 Special Demo Version", "keen6-demo"),
            (
                "Commander Keen 6 Promotional Release Version",
                "keen6-promo",
            ),
            ("Commander Keen 6 v1.0", "keen6-1.0"),
            ("Commander Keen 6 v1.0 (hacked)", "keen6-1.0-nocp"),
            ("Commander Keen 6 v1.0 CGA", "keen6-1.0-cga"),
            ("Commander Keen 6 v1.4", "keen6-1.4-ega"),
            ("Commander Keen 6 v1.4 CGA", "keen6-1.4-cga"),
            ("Commander Keen 6 v1.5", "keen6-1.5-ega"),
            ("Commander Keen 6 v1.5 (hacked)", "keen6-1.5-ega-nocp"),
            ("Commander Keen 6 v1.5 CGA", "keen6-1.5-cga"),
            ("Keen Dreams v1.0", "kdreams-1.0"),
            ("Keen Dreams v1.01 Shareware", "kdreams-1.01"),
            ("Keen Dreams v1.13 Shareware", "kdreams-1.13"),
            ("Keen Dreams v1.92 REV 0", "kdreams-1.92"),
            ("Keen Dreams v1.93 REV 1", "kdreams-1.93"),
            ("Keen Dreams id Anthology Version", "kdreams-1.93-anthology"),
        ];
        let root = std::env::var_os("KEEN_GAMES")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.local/games")
            })
            .join("versions");

        let mut wrong = Vec::new();
        for (folder, version) in expected {
            let found: Vec<_> = detect(&Source::open(&root.join(folder)).unwrap())
                .unwrap()
                .into_iter()
                .map(|d| (d.dir, d.version_id))
                .collect();
            if found != [(String::new(), Some(version.to_string()))] {
                wrong.push(format!("{folder}: {found:?}, expected {version}"));
            }
        }
        // Every catalog version is covered, and no folder is left out.
        let versions: usize = catalog::games().iter().map(|g| g.versions.len()).sum();
        let folders = std::fs::read_dir(&root).unwrap().count();
        assert!(wrong.is_empty(), "{wrong:#?}");
        assert_eq!((expected.len(), folders), (versions, versions));
    }
}

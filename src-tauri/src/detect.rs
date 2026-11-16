//! Finds games inside an import source (PLAN.md §5).

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

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
    /// Base files under another name than the game expects: path in the
    /// source → the name they get in the library. An executable the
    /// player renamed (`KEEN5.EXE` for `KEEN5E.EXE`) is known by its hash.
    pub renamed: BTreeMap<String, String>,
}

impl Detection {
    /// The name `rel`, one of `base_files`, gets in the library.
    pub fn library_name<'a>(&'a self, rel: &'a str) -> &'a str {
        self.renamed
            .get(rel)
            .map_or_else(|| split_dir(rel).1, String::as_str)
    }

    /// The base file the library will have as `name`.
    pub fn base_file(&self, name: &str) -> Option<&str> {
        self.base_files
            .iter()
            .find(|rel| self.library_name(rel).eq_ignore_ascii_case(name))
            .map(String::as_str)
    }
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

        // Hashes of this directory's files, for renamed executables.
        let mut hashes: HashMap<&str, String> = HashMap::new();
        let mut games: Vec<(&Game, BTreeMap<String, String>)> = Vec::new();
        for game in catalog::games() {
            if !game.required_files.iter().all(|f| has(f)) {
                continue;
            }
            if game.executables.iter().any(|e| has(e)) {
                games.push((game, BTreeMap::new()));
            } else if let Some((rel, name)) = renamed_executable(source, game, &files, &mut hashes)?
            {
                games.push((game, BTreeMap::from([(rel.to_string(), name.to_string())])));
            }
        }
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
        for (game, renamed) in games {
            let mut by_name = by_name.clone();
            for (rel, name) in &renamed {
                by_name.insert(name.to_ascii_uppercase(), rel.as_str());
            }
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
                renamed,
            });
        }
    }
    Ok(found)
}

/// A file among `files` (one directory) that is a known executable of
/// `game` under another name, with that name: same extension, same hash.
/// The first catalog version with a match wins.
fn renamed_executable<'a>(
    source: &Source,
    game: &'a Game,
    files: &[&'a str],
    hashes: &mut HashMap<&'a str, String>,
) -> Result<Option<(&'a str, &'a str)>> {
    let extension = |name: &str| name.rsplit_once('.').map(|(_, e)| e.to_ascii_uppercase());
    for version in &game.versions {
        for (name, expected) in &version.files {
            if !game
                .executables
                .iter()
                .any(|e| e.eq_ignore_ascii_case(name))
            {
                continue;
            }
            for &rel in files {
                let file = split_dir(rel).1;
                if extension(file) != extension(name) || game.is_state_file(file) {
                    continue;
                }
                if !hashes.contains_key(rel) {
                    hashes.insert(rel, sha256_hex(&source.read(rel)?));
                }
                if hashes[rel] == *expected {
                    return Ok(Some((rel, name.as_str())));
                }
            }
        }
    }
    Ok(None)
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

/// File name → the hashes it has in some release, by game id.
type Originals = HashMap<String, HashMap<String, Vec<String>>>;

static ORIGINAL_DATA: LazyLock<Originals> = LazyLock::new(|| {
    let mut all: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(include_str!("../catalog/original-data.json"))
            .expect("bundled original-data.json is invalid");
    all.remove("$comment");
    serde_json::from_value(all.into()).expect("bundled original-data.json is invalid")
});

/// Whether `detection` is a known release whose data files (levels,
/// graphics) aren't those of any release: a mod copied over the game,
/// without a patch to tell. An unknown executable may be any release, so
/// its data files aren't judged.
pub fn modified_data(source: &Source, detection: &Detection) -> Result<bool> {
    modified_against(source, detection, &ORIGINAL_DATA)
}

fn modified_against(source: &Source, detection: &Detection, originals: &Originals) -> Result<bool> {
    if detection.version_id.is_none() {
        return Ok(false);
    }
    let Some(known) = originals.get(&detection.game_id) else {
        return Ok(false);
    };
    for rel in &detection.base_files {
        let name = detection.library_name(rel).to_ascii_uppercase();
        if let Some(hashes) = known.get(&name)
            && !hashes.contains(&sha256_hex(&source.read(rel)?))
        {
            return Ok(true);
        }
    }
    Ok(false)
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
    fn renamed_executable_is_known_by_its_hash() {
        let mut game = catalog::game("keen5").unwrap().clone();
        let exe: &[u8] = b"keen5";
        game.versions[1]
            .files
            .insert("KEEN5E.EXE".into(), sha256_hex(exe));
        let tmp = tempfile::tempdir().unwrap();
        write_folder(
            tmp.path(),
            &[
                ("K5/KEEN5.EXE", exe),
                ("K5/OTHER.EXE", b"other"),
                ("K5/KEEN5.TXT", exe),
            ],
        );
        let source = Source::open(tmp.path()).unwrap();
        let files = ["K5/KEEN5.TXT", "K5/OTHER.EXE", "K5/KEEN5.EXE"];
        let found = |game: &Game| {
            renamed_executable(&source, game, &files, &mut HashMap::new())
                .unwrap()
                .map(|(rel, name)| (rel.to_string(), name.to_string()))
        };
        // Only a file with the executable's extension counts.
        assert_eq!(
            found(&game),
            Some(("K5/KEEN5.EXE".into(), "KEEN5E.EXE".into()))
        );
        assert_eq!(found(catalog::game("keen5").unwrap()), None);

        let d = Detection {
            game_id: "keen5".into(),
            dir: "K5".into(),
            version_id: None,
            base_files: files.map(String::from).to_vec(),
            state_files: Vec::new(),
            renamed: BTreeMap::from([("K5/KEEN5.EXE".into(), "KEEN5E.EXE".into())]),
        };
        assert_eq!(d.library_name("K5/KEEN5.EXE"), "KEEN5E.EXE");
        assert_eq!(d.library_name("K5/OTHER.EXE"), "OTHER.EXE");
        assert_eq!(d.base_file("keen5e.exe"), Some("K5/KEEN5.EXE"));
        assert_eq!(d.base_file("KEEN5.EXE"), None);
    }

    /// A real release with its executable renamed is found and identified.
    ///
    /// `cargo test --lib detect::tests::real_renamed_executable -- --ignored`
    #[test]
    #[ignore = "needs the real games"]
    fn real_renamed_executable() {
        let root = std::env::var_os("KEEN_GAMES")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.local/games")
            })
            .join("versions");
        for (folder, exe, renamed, version) in [
            (
                "Commander Keen 5 v1.4",
                "KEEN5E.EXE",
                "KEEN5.EXE",
                "keen5-1.4-ega",
            ),
            (
                "Commander Keen 4 v1.4",
                "KEEN4E.EXE",
                "K4.EXE",
                "keen4-1.4-ega",
            ),
            (
                "Commander Keen 6 v1.4",
                "KEEN6.EXE",
                "KEEN6E1.EXE",
                "keen6-1.4-ega",
            ),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            for e in std::fs::read_dir(root.join(folder)).unwrap() {
                let e = e.unwrap();
                let name = e.file_name().to_string_lossy().into_owned();
                let to = if name.eq_ignore_ascii_case(exe) {
                    renamed
                } else {
                    &name
                };
                std::fs::copy(e.path(), tmp.path().join(to)).unwrap();
            }
            let found = detect(&Source::open(tmp.path()).unwrap()).unwrap();
            assert_eq!(found.len(), 1, "{folder}");
            assert_eq!(found[0].version_id.as_deref(), Some(version), "{folder}");
            assert_eq!(
                found[0].renamed,
                BTreeMap::from([(renamed.to_string(), exe.to_string())]),
                "{folder}"
            );
        }
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
    fn data_of_no_release_is_a_mod() {
        let tmp = tempfile::tempdir().unwrap();
        for (name, data) in [
            ("KEEN3.EXE", &b"exe"[..]),
            ("LEVEL01.CK3", b"level"),
            ("README.TXT", b"x"),
        ] {
            std::fs::write(tmp.path().join(name), data).unwrap();
        }
        let source = Source::open(tmp.path()).unwrap();
        let detection = |version: Option<&str>| Detection {
            game_id: "keen3".into(),
            dir: String::new(),
            version_id: version.map(String::from),
            base_files: vec![
                "KEEN3.EXE".into(),
                "LEVEL01.CK3".into(),
                "README.TXT".into(),
            ],
            state_files: vec![],
            renamed: BTreeMap::new(),
        };
        let originals = |level: &[u8]| -> Originals {
            HashMap::from([(
                "keen3".into(),
                HashMap::from([("LEVEL01.CK3".into(), vec!["0".into(), sha256_hex(level)])]),
            )])
        };
        let known = detection(Some("keen3-1.31"));
        assert!(!modified_against(&source, &known, &originals(b"level")).unwrap());
        assert!(modified_against(&source, &known, &originals(b"other")).unwrap());
        // An unknown release may have levels of its own.
        assert!(!modified_against(&source, &detection(None), &originals(b"other")).unwrap());
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
            ("Commander Keen 2 v1.1", "keen2-1.1"),
            ("Commander Keen 2 v1.32", "keen2-1.32"),
            ("Commander Keen 3 v1.0", "keen3-1.0"),
            ("Commander Keen 3 v1.31", "keen3-1.31"),
            ("Commander Keen 3 v1.1", "keen3-1.1"),
            ("Commander Keen 3 v1.32", "keen3-1.32"),
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
            ("Commander Keen 6 Apogee Demo Version", "keen6-demo-apogee"),
            (
                "Commander Keen 6 Promotional Release Version",
                "keen6-promo",
            ),
            ("Commander Keen 6 v1.0", "keen6-1.0"),
            ("Commander Keen 6 v1.0 (hacked)", "keen6-1.0-nocp"),
            ("Commander Keen 6 v1.0 CGA", "keen6-1.0-cga"),
            (
                "Commander Keen 6 id Anthology Version",
                "keen6-1.0-cga-anthology",
            ),
            ("Commander Keen 6 v1.4", "keen6-1.4-ega"),
            ("Commander Keen 6 v1.4 CGA", "keen6-1.4-cga"),
            ("Commander Keen 6 v1.5", "keen6-1.5-ega"),
            ("Commander Keen 6 v1.5 (hacked)", "keen6-1.5-ega-nocp"),
            ("Commander Keen 6 v1.5 CGA", "keen6-1.5-cga"),
            ("Keen Dreams v1.0", "kdreams-1.0"),
            ("Keen Dreams v1.01 Shareware", "kdreams-1.01"),
            ("Keen Dreams v1.05 REV 1", "kdreams-1.05"),
            ("Keen Dreams v1.13 Shareware", "kdreams-1.13"),
            ("Keen Dreams v1.20 Shareware", "kdreams-1.20"),
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
            let source = Source::open(&root.join(folder)).unwrap();
            for d in detect(&source).unwrap() {
                if modified_data(&source, &d).unwrap() {
                    wrong.push(format!("{folder}: data files taken for a mod's"));
                }
            }
        }
        // Every catalog version is covered, and no folder is left out.
        let versions: usize = catalog::games().iter().map(|g| g.versions.len()).sum();
        let folders = std::fs::read_dir(&root)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_dir())
            .count();
        assert!(wrong.is_empty(), "{wrong:#?}");
        assert_eq!((expected.len(), folders), (versions, versions));
    }
}

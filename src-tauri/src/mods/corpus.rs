use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::detect::sha256_hex;
use crate::recipe::Recipe;
use crate::source::Source;

use super::*;
use crate::downloads;

const MANIFEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/mod-corpus.json");

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    #[serde(rename = "_comment")]
    comment: String,
    mods: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    url: String,
    #[serde(default)]
    sha256: String,
    #[serde(flatten)]
    expect: Option<Expected>,
}

/// What `inspect` should report: the game, the name, and the default recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    game_id: String,
    name: String,
    recipe: Recipe,
    version_ids: Vec<String>,
    confident: bool,
    /// Brings an executable and all of the game's files.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    complete: bool,
}

impl From<&ModInfo> for Expected {
    fn from(info: &ModInfo) -> Self {
        Expected {
            game_id: info.game_id.clone(),
            name: info.name.clone(),
            recipe: info.recipes[0].recipe.clone(),
            version_ids: info.recipes[0].version_ids.clone(),
            confident: info.confident,
            complete: info.complete,
        }
    }
}

fn cache_dir() -> PathBuf {
    std::env::var_os("BLOOGBASE_MOD_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.local/mods"))
}

/// The cached archive, downloaded first if missing. Downloads are spaced
/// out so the community's file server isn't hammered.
async fn archive(entry: &Entry) -> std::result::Result<PathBuf, String> {
    let file = downloads::url_file_name(&entry.url);
    let path = cache_dir().join(file);
    if !path.is_file() {
        if entry.sha256.is_empty() {
            return Err("not cached and no sha256 to verify a download".into());
        }
        eprintln!("downloading {}", entry.url);
        downloads::fetch(&entry.url, &entry.sha256, 0, &path, &|_, _| {})
            .await
            .map_err(|e| e.to_string())?;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok(path)
}

#[tokio::test]
#[ignore = "downloads real mods on first run"]
async fn corpus_matches_expectations() {
    let update = std::env::var_os("BLOOGBASE_CORPUS_UPDATE").is_some();
    let mut manifest: Manifest =
        serde_json::from_str(&fs::read_to_string(MANIFEST).unwrap()).unwrap();
    let mut failures = Vec::new();

    for entry in &mut manifest.mods {
        let path = match archive(entry).await {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{}: {e}", entry.url));
                continue;
            }
        };
        let hash = sha256_hex(&fs::read(&path).unwrap());
        if entry.sha256.is_empty() && update {
            entry.sha256 = hash.clone();
        }
        if !hash.eq_ignore_ascii_case(&entry.sha256) {
            failures.push(format!("{}: cached file has sha256 {hash}", path.display()));
            continue;
        }
        let actual = match inspect(&path, None) {
            Ok(info) => Expected::from(&info),
            Err(e) => {
                failures.push(format!("{}: {e}", entry.url));
                continue;
            }
        };
        if update {
            entry.expect = Some(actual);
        } else if entry.expect.as_ref() != Some(&actual) {
            failures.push(format!(
                "{}\n  expected {:?}\n  actual   {actual:?}",
                entry.url, entry.expect
            ));
        }
    }

    if update {
        fs::write(
            MANIFEST,
            serde_json::to_string_pretty(&manifest).unwrap() + "\n",
        )
        .unwrap();
        eprintln!("updated {MANIFEST}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!("{} mods as expected", manifest.mods.len());
}

/// Index entries whose download brings all of its game's files say so:
/// `standalone` (the build adds `requiresGame` where the game has no free
/// shareware),
/// and the others don't. Reads `index/dist/index.json` (`pnpm index:build`)
/// and the archives `pnpm index:check` cached.
///
/// `cargo test index_entries_say -- --ignored --nocapture`
#[test]
#[ignore = "needs the index's cached downloads"]
fn index_entries_say_whether_they_bring_their_game() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("index/dist/index.json")).unwrap()).unwrap();
    let cache = root.join(".local/mods/index-cache");
    let mut wrong = Vec::new();
    let mut checked = 0;
    for e in index["mods"].as_array().unwrap() {
        let entry: crate::mod_index::IndexEntry = serde_json::from_value(e.clone()).unwrap();
        // Source mods are standalone without the game's files; mods played
        // over another one have only part of them.
        let source_mod = entry.standalone && entry.downloads.iter().any(|d| !d.programs.is_empty());
        if source_mod || entry.extends.is_some() {
            continue;
        }
        // Older downloads too: installed ones follow the entry.
        for d in &entry.downloads {
            let path = cache.join(format!("{}.zip", d.sha256.to_ascii_lowercase()));
            if !path.is_file() {
                wrong.push(format!("{}: {} not cached", entry.id, d.url));
                continue;
            }
            checked += 1;
            // A launcher's root holds no game; the folder it played from
            // before, or its first, holds one.
            let folder = entry.root.as_ref().map(|r| {
                let dir = entry
                    .save_dirs
                    .iter()
                    .find(|d| d.old_root)
                    .or(entry.save_dirs.first())
                    .map_or("", |d| d.dir.as_str());
                [r.as_str(), dir]
                    .into_iter()
                    .filter(|p| !p.is_empty())
                    .collect::<Vec<_>>()
                    .join("/")
            });
            let source = Source::open(&path).unwrap();
            let info =
                match analyze::analyze_in(&source, &path, Some(&entry.game), folder.as_deref())
                    .map(|a| a.info)
                {
                    Ok(i) => i,
                    Err(err) => {
                        wrong.push(format!("{}: {err}", entry.id));
                        continue;
                    }
                };
            let says = (entry.standalone, entry.requires_game);
            let is = (info.complete, info.complete && !info.standalone);
            if says != is {
                wrong.push(format!(
                    "{}: standalone/requiresGame {says:?}, {} {is:?}",
                    entry.id, d.url
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    eprintln!("{checked} downloads as expected");
}

/// Every index download's `fingerprint` is the one its archive gives
/// ([`super::index_add::fingerprint`]). With
/// `BLOOGBASE_WRITE_FINGERPRINTS=1` it writes missing or wrong ones into
/// `index/mods/<id>.yaml` instead. Reads `index/dist/index.json`
/// (`pnpm index:build`) and the archives `pnpm index:check` cached.
///
/// `cargo test index_fingerprints -- --ignored --nocapture`
#[test]
#[ignore = "needs the index's cached downloads"]
fn index_fingerprints_are_those_of_the_downloads() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("index/dist/index.json")).unwrap()).unwrap();
    let cache = root.join(".local/mods/index-cache");
    let write = std::env::var_os("BLOOGBASE_WRITE_FINGERPRINTS").is_some();
    let mut wrong = Vec::new();
    let mut written = 0;
    for e in index["mods"].as_array().unwrap() {
        let entry: crate::mod_index::IndexEntry = serde_json::from_value(e.clone()).unwrap();
        for d in &entry.downloads {
            let path = cache.join(format!("{}.zip", d.sha256.to_ascii_lowercase()));
            if !path.is_file() {
                wrong.push(format!("{}: {} not cached", entry.id, d.url));
                continue;
            }
            let source = Source::open(&path).unwrap();
            let found =
                analyze::analyze_in(&source, &path, Some(&entry.game), entry.root.as_deref())
                    .map_err(|e| e.to_string())
                    .and_then(|a| {
                        super::index_add::fingerprint(&source, &a).map_err(|e| e.to_string())
                    });
            let found = match found {
                Ok(Some(f)) => f,
                Ok(None) => {
                    wrong.push(format!(
                        "{}: {} has no files to know it by",
                        entry.id, d.url
                    ));
                    continue;
                }
                Err(err) => {
                    wrong.push(format!("{}: {err}", entry.id));
                    continue;
                }
            };
            if d.fingerprint.as_ref() == Some(&found) {
                continue;
            }
            if !write {
                wrong.push(format!(
                    "{}: {} has fingerprint {}",
                    entry.id, d.url, found.sha256
                ));
                continue;
            }
            let yaml_path = root.join("index/mods").join(format!("{}.yaml", entry.id));
            let yaml = fs::read_to_string(&yaml_path).unwrap();
            fs::write(&yaml_path, with_fingerprint(&yaml, &d.sha256, &found)).unwrap();
            written += 1;
        }
        // Each variant's, over its root as installed (§9.7).
        if entry.variants.is_empty() {
            continue;
        }
        let mut c = super::Curated::new(&entry, 0);
        c.part_files = entry.downloads[0]
            .parts
            .iter()
            .map(|(key, p)| {
                let path = cache.join(format!("{}.zip", p.sha256.to_ascii_lowercase()));
                (key.clone(), path)
            })
            .collect();
        let main = cache.join(format!(
            "{}.zip",
            entry.downloads[0].sha256.to_ascii_lowercase()
        ));
        let tmp = tempfile::tempdir().unwrap();
        let staged = super::variant_files::analyze_variants(&c, &main)
            .and_then(|v| super::variant_files::stage(&v, tmp.path()));
        if let Err(err) = staged {
            wrong.push(format!("{}: variants: {err}", entry.id));
            continue;
        }
        for v in &entry.variants {
            let dir = tmp.path().join("variants").join(&v.id);
            let found = match super::variant_files::fingerprint_of(&dir, &entry.game) {
                Ok(Some(f)) => f,
                Ok(None) => {
                    wrong.push(format!("{}: variant {} has no files", entry.id, v.id));
                    continue;
                }
                Err(err) => {
                    wrong.push(format!("{}: variant {}: {err}", entry.id, v.id));
                    continue;
                }
            };
            if v.fingerprint.as_ref() == Some(&found) {
                continue;
            }
            if !write {
                wrong.push(format!(
                    "{}: variant {} has fingerprint {}",
                    entry.id, v.id, found.sha256
                ));
                continue;
            }
            let yaml_path = root.join("index/mods").join(format!("{}.yaml", entry.id));
            let yaml = fs::read_to_string(&yaml_path).unwrap();
            fs::write(&yaml_path, with_variant_fingerprint(&yaml, &v.id, &found)).unwrap();
            written += 1;
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    eprintln!("{written} fingerprints written");
}

/// `yaml` with `f` as the fingerprint of the variant `id`, last in its
/// fields, in place of the one it had.
fn with_variant_fingerprint(yaml: &str, id: &str, f: &crate::mod_index::Fingerprint) -> String {
    let lines: Vec<&str> = yaml.lines().collect();
    let list = lines
        .iter()
        .position(|l| *l == "variants:")
        .expect("a variants list");
    let at = (list + 1..lines.len())
        .find(|&i| lines[i].trim() == format!("- id: {id}"))
        .expect("the variant's id line");
    let end = (at + 1..lines.len())
        .find(|&i| !lines[i].starts_with("    "))
        .unwrap_or(lines.len());
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < end {
        if i > at && lines[i].starts_with("    fingerprint:") {
            i += 1;
            while i < end && lines[i].starts_with("      ") {
                i += 1;
            }
            continue;
        }
        out.push(lines[i].to_string());
        i += 1;
    }
    out.push(super::index_add::fingerprint_yaml(f).trim_end().to_string());
    out.extend(lines[end..].iter().map(|l| l.to_string()));
    out.join("\n") + "\n"
}

/// `yaml` with `f` as the fingerprint of the download `sha256`, right
/// after its `size`, in place of the one it had.
fn with_fingerprint(yaml: &str, sha256: &str, f: &crate::mod_index::Fingerprint) -> String {
    let lines: Vec<&str> = yaml.lines().collect();
    let at = lines
        .iter()
        .position(|l| {
            l.trim()
                .strip_prefix("sha256:")
                .is_some_and(|h| h.trim().eq_ignore_ascii_case(sha256))
        })
        .expect("the download's sha256 line");
    let size = (at..lines.len())
        .find(|&i| lines[i].trim_start().starts_with("size:"))
        .expect("the download's size line");
    // The download's other fields, until the next download or field.
    let end = (size + 1..lines.len())
        .find(|&i| !lines[i].starts_with("    ") || lines[i].starts_with("  - "))
        .unwrap_or(lines.len());
    let mut out: Vec<String> = lines[..=size].iter().map(|l| l.to_string()).collect();
    out.push(super::index_add::fingerprint_yaml(f).trim_end().to_string());
    let mut i = size + 1;
    while i < end {
        if lines[i] == "    fingerprint:" {
            i += 1;
            while i < end && lines[i].starts_with("      ") {
                i += 1;
            }
            continue;
        }
        out.push(lines[i].to_string());
        i += 1;
    }
    out.extend(lines[end..].iter().map(|l| l.to_string()));
    out.join("\n") + "\n"
}

#[test]
fn variant_fingerprints_go_last_in_place_of_the_old() {
    let f = crate::mod_index::Fingerprint {
        files: vec!["LEVEL01.CK1".into()],
        sha256: "ab".repeat(32),
    };
    let yaml = "variants:\n  - id: hard\n    name: Hard\n    fingerprint: { sha256: old, files: [X] }\n    root: HARD\n  - id: easy\n    name: Easy\nminAppVersion: 1.0.0\n";
    let fp = format!(
        "    fingerprint:\n      sha256: {}\n      files: [LEVEL01.CK1]\n",
        "ab".repeat(32)
    );
    assert_eq!(
        with_variant_fingerprint(yaml, "hard", &f),
        format!(
            "variants:\n  - id: hard\n    name: Hard\n    root: HARD\n{fp}  - id: easy\n    name: Easy\nminAppVersion: 1.0.0\n"
        )
    );
    assert_eq!(
        with_variant_fingerprint(yaml, "easy", &f),
        format!(
            "variants:\n  - id: hard\n    name: Hard\n    fingerprint: {{ sha256: old, files: [X] }}\n    root: HARD\n  - id: easy\n    name: Easy\n{fp}minAppVersion: 1.0.0\n"
        )
    );
}

#[test]
fn fingerprints_go_after_the_size_in_place_of_the_old() {
    let f = crate::mod_index::Fingerprint {
        files: vec!["LEVEL01.CK4".into(), "1.PAT".into()],
        sha256: "ab".repeat(32),
    };
    let yaml = "downloads:\n  - url: https://a/b.zip\n    sha256: AA\n    size: 1\n    fingerprint:\n      sha256: old\n      files: [X]\n    programs:\n      A.EXE: cc\n  - url: https://a/c.zip\n    sha256: bb\n    size: 2\nrecipe: x\n";
    let want = format!(
        "downloads:\n  - url: https://a/b.zip\n    sha256: AA\n    size: 1\n    fingerprint:\n      sha256: {}\n      files: [LEVEL01.CK4, \"1.PAT\"]\n    programs:\n      A.EXE: cc\n  - url: https://a/c.zip\n    sha256: bb\n    size: 2\nrecipe: x\n",
        "ab".repeat(32)
    );
    assert_eq!(with_fingerprint(yaml, "aa", &f), want);
}

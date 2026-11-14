use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::detect::sha256_hex;
use crate::recipe::Recipe;

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
            let info = match inspect(&path, Some(&entry.game)) {
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

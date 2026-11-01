//! The curated mod index (PLAN.md §10.2): fetched from GitHub Pages, checked
//! against the embedded minisign keys, cached for offline use.
//!
//! The index decides what gets downloaded and run, so anything that isn't
//! signed by one of [`PUBLIC_KEYS`] is ignored and the cached copy stays.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use minisign_verify::{PublicKey, Signature};
use rusqlite::{params, params_from_iter};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::catalog;
use crate::db::Db;
use crate::detect::sha256_hex;
use crate::downloads::{self, USER_AGENT};
use crate::error::{CommandError, Error, Result};
use crate::library::now;
use crate::mods::{self, Curated};
use crate::paths::AppPaths;
use crate::recipe::{Recipe, VersionRecipes};

/// Where the published index lives (`index/` in the repository, built and
/// signed by `.github/workflows/index.yml`).
pub const DEFAULT_URL: &str = "https://transporthummel.github.io/BloogBase/index/index.json";

/// Keys whose signatures are accepted. Keep the old key here for a while
/// when rotating.
const PUBLIC_KEYS: &[&str] = &["RWSsNItW8utvDkwyXBRJitGgakWA1N2Lozafd+JqhOQ+SvdlMsmFNedJ"];

/// The newest `schemaVersion` this app understands.
const SCHEMA_VERSION: u32 = 1;

/// Between automatic checks; the server mostly answers 304 anyway.
pub const REFRESH_EVERY: Duration = Duration::from_secs(4 * 60 * 60);

/// Far more than the index will ever need.
const MAX_SIZE: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub id: String,
    pub name: String,
    /// Catalog game id.
    pub game: String,
    /// Catalog versions the mod works with; empty: any.
    pub requires: Vec<String>,
    pub authors: Vec<String>,
    /// Author name → their page, for the authors that have one.
    #[serde(default)]
    pub author_links: BTreeMap<String, String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub released: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
    /// When it joined the index, `YYYY-MM-DD`.
    pub added: String,
    #[serde(rename = "type")]
    pub kind: ModKind,
    /// In English.
    #[serde(default)]
    pub description: Option<String>,
    /// Language code → the texts in that language.
    #[serde(default)]
    pub translations: BTreeMap<String, Translation>,
    pub links: Links,
    /// Newest first.
    pub downloads: Vec<Download>,
    pub recipe: Recipe,
    /// Instead of `recipe` on these catalog versions.
    #[serde(default)]
    pub version_recipes: VersionRecipes,
    /// Folders in the mod's root whose files also go into the root.
    #[serde(default)]
    pub copy_to_root: Vec<String>,
    /// The index mod this one is played over, e.g. a level pack for a mod.
    #[serde(default)]
    pub extends: Option<String>,
    /// Files of its own that mods copied over it lack, e.g. a start batch.
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub min_app_version: Option<String>,
}

/// An entry's texts in another language.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct Translation {
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ModKind {
    Episode,
    Levelpack,
    Conversion,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct Links {
    #[serde(default)]
    pub keenwiki: Option<String>,
    #[serde(default)]
    pub pckf: Option<String>,
    /// The thread on the Keen: Modding forum.
    #[serde(default)]
    pub keenmodding: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Download {
    #[serde(default)]
    pub version: Option<String>,
    pub url: String,
    pub sha256: String,
    /// Bytes.
    pub size: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawIndex {
    schema_version: u32,
    generated_at: u64,
    /// Parsed one by one, so an entry this app doesn't understand only
    /// hides that entry.
    mods: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
struct Parsed {
    entries: Vec<IndexEntry>,
    /// Entries for a newer app.
    hidden: u32,
    generated_at: u64,
}

/// An index entry with what the library knows about it.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexMod {
    pub entry: IndexEntry,
    /// The installed mod made from this entry.
    pub mod_id: Option<String>,
    /// Installed, but the index has a different download now.
    pub update_available: bool,
    /// An update of the index brought it in the last [`NEW_FOR`] seconds.
    /// The first download of the index brings nothing new: everything is.
    pub new: bool,
}

/// How long an entry an update brought counts as new.
const NEW_FOR: i64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub url: String,
    /// Unix seconds of the last successful check.
    pub checked_at: Option<u32>,
    /// When the index in use was built.
    pub generated_at: Option<u32>,
    pub mod_count: u32,
    /// Entries only a newer BloogBase can show.
    pub hidden_count: u32,
    /// Why the last check failed; the cached index stays in use.
    pub error: Option<CommandError>,
}

/// Sent after every check of the index, successful or not.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct IndexUpdated(pub IndexStatus);

/// What [`ModIndex::refresh`] remembers between runs, next to the cached files.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheMeta {
    url: String,
    etag: Option<String>,
    checked_at: Option<i64>,
    /// Entry id → when an update brought it, while it counts as new.
    #[serde(default)]
    appeared: BTreeMap<String, i64>,
}

#[derive(Default)]
struct State {
    index: Option<Parsed>,
    meta: CacheMeta,
    error: Option<CommandError>,
    /// Last attempt in this run, successful or not.
    attempted: Option<Instant>,
}

pub struct ModIndex {
    url: String,
    keys: Vec<PublicKey>,
    dir: PathBuf,
    state: Mutex<State>,
    /// Held while checking, so concurrent refreshes wait for one result.
    refreshing: tokio::sync::Mutex<()>,
}

/// The index URL: `BLOOGBASE_INDEX_URL` if set (a URL or a local path);
/// in debug builds a locally built `index/dist/index.json`; else
/// [`DEFAULT_URL`].
pub fn configured_url() -> String {
    if let Some(url) = std::env::var("BLOOGBASE_INDEX_URL")
        .ok()
        .filter(|u| !u.is_empty())
    {
        return url;
    }
    if cfg!(debug_assertions) {
        let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("../index/dist/index.json");
        if local.is_file() {
            return local.display().to_string();
        }
    }
    DEFAULT_URL.into()
}

impl ModIndex {
    /// Uses the cached index, if any; call [`refresh`](Self::refresh) to check
    /// for a new one.
    pub fn open(paths: &AppPaths, url: String) -> Self {
        Self::with_keys(paths, url, PUBLIC_KEYS)
    }

    fn with_keys(paths: &AppPaths, url: String, keys: &[&str]) -> Self {
        let keys = keys
            .iter()
            .map(|k| PublicKey::from_base64(k).expect("embedded public key is valid"))
            .collect();
        let index = ModIndex {
            url,
            keys,
            dir: paths.cache.join("index"),
            state: Mutex::default(),
            refreshing: tokio::sync::Mutex::new(()),
        };
        let meta: CacheMeta = std::fs::read(index.dir.join("meta.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let cached = std::fs::read(index.dir.join("index.json")).and_then(|data| {
            std::fs::read_to_string(index.dir.join("index.json.minisig")).map(|sig| (data, sig))
        });
        let parsed = match cached {
            Ok((data, sig)) => index.verify(&data, &sig),
            Err(e) => Err(e.into()),
        };
        match parsed {
            Ok(p) => {
                tracing::info!(
                    "cached mod index from {}: {} mods",
                    meta.url,
                    p.entries.len()
                );
                index.lock().index = Some(p);
            }
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!("ignoring the cached mod index: {e}"),
        }
        index.lock().meta = meta;
        index
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> IndexStatus {
        let st = self.lock();
        IndexStatus {
            url: self.url.clone(),
            checked_at: st.meta.checked_at.map(|t| t as u32),
            generated_at: st.index.as_ref().map(|i| i.generated_at as u32),
            mod_count: st.index.as_ref().map_or(0, |i| i.entries.len() as u32),
            hidden_count: st.index.as_ref().map_or(0, |i| i.hidden),
            error: st.error.clone(),
        }
    }

    pub fn entries(&self) -> Vec<IndexEntry> {
        self.lock()
            .index
            .as_ref()
            .map(|i| i.entries.clone())
            .unwrap_or_default()
    }

    pub fn entry(&self, id: &str) -> Option<IndexEntry> {
        self.entries().into_iter().find(|e| e.id == id)
    }

    /// The entry whose newest download is the archive at `path`, so a mod
    /// added from a file gets the tested recipe. Older downloads don't
    /// count: the entry's recipe may not fit them.
    pub fn entry_for_archive(&self, path: &Path, game_id: &str) -> Option<IndexEntry> {
        if !path.is_file() {
            return None;
        }
        let entries = self.entries();
        if !entries.iter().any(|e| e.game == game_id) {
            return None;
        }
        let hash = sha256_hex(&std::fs::read(path).ok()?);
        entries.into_iter().find(|e| {
            e.game == game_id
                && e.downloads
                    .first()
                    .is_some_and(|d| d.sha256.eq_ignore_ascii_case(&hash))
        })
    }

    /// Checks for a new index, unless that happened less than
    /// [`REFRESH_EVERY`] ago (`force` checks anyway). Failures are kept in
    /// the status; the index in use never gets worse.
    pub async fn refresh(&self, force: bool) -> IndexStatus {
        let _guard = self.refreshing.lock().await;
        let recent = self
            .lock()
            .attempted
            .is_some_and(|t| t.elapsed() < REFRESH_EVERY);
        if recent && !force {
            return self.status();
        }
        self.lock().attempted = Some(Instant::now());
        match self.fetch().await {
            Ok(changed) => {
                if changed {
                    tracing::info!("mod index updated from {}", self.url);
                }
                self.lock().error = None;
            }
            Err(e) => {
                tracing::warn!("mod index check failed: {e}");
                self.lock().error = Some(CommandError::from_quiet(e));
            }
        }
        self.status()
    }

    /// Downloads and stores a new index. `Ok(false)`: nothing new.
    async fn fetch(&self) -> Result<bool> {
        let etag = {
            let st = self.lock();
            st.meta
                .etag
                .clone()
                .filter(|_| st.meta.url == self.url && st.index.is_some())
        };
        let Some((data, sig, etag)) = download(&self.url, etag.as_deref()).await? else {
            self.store_meta(None, false)?;
            return Ok(false);
        };
        let parsed = self.verify(&data, &sig)?;
        let changed = {
            let st = self.lock();
            if let Some(current) = &st.index
                && parsed.generated_at < current.generated_at
            {
                // A replayed old (but validly signed) index.
                return Err(Error::UnexpectedContent(format!(
                    "the mod index from {} is older than the one in use",
                    self.url
                )));
            }
            st.index.as_ref() != Some(&parsed)
        };
        std::fs::create_dir_all(&self.dir)?;
        write_atomic(&self.dir.join("index.json"), &data)?;
        write_atomic(&self.dir.join("index.json.minisig"), sig.as_bytes())?;
        {
            let mut st = self.lock();
            let t = now();
            // Not on the first download: with nothing before, all would be new.
            let before: Option<Vec<String>> = st
                .index
                .as_ref()
                .filter(|i| !i.entries.is_empty())
                .map(|i| i.entries.iter().map(|e| e.id.clone()).collect());
            if let Some(before) = before {
                for e in &parsed.entries {
                    if !before.contains(&e.id) {
                        st.meta.appeared.entry(e.id.clone()).or_insert(t);
                    }
                }
            }
            st.meta
                .appeared
                .retain(|id, at| t - *at < NEW_FOR && parsed.entries.iter().any(|e| &e.id == id));
            st.index = Some(parsed);
        }
        self.store_meta(etag, true)?;
        Ok(changed)
    }

    fn store_meta(&self, etag: Option<String>, replace_etag: bool) -> Result<()> {
        let json = {
            let mut st = self.lock();
            st.meta.url = self.url.clone();
            if replace_etag {
                st.meta.etag = etag;
            }
            st.meta.checked_at = Some(now());
            serde_json::to_vec_pretty(&st.meta).expect("meta serializes")
        };
        std::fs::create_dir_all(&self.dir)?;
        write_atomic(&self.dir.join("meta.json"), &json)
    }

    fn verify(&self, data: &[u8], sig: &str) -> Result<Parsed> {
        let sig = Signature::decode(sig).map_err(|e| Error::BadSignature(e.to_string()))?;
        let valid = self
            .keys
            .iter()
            .any(|k| k.verify(data, &sig, false).is_ok());
        if !valid {
            return Err(Error::BadSignature("not signed by a trusted key".into()));
        }
        parse(data)
    }
}

/// `(data, signature, etag)`, or `None` when the server says nothing changed.
async fn download(
    url: &str,
    etag: Option<&str>,
) -> Result<Option<(Vec<u8>, String, Option<String>)>> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        let path = Path::new(url);
        let data = std::fs::read(path)?;
        let sig = std::fs::read_to_string(format!("{url}.minisig"))?;
        return Ok(Some((data, sig, None)));
    }
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .build()?;
    let mut req = client.get(url);
    if let Some(etag) = etag {
        req = req.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    let resp = req.send().await?;
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(Error::HttpStatus(resp.status().as_u16()));
    }
    let new_etag = resp
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let data = resp.bytes().await?;
    if data.len() > MAX_SIZE {
        return Err(Error::UnexpectedContent(
            "the mod index is too large".into(),
        ));
    }
    let sig_resp = client.get(format!("{url}.minisig")).send().await?;
    if !sig_resp.status().is_success() {
        return Err(Error::HttpStatus(sig_resp.status().as_u16()));
    }
    let sig = sig_resp.text().await?;
    Ok(Some((data.to_vec(), sig, new_etag)))
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn parse(data: &[u8]) -> Result<Parsed> {
    let raw: RawIndex = serde_json::from_slice(data)
        .map_err(|e| Error::UnexpectedContent(format!("mod index: {e}")))?;
    let total = raw.mods.len() as u32;
    if raw.schema_version > SCHEMA_VERSION {
        return Ok(Parsed {
            entries: Vec::new(),
            hidden: total,
            generated_at: raw.generated_at,
        });
    }
    let app = env!("CARGO_PKG_VERSION");
    let entries: Vec<IndexEntry> = raw
        .mods
        .into_iter()
        .filter_map(|v| {
            let id = v
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("?")
                .to_string();
            match serde_json::from_value::<IndexEntry>(v) {
                Ok(e) if usable(&e, app) => Some(e),
                Ok(_) => None,
                Err(err) => {
                    tracing::debug!("index entry {id} not understood: {err}");
                    None
                }
            }
        })
        .collect();
    Ok(Parsed {
        hidden: total - entries.len() as u32,
        entries,
        generated_at: raw.generated_at,
    })
}

fn usable(e: &IndexEntry, app_version: &str) -> bool {
    !e.downloads.is_empty()
        && catalog::game(&e.game).is_some()
        && e.min_app_version
            .as_deref()
            .is_none_or(|min| version_at_least(app_version, min))
}

/// `1.10.0` ≥ `1.9.2`; pre-release suffixes are ignored.
fn version_at_least(version: &str, min: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.split(['-', '+'])
            .next()
            .unwrap_or("")
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let (mut a, mut b) = (parts(version), parts(min));
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    a >= b
}

/// Ties installed mods that aren't tied to an entry to the one whose
/// download they were installed from, byte for byte: added from KeenWiki
/// or a file, maybe before the entry joined the index. An older download
/// counts too, so the entry offers the update. The entry's details replace
/// what the mod brought along ([`mods::tie_to_index`]).
pub fn link_installed(index: &ModIndex, db: &Db) -> Result<()> {
    for e in index.entries() {
        for (i, d) in e.downloads.iter().enumerate() {
            let found: Vec<String> = db
                .conn()
                .prepare(
                    "SELECT id FROM mods WHERE index_id IS NULL AND game_id = ?1
                     AND lower(archive_sha256) = lower(?2)",
                )?
                .query_map(params![e.game, d.sha256], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for mod_id in found {
                mods::tie_to_index(db, &mod_id, &Curated::new(&e, i))?;
            }
        }
    }
    Ok(())
}

/// The index entries with their install state.
pub fn list(index: &ModIndex, db: &Db) -> Result<Vec<IndexMod>> {
    let entries = index.entries();
    let appeared = index.lock().meta.appeared.clone();
    let t = now();
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    let installed: Vec<(String, String, Option<String>)> = {
        let conn = db.conn();
        let marks = vec!["?"; ids.len()].join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT index_id, id, archive_sha256 FROM mods WHERE index_id IN ({marks})
             ORDER BY added_at"
        ))?;
        stmt.query_map(params_from_iter(&ids), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?
    };
    Ok(entries
        .into_iter()
        .map(|entry| {
            let found = installed.iter().find(|(id, ..)| *id == entry.id);
            IndexMod {
                new: appeared.get(&entry.id).is_some_and(|at| t - at < NEW_FOR),
                mod_id: found.map(|(_, m, _)| m.clone()),
                update_available: found.is_some_and(|(.., sha)| {
                    sha.as_deref()
                        .is_none_or(|s| !s.eq_ignore_ascii_case(&entry.downloads[0].sha256))
                }),
                entry,
            }
        })
        .collect())
}

/// The archive an index entry installs from. A download goes again when
/// this is dropped; the player's own copy stays.
pub struct Archive {
    pub path: PathBuf,
    /// In bytes, as the index tells.
    pub size: u64,
    downloaded: bool,
}

impl Drop for Archive {
    fn drop(&mut self) {
        if self.downloaded {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// The player's own copy of `entry`'s download if they picked one (it must
/// be that file), else the download, fetched into `cache`.
pub async fn archive(
    cache: &Path,
    entry: &IndexEntry,
    own: Option<PathBuf>,
    progress: &impl Fn(u64, u64),
) -> Result<Archive> {
    let download = &entry.downloads[0];
    if let Some(path) = own {
        let hash = sha256_hex(&tokio::fs::read(&path).await?);
        if !hash.eq_ignore_ascii_case(&download.sha256) {
            return Err(Error::UnexpectedContent(format!(
                "{} is not the download of {}",
                path.display(),
                entry.id
            )));
        }
        return Ok(Archive {
            path,
            size: u64::from(download.size),
            downloaded: false,
        });
    }
    let path = cache
        .join("downloads")
        .join(format!("index-{}.zip", entry.id));
    downloads::fetch(
        &download.url,
        &download.sha256,
        u64::from(download.size),
        &path,
        progress,
    )
    .await?;
    Ok(Archive {
        path,
        size: u64::from(download.size),
        downloaded: true,
    })
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    const SAMPLE: &str = include_str!("../tests/index-sample.json");

    struct Keys {
        sk: minisign::SecretKey,
        pk: String,
    }

    fn keys() -> Keys {
        let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        Keys {
            pk: kp.pk.to_base64(),
            sk: kp.sk,
        }
    }

    fn sign(k: &Keys, data: &str) -> String {
        minisign::sign(None, &k.sk, std::io::Cursor::new(data), None, None)
            .unwrap()
            .into_string()
    }

    fn setup() -> (tempfile::TempDir, AppPaths) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        (tmp, paths)
    }

    /// Writes a signed index next to the data folder; returns its path.
    fn publish(tmp: &Path, k: &Keys, json: &str) -> String {
        let file = tmp.join("index.json");
        std::fs::write(&file, json).unwrap();
        std::fs::write(tmp.join("index.json.minisig"), sign(k, json)).unwrap();
        file.display().to_string()
    }

    #[test]
    fn embedded_keys_are_valid() {
        for k in PUBLIC_KEYS {
            PublicKey::from_base64(k).unwrap();
        }
    }

    #[test]
    fn parses_the_sample_and_hides_entries_for_newer_apps() {
        let p = parse(SAMPLE.as_bytes()).unwrap();
        // `sample-levels` needs app 9.0.0.
        assert_eq!(p.hidden, 1);
        assert_eq!(p.entries.len(), 1);
        let e = &p.entries[0];
        assert_eq!(e.id, "sample-patch");
        assert_eq!(e.kind, ModKind::Episode);
        assert_eq!(e.requires, ["keen4-1.4-ega"]);
        assert_eq!(
            e.translations["de"].description.as_deref(),
            Some("Eine ausgedachte Mod für Tests.")
        );
        assert!(matches!(e.recipe, Recipe::Patch { .. }));
        assert_eq!(e.downloads[0].size, 1234);

        let newer = SAMPLE.replace(r#""type": "levelpack""#, r#""type": "minigame""#);
        let newer = newer.replace("9.0.0", "0.0.1");
        assert_eq!(parse(newer.as_bytes()).unwrap().hidden, 1);
        let future = SAMPLE.replace(r#""schemaVersion": 1"#, r#""schemaVersion": 2"#);
        let p = parse(future.as_bytes()).unwrap();
        assert_eq!((p.entries.len(), p.hidden), (0, 2));
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(version_at_least("0.1.0", "0.1.0"));
        assert!(version_at_least("1.10.0", "1.9.2"));
        assert!(!version_at_least("0.1.0", "0.2"));
        assert!(version_at_least("1.0.0-beta", "1.0"));
    }

    #[tokio::test]
    async fn accepts_signed_and_caches_it() {
        let (tmp, paths) = setup();
        let k = keys();
        let url = publish(tmp.path(), &k, SAMPLE);
        let index = ModIndex::with_keys(&paths, url.clone(), &[&k.pk]);
        assert_eq!(index.status().mod_count, 0);
        let st = index.refresh(false).await;
        assert!(st.error.is_none(), "{:?}", st.error);
        assert_eq!((st.mod_count, st.hidden_count), (1, 1));
        assert_eq!(st.generated_at, Some(1_760_000_000));
        assert!(st.checked_at.is_some());

        // Offline next start: the cache is used.
        std::fs::remove_file(&url).unwrap();
        let again = ModIndex::with_keys(&paths, url, &[&k.pk]);
        assert_eq!(again.status().mod_count, 1);
        let st = again.refresh(true).await;
        assert!(st.error.is_some());
        assert_eq!(st.mod_count, 1);
    }

    #[tokio::test]
    async fn finds_the_entry_of_a_downloaded_archive() {
        let (tmp, paths) = setup();
        let k = keys();
        let zip = tmp.path().join("copy.zip");
        std::fs::write(&zip, b"the download").unwrap();
        let hash = sha256_hex(b"the download");
        let json = SAMPLE.replace(&"1".repeat(64), &hash.to_uppercase());
        let url = publish(tmp.path(), &k, &json);
        let index = ModIndex::with_keys(&paths, url, &[&k.pk]);
        index.refresh(false).await;

        let found = |path: &Path, game: &str| index.entry_for_archive(path, game).map(|e| e.id);
        assert_eq!(found(&zip, "keen4").as_deref(), Some("sample-patch"));
        // Another game's mod, another file or a folder: no match.
        assert_eq!(found(&zip, "keen5"), None);
        std::fs::write(tmp.path().join("other.zip"), b"other").unwrap();
        assert_eq!(found(&tmp.path().join("other.zip"), "keen4"), None);
        assert_eq!(found(tmp.path(), "keen4"), None);
    }

    #[tokio::test]
    async fn rejects_bad_signatures_and_older_indexes() {
        let (tmp, paths) = setup();
        let k = keys();
        let other = keys();
        let url = publish(tmp.path(), &other, SAMPLE);
        let index = ModIndex::with_keys(&paths, url.clone(), &[&k.pk]);
        let st = index.refresh(false).await;
        assert_eq!(
            st.error.unwrap().kind,
            crate::error::ErrorKind::BadSignature
        );
        assert_eq!(st.mod_count, 0);

        // Tampered after signing.
        std::fs::write(tmp.path().join("index.json.minisig"), sign(&k, SAMPLE)).unwrap();
        std::fs::write(&url, SAMPLE.replace("Sample Patch Mod", "Evil")).unwrap();
        assert!(index.refresh(true).await.error.is_some());

        publish(tmp.path(), &k, SAMPLE);
        assert!(index.refresh(true).await.error.is_none());
        let old = SAMPLE.replace("1760000000", "1700000000");
        publish(tmp.path(), &k, &old);
        let st = index.refresh(true).await;
        assert!(st.error.is_some());
        assert_eq!(st.generated_at, Some(1_760_000_000));

        // A cached file that was changed on disk is ignored.
        std::fs::write(
            paths.cache.join("index/index.json"),
            SAMPLE.replace("Sample Patch Mod", "Evil"),
        )
        .unwrap();
        let reopened = ModIndex::with_keys(&paths, url, &[&k.pk]);
        assert_eq!(reopened.status().mod_count, 0);
    }

    /// Serves the index over HTTP with an ETag; counts full responses.
    /// Answers one request per connection and says so: a client reusing a
    /// connection the server had already closed failed now and then.
    fn serve(json: String, sig: String) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/index.json", listener.local_addr().unwrap());
        let full = Arc::new(AtomicUsize::new(0));
        let count = full.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut cached = false;
                loop {
                    let mut h = String::new();
                    reader.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                    cached |= h.to_ascii_lowercase().starts_with("if-none-match: \"v1\"");
                }
                let response = if line.contains(".minisig") {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sig}",
                        sig.len()
                    )
                } else if cached {
                    "HTTP/1.1 304 Not Modified\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n"
                        .to_string()
                } else {
                    count.fetch_add(1, Ordering::SeqCst);
                    format!(
                        "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                        json.len()
                    )
                };
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (url, full)
    }

    #[tokio::test]
    async fn uses_etags_and_waits_between_checks() {
        let (_tmp, paths) = setup();
        let k = keys();
        let (url, full) = serve(SAMPLE.into(), sign(&k, SAMPLE));
        let index = ModIndex::with_keys(&paths, url, &[&k.pk]);
        assert!(index.refresh(false).await.error.is_none());
        assert_eq!(full.load(Ordering::SeqCst), 1);
        // Too soon: no request at all.
        index.refresh(false).await;
        // Forced: a conditional request, answered with 304.
        let st = index.refresh(true).await;
        assert!(st.error.is_none(), "{:?}", st.error);
        assert_eq!(full.load(Ordering::SeqCst), 1);
        assert_eq!(st.mod_count, 1);
    }

    #[tokio::test]
    async fn marks_entries_an_update_brought_as_new() {
        let (tmp, paths) = setup();
        let k = keys();
        let url = publish(tmp.path(), &k, SAMPLE);
        let index = ModIndex::with_keys(&paths, url.clone(), &[&k.pk]);
        let db = Db::open_in_memory().unwrap();
        // The first download: nothing is new.
        index.refresh(true).await;
        assert!(!list(&index, &db).unwrap()[0].new);

        // An update with another entry: only that one is.
        let more = SAMPLE
            .replace(
                r#""generatedAt": 1760000000"#,
                r#""generatedAt": 1760000001"#,
            )
            .replace(r#""minAppVersion": "9.0.0""#, r#""minAppVersion": "0.0.1""#);
        publish(tmp.path(), &k, &more);
        let st = index.refresh(true).await;
        assert!(st.error.is_none(), "{:?}", st.error);
        let is_new = |index: &ModIndex, id: &str| {
            list(index, &db)
                .unwrap()
                .into_iter()
                .find(|m| m.entry.id == id)
                .unwrap()
                .new
        };
        assert!(is_new(&index, "sample-levels"));
        assert!(!is_new(&index, "sample-patch"));

        // Remembered across starts.
        let again = ModIndex::with_keys(&paths, url, &[&k.pk]);
        assert!(is_new(&again, "sample-levels"));
    }

    #[test]
    fn lists_install_state() {
        let (tmp, paths) = setup();
        let k = keys();
        let url = publish(tmp.path(), &k, SAMPLE);
        let index = ModIndex::with_keys(&paths, url, &[&k.pk]);
        tauri::async_runtime::block_on(index.refresh(false));
        let db = Db::open_in_memory().unwrap();
        let mods = list(&index, &db).unwrap();
        assert_eq!(mods.len(), 1);
        assert!(mods[0].mod_id.is_none() && !mods[0].update_available);

        db.conn()
            .execute(
                "INSERT INTO mods (id, game_id, index_id, name, authors, recipe_json, path,
                                   source, archive_sha256, added_at)
                 VALUES ('m', 'keen4', 'sample-patch', 'x', '[]', '{}', '/x', 'index', 'ab', 0)",
                [],
            )
            .unwrap();
        let mods = list(&index, &db).unwrap();
        assert_eq!(mods[0].mod_id.as_deref(), Some("m"));
        assert!(mods[0].update_available);
        let sha = &mods[0].entry.downloads[0].sha256;
        db.conn()
            .execute("UPDATE mods SET archive_sha256 = ?1", [sha])
            .unwrap();
        assert!(!list(&index, &db).unwrap()[0].update_available);
    }

    #[test]
    fn ties_installed_mods_to_the_entry_of_their_download() {
        let (tmp, paths) = setup();
        let k = keys();
        let url = publish(tmp.path(), &k, SAMPLE);
        let index = ModIndex::with_keys(&paths, url, &[&k.pk]);
        tauri::async_runtime::block_on(index.refresh(false));
        let db = Db::open_in_memory().unwrap();
        let sha = index.entry("sample-patch").unwrap().downloads[0]
            .sha256
            .clone();
        // Added from KeenWiki, the same download; another game; another
        // download; already tied to another entry.
        for (id, game, index_id, sha) in [
            ("wiki", "keen4", None, sha.to_uppercase()),
            ("k5", "keen5", None, sha.clone()),
            ("other", "keen4", None, "ab".into()),
            ("tied", "keen4", Some("elsewhere"), sha.clone()),
        ] {
            db.conn()
                .execute(
                    "INSERT INTO mods (id, game_id, index_id, name, authors, recipe_json, path,
                                       source, archive_sha256, added_at, description,
                                       version, pckf)
                     VALUES (?1, ?2, ?3, 'x', '[\"Wiki\"]', '{}', '/x', 'keenwiki', ?4, 0,
                             'From the wiki.', '1.0', 'https://pckf.com/wiki')",
                    params![id, game, index_id, sha],
                )
                .unwrap();
        }
        link_installed(&index, &db).unwrap();
        let tied: Vec<(String, Option<String>)> = db
            .conn()
            .prepare("SELECT id, index_id FROM mods ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let of = |id: &str| tied.iter().find(|(m, _)| m == id).unwrap().1.clone();
        assert_eq!(of("wiki").as_deref(), Some("sample-patch"));
        assert_eq!(of("k5"), None);
        assert_eq!(of("other"), None);
        assert_eq!(of("tied").as_deref(), Some("elsewhere"));
        assert_eq!(
            list(&index, &db).unwrap()[0].mod_id.as_deref(),
            Some("wiki")
        );
        // The entry's details replace the wiki's; untied mods keep theirs.
        let details = |id: &str| -> (
            String,
            String,
            String,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
        ) {
            db.conn()
                .query_row(
                    "SELECT source, name, authors, description, version, keenwiki_url, pckf
                     FROM mods WHERE id = ?1",
                    [id],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    },
                )
                .unwrap()
        };
        assert_eq!(
            details("wiki"),
            (
                "index".into(),
                "Sample Patch Mod".into(),
                r#"["Someone","Someone Else"]"#.into(),
                Some("A made-up mod for tests.".into()),
                "1.2".into(),
                Some("https://keenwiki.shikadi.net/wiki/Sample".into()),
                None,
            )
        );
        assert_eq!(details("other").1, "x");
        assert_eq!(details("tied").0, "keenwiki");
    }

    fn sample_entry(sha256: &str) -> IndexEntry {
        let mut index: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        let mut entry = index["mods"][0].take();
        entry["downloads"][0]["sha256"] = sha256.into();
        serde_json::from_value(entry).unwrap()
    }

    #[tokio::test]
    async fn a_download_goes_again_once_installed() {
        const BODY: &[u8] = b"the archive";
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            BODY.len()
        );
        let url = crate::downloads::tests::serve(vec![[head.as_bytes(), BODY].concat()]);
        let tmp = tempfile::tempdir().unwrap();
        let mut entry = sample_entry(&sha256_hex(BODY));
        entry.downloads[0].url = url;
        entry.downloads[0].size = BODY.len() as u32;
        let progress = AtomicUsize::new(0);
        let got = archive(tmp.path(), &entry, None, &|_, _| {
            progress.fetch_add(1, Ordering::SeqCst);
        })
        .await
        .unwrap();
        let path = got.path.clone();
        assert_eq!(
            path,
            tmp.path().join("downloads").join("index-sample-levels.zip")
        );
        assert_eq!(std::fs::read(&path).unwrap(), BODY);
        assert!(progress.load(Ordering::SeqCst) > 0);
        drop(got);
        assert!(!path.exists(), "the download goes once installed");
    }

    #[tokio::test]
    async fn installs_from_the_players_own_copy_only_if_it_is_the_download() {
        let tmp = tempfile::tempdir().unwrap();
        let own = tmp.path().join("levels.zip");
        std::fs::write(&own, b"the archive").unwrap();
        let entry = sample_entry(&sha256_hex(b"the archive").to_uppercase());
        let got = archive(tmp.path(), &entry, Some(own.clone()), &|_, _| {})
            .await
            .unwrap();
        assert_eq!(got.path, own);
        drop(got);
        assert!(own.exists(), "the player's copy stays");

        let other = sample_entry(&sha256_hex(b"another archive"));
        let err = archive(tmp.path(), &other, Some(own.clone()), &|_, _| {}).await;
        assert!(matches!(err, Err(Error::UnexpectedContent(_))));
    }
}

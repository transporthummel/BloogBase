//! The curated mod index (PLAN.md §10.2): fetched from GitHub Pages, checked
//! against the embedded minisign keys, cached for offline use. Release builds
//! carry the index published when they were built ([`SEED`]), so mods can be
//! found before the first check succeeds.
//!
//! The index decides what gets downloaded and run, so anything that isn't
//! signed by one of [`PUBLIC_KEYS`] is ignored and the cached copy stays.

use std::collections::{BTreeMap, HashMap};
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
use crate::launcher::Running;
use crate::library::now;
use crate::mods::{self, Curated};
use crate::paths::AppPaths;
use crate::recipe::{Recipe, VersionRecipes};
use crate::source::{Source, split_dir};

/// Where the published index lives (`index/` in the repository, built and
/// signed by `.github/workflows/index.yml`).
pub const DEFAULT_URL: &str = "https://transporthummel.github.io/BloogBase/index/index.json";

/// Keys whose signatures are accepted. Keep the old key here for a while
/// when rotating.
const PUBLIC_KEYS: &[&str] = &["RWSsNItW8utvDkwyXBRJitGgakWA1N2Lozafd+JqhOQ+SvdlMsmFNedJ"];

/// The index at [`DEFAULT_URL`] when this build was made, with its signature:
/// `src-tauri/seed/`, filled by `release.yml` (`build.rs`). Empty in builds
/// without it. Checked like a download; used while the cache has nothing
/// newer.
const SEED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/seed-index.json"));
const SEED_SIG: &str = include_str!(concat!(env!("OUT_DIR"), "/seed-index.json.minisig"));

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
    /// How far along the mod is, from its KeenWiki page.
    #[serde(default)]
    pub status: ModStatus,
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
    /// The program whose title screen is the cover, where the recipe runs a
    /// menu starting several.
    #[serde(default)]
    pub cover: Option<String>,
    /// Ships its whole game (a source mod) and plays without a copy of
    /// `game`, the game it's built on. Older apps ignore it and play it on
    /// that game.
    #[serde(default)]
    pub standalone: bool,
    /// With `standalone`: still plays from its own files alone, but needs
    /// a copy of `game` like any other mod. The app decides it, not the
    /// index: every standalone mod of a game without free shareware (see
    /// [`IndexEntry::needs_no_game`]); the index carries it for apps before
    /// 1.3.0, and [`parse`] overwrites it. Apps before 1.2.0 ignore it.
    #[serde(default)]
    pub requires_game: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub featured: bool,
    /// For mods that generate their levels at every start: how to keep
    /// them as worlds.
    #[serde(default)]
    pub worlds: Option<crate::worlds::Worlds>,
    /// The folder of the archive the mod is installed from, `""` for its
    /// root; `None`: the app finds it.
    #[serde(default)]
    pub root: Option<String>,
    /// Folders right in the mod's root the game keeps saves and settings
    /// in, besides the root.
    #[serde(default)]
    pub save_dirs: Vec<SaveDir>,
    /// The cover is the title screen of a folder of the mod, with its patch.
    #[serde(default)]
    pub cover_from: Option<CoverFrom>,
    /// Ways to play the mod, the player chooses one; empty for most.
    #[serde(default)]
    pub variants: Vec<Variant>,
    #[serde(default)]
    pub min_app_version: Option<String>,
}

/// One way to play an index mod (PLAN.md §9.7): a folder of its archive or
/// of one of the download's `parts`, with what's started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    /// Stable: saves, backups and the profile keep it.
    pub id: String,
    /// Preselected and played until the player picks another.
    #[serde(default)]
    pub default: bool,
    /// In English.
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Language code → the texts in that language.
    #[serde(default)]
    pub translations: BTreeMap<String, VariantTranslation>,
    /// A key of the newest download's `parts`; `None`: the main archive.
    #[serde(default)]
    pub archive: Option<String>,
    /// The folder of its archive, `""` for its root; `None`: the entry's
    /// `root` for the main archive, else the one holding the recipe's files.
    #[serde(default)]
    pub root: Option<String>,
    /// In place of the entry's recipe and version recipes.
    #[serde(default)]
    pub recipe: Option<Recipe>,
    /// In place of the entry's `copyToRoot`.
    #[serde(default)]
    pub copy_to_root: Option<Vec<String>>,
    /// Name in the game → the file below the root it's a copy of.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    /// In place of the entry's cover program.
    #[serde(default)]
    pub cover: Option<String>,
    /// The files of its root as installed, as a download's.
    #[serde(default)]
    pub fingerprint: Option<Fingerprint>,
}

impl Variant {
    /// Its name in `language` (`de`, `de-DE`, …) where translated, else in
    /// English.
    pub fn name_in(&self, language: Option<&str>) -> &str {
        language
            .and_then(|l| self.translations.get(l.split('-').next().unwrap_or(l)))
            .map_or(&self.name, |t| &t.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct VariantTranslation {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// A further archive of a download's version, which variants are
/// installed from; downloaded with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Part {
    pub url: String,
    pub sha256: String,
    /// Bytes.
    pub size: u32,
}

/// A folder right in a mod's root the game keeps saves and settings in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveDir {
    pub dir: String,
    /// In English.
    pub name: String,
    /// Installs from before the entry named its root kept this folder's
    /// files as their root.
    #[serde(default)]
    pub old_root: bool,
    /// Language code → the name in that language.
    #[serde(default)]
    pub translations: BTreeMap<String, SaveDirTranslation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SaveDirTranslation {
    pub name: String,
}

/// The cover of a mod whose root only holds a launcher: the title screen
/// of `dir`, played with `patch`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct CoverFrom {
    pub dir: String,
    pub patch: String,
}

impl IndexEntry {
    /// Plays without a copy of its game: standalone, for a game whose
    /// shareware is free (decided 2026-11-14: a licence question, so the
    /// app's call, not the index's).
    pub fn needs_no_game(&self) -> bool {
        self.standalone && crate::free_games::has_shareware(&self.game)
    }
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

/// The "Status" of a mod's KeenWiki page; the index leaves out `complete`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ModStatus {
    #[default]
    Complete,
    Alpha,
    Beta,
    Abandoned,
    /// One a newer index knows: shown like `complete` rather than hiding
    /// the entry.
    #[serde(other)]
    Other,
}

impl ModStatus {
    /// The status a KeenWiki page's "Status" (`Complete`, `Beta`) names.
    pub fn from_page(status: &str) -> Self {
        match status.trim().to_ascii_lowercase().as_str() {
            "complete" => Self::Complete,
            "alpha" => Self::Alpha,
            "beta" => Self::Beta,
            "abandoned" => Self::Abandoned,
            _ => Self::Other,
        }
    }

    /// As stored and in the index; `None` for [`ModStatus::Other`].
    pub fn as_str(self) -> Option<&'static str> {
        match self {
            Self::Complete => Some("complete"),
            Self::Alpha => Some("alpha"),
            Self::Beta => Some("beta"),
            Self::Abandoned => Some("abandoned"),
            Self::Other => None,
        }
    }

    /// A page's status as stored, unless the app doesn't know it.
    pub fn stored(page: Option<&str>) -> Option<Self> {
        page.map(Self::from_page).filter(|s| *s != Self::Other)
    }
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
    /// How an older download starts, when unlike the newest (`recipe`).
    #[serde(default)]
    pub recipe: Option<Recipe>,
    /// File name → SHA-256 of the programs it runs, by which a standalone
    /// mod is known in the player's own folder ([`download_for_files`]).
    #[serde(default)]
    pub programs: BTreeMap<String, String>,
    /// The files that make this version, by which the mod is known in a
    /// player's own folder or ZIP and among the mods installed by hand.
    #[serde(default)]
    pub fingerprint: Option<Fingerprint>,
    /// Key → further archives of this version (the newest download's).
    #[serde(default)]
    pub parts: BTreeMap<String, Part>,
}

/// The files of a download's mod root that make its version (levels,
/// graphics, sounds, patches, programs; no documents, saves or settings),
/// and one SHA-256 over them ([`fingerprint_sha256`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Fingerprint {
    pub files: Vec<String>,
    pub sha256: String,
}

/// The SHA-256 of a [`Fingerprint`]: over a line `NAME sha256` for each of
/// its files (name upper case, SHA-256 lower case), sorted by name. The
/// index tool computes it the same way.
pub fn fingerprint_sha256(files: &[(String, String)]) -> String {
    let mut lines: Vec<String> = files
        .iter()
        .map(|(name, sha)| {
            format!(
                "{} {}\n",
                name.to_ascii_uppercase(),
                sha.to_ascii_lowercase()
            )
        })
        .collect();
    lines.sort();
    sha256_hex(lines.concat().as_bytes())
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
    /// `index` is [`SEED`], not the cached download.
    from_seed: bool,
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
        let seed = (url == DEFAULT_URL && !SEED.is_empty()).then_some((SEED, SEED_SIG));
        Self::with_seed(paths, url, PUBLIC_KEYS, seed)
    }

    #[cfg(test)]
    fn with_keys(paths: &AppPaths, url: String, keys: &[&str]) -> Self {
        Self::with_seed(paths, url, keys, None)
    }

    /// `seed`: an index from `url`, used when the cache has none as new.
    fn with_seed(
        paths: &AppPaths,
        url: String,
        keys: &[&str],
        seed: Option<(&[u8], &str)>,
    ) -> Self {
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
        let cached = match parsed {
            Ok(p) => Some(p),
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                tracing::warn!("ignoring the cached mod index: {e}");
                None
            }
        };
        let seed = seed.and_then(|(data, sig)| match index.verify(data, sig) {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!("ignoring the bundled mod index: {e}");
                None
            }
        });
        let use_seed = match (&cached, &seed) {
            (_, None) => false,
            (None, Some(_)) => true,
            // A cache from another place (a debug build's local index) says
            // nothing about this one.
            (Some(c), Some(s)) => meta.url != index.url || s.generated_at > c.generated_at,
        };
        {
            let mut st = index.lock();
            if use_seed {
                let s = seed.expect("checked above");
                tracing::info!("bundled mod index: {} mods", s.entries.len());
                st.index = Some(s);
                st.from_seed = true;
            } else if let Some(c) = cached {
                tracing::info!(
                    "cached mod index from {}: {} mods",
                    meta.url,
                    c.entries.len()
                );
                st.index = Some(c);
            }
            st.meta = meta;
        }
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

    /// The entry with the download whose files are in `source`, byte for
    /// byte ([`download_for_files`]): a mod the player has on disk, unpacked
    /// or repacked, maybe an older version and with saves. Newest download
    /// first.
    pub fn entry_for_files(&self, source: &Source) -> Option<(IndexEntry, usize)> {
        let entries: Vec<IndexEntry> = self
            .entries()
            .into_iter()
            .filter(|e| e.downloads.iter().any(Download::known_by_files))
            .collect();
        if entries.is_empty() {
            return None;
        }
        let files = source.files().ok()?;
        let mut hashes = HashMap::new();
        entries.into_iter().find_map(|e| {
            let i = download_for_files(&e, source, &files, &mut hashes)?;
            Some((e, i))
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
                .filter(|_| st.meta.url == self.url && st.index.is_some() && !st.from_seed)
        };
        let Some((data, sig, etag)) = download(&self.url, etag.as_deref()).await? else {
            self.store_meta(None, false)?;
            return Ok(false);
        };
        let parsed = self.verify(&data, &sig)?;
        let changed = {
            let st = self.lock();
            // Only against an index from the same place: switching to another
            // one (a debug build's local index) may well go back in time.
            if let Some(current) = &st.index
                && (st.meta.url == self.url || st.from_seed)
                && parsed.generated_at < current.generated_at
            {
                // A replayed old (but validly signed) index.
                return Err(Error::OlderIndex(self.url.clone()));
            }
            st.index.as_ref() != Some(&parsed)
        };
        std::fs::create_dir_all(&self.dir)?;
        write_atomic(&self.dir.join("index.json"), &data)?;
        write_atomic(&self.dir.join("index.json.minisig"), sig.as_bytes())?;
        {
            let mut st = self.lock();
            let t = now();
            // Not on the first download: with nothing before, all would be
            // new. The bundled index alone doesn't count either, as the
            // player hasn't seen it before.
            let first = st.from_seed && st.meta.checked_at.is_none();
            let before: Option<Vec<String>> = st
                .index
                .as_ref()
                .filter(|i| !i.entries.is_empty() && !first)
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
            st.from_seed = false;
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
    // Debug builds run the code of the next release, which entries made for
    // it need, so they skip `minAppVersion` (tests keep it).
    let app = (!cfg!(all(debug_assertions, not(test)))).then_some(env!("CARGO_PKG_VERSION"));
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
                Ok(mut e) if usable(&e, app) => {
                    // For the interface, which reads the field.
                    e.requires_game = e.standalone && !e.needs_no_game();
                    Some(e)
                }
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

/// `app_version` `None`: any `minAppVersion` will do.
fn usable(e: &IndexEntry, app_version: Option<&str>) -> bool {
    !e.downloads.is_empty()
        && catalog::game(&e.game).is_some()
        && e.min_app_version
            .as_deref()
            .is_none_or(|min| app_version.is_none_or(|v| version_at_least(v, min)))
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

/// The download of `entry` whose files the player's own folder or ZIP at
/// `path` has, unless it's the newest download itself, which installs as
/// that.
pub fn players_download(entry: &IndexEntry, path: &Path) -> Option<usize> {
    if path.is_file()
        && let (Some(d), Ok(data)) = (entry.downloads.first(), std::fs::read(path))
        && sha256_hex(&data).eq_ignore_ascii_case(&d.sha256)
    {
        return None;
    }
    let source = Source::open(path).ok()?;
    let files = source.files().ok()?;
    download_for_files(entry, &source, &files, &mut HashMap::new())
}

impl Download {
    /// Whether a player's own copy of it can be told by its files.
    fn known_by_files(&self) -> bool {
        self.fingerprint.is_some() || !self.programs.is_empty()
    }
}

/// The download of `entry` whose files all sit in one folder of `source`
/// (`files`), byte for byte: those of its `fingerprint`, else its
/// `programs`. Other files beside them don't matter. `hashes` keeps what
/// was read, by path.
fn download_for_files(
    entry: &IndexEntry,
    source: &Source,
    files: &[String],
    hashes: &mut HashMap<String, Option<String>>,
) -> Option<usize> {
    let mut hash = |rel: &str| -> Option<String> {
        hashes
            .entry(rel.to_string())
            .or_insert_with(|| source.read(rel).ok().map(|d| sha256_hex(&d)))
            .clone()
    };
    // The SHA-256 of each of `names` in `dir`, if all are there.
    let mut in_dir = |dir: &str, names: &mut dyn Iterator<Item = &String>| {
        names
            .map(|name| {
                let path = files.iter().find(|g| {
                    let (gd, gn) = split_dir(g);
                    gd == dir && gn.eq_ignore_ascii_case(name)
                })?;
                Some((name.clone(), hash(path)?))
            })
            .collect::<Option<Vec<(String, String)>>>()
    };
    let by_download = entry.downloads.iter().position(|d| {
        let first = match &d.fingerprint {
            Some(f) => f.files.first(),
            None => d.programs.keys().next(),
        };
        let Some(first) = first else {
            return false;
        };
        // The folders holding the first file are the candidates.
        files
            .iter()
            .filter(|f| split_dir(f).1.eq_ignore_ascii_case(first))
            .any(|f| {
                let dir = split_dir(f).0;
                match &d.fingerprint {
                    Some(fp) => in_dir(dir, &mut fp.files.iter())
                        .is_some_and(|h| fingerprint_sha256(&h).eq_ignore_ascii_case(&fp.sha256)),
                    None => in_dir(dir, &mut d.programs.keys()).is_some_and(|h| {
                        h.iter()
                            .zip(d.programs.values())
                            .all(|((_, h), sha)| h.eq_ignore_ascii_case(sha))
                    }),
                }
            })
    });
    // Or one variant's files, which are the newest download's (§9.7).
    by_download.or_else(|| {
        entry
            .variants
            .iter()
            .filter_map(|v| v.fingerprint.as_ref())
            .any(|fp| {
                let Some(first) = fp.files.first() else {
                    return false;
                };
                files
                    .iter()
                    .filter(|f| split_dir(f).1.eq_ignore_ascii_case(first))
                    .any(|f| {
                        in_dir(split_dir(f).0, &mut fp.files.iter()).is_some_and(|h| {
                            fingerprint_sha256(&h).eq_ignore_ascii_case(&fp.sha256)
                        })
                    })
            })
            .then_some(0)
    })
}

/// Ties installed mods that aren't tied to an entry to the one whose
/// download they were installed from: added from KeenWiki or a file, maybe
/// before the entry joined the index. Known by the archive's SHA-256, or by
/// the download's files among the mod's (in `paths.mods`), unless the
/// entry is installed already. An older download counts too, so the entry
/// offers the update. The entry's details replace what the mod brought
/// along ([`mods::tie_to_index`]). Mods tied already follow their entry's
/// `standalone`, so an entry can drop its game.
pub fn link_installed(index: &ModIndex, db: &Db, paths: &AppPaths) -> Result<()> {
    let entries = index.entries();
    for e in &entries {
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
                mods::tie_to_index(db, &mod_id, &Curated::new(e, i))?;
            }
        }
    }
    let untied: Vec<(String, String)> = db
        .conn()
        .prepare("SELECT id, game_id FROM mods WHERE index_id IS NULL ORDER BY added_at")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (mod_id, game_id) in untied {
        let candidates: Vec<&IndexEntry> = entries
            .iter()
            .filter(|e| {
                e.game == game_id
                    && (e.downloads.iter().any(Download::known_by_files)
                        || e.variants.iter().any(|v| v.fingerprint.is_some()))
            })
            .collect();
        if candidates.is_empty() {
            continue;
        }
        let Ok(source) = Source::open(&paths.mods.join(&mod_id).join("files")) else {
            continue;
        };
        let Ok(files) = source.files() else {
            continue;
        };
        let mut hashes = HashMap::new();
        let found = candidates
            .into_iter()
            .find_map(|e| Some((e, download_for_files(e, &source, &files, &mut hashes)?)));
        if let Some((e, i)) = found
            && mods::find_by_index(db, &e.id)?.is_none()
        {
            mods::tie_to_index(db, &mod_id, &Curated::new(e, i))?;
        }
    }
    for e in &entries {
        mods::set_standalone(db, &e.id, e.needs_no_game(), e.standalone)?;
        mods::set_worlds(db, &e.id, e.worlds.as_ref())?;
        mods::set_save_dirs(db, &e.id, &e.save_dirs)?;
        mods::set_cover_from(db, &e.id, e.cover_from.as_ref())?;
    }
    Ok(())
}

/// Reinstalls the index mods whose entry names a `root` other than the one
/// they were installed from, from the archive they keep, with the entry's
/// download they are ([`mods::follow_root`]): copies installed before the
/// entry named it (decided 2026-11-19). A mod that runs, or whose
/// reinstall fails, stays as it is until the next check.
pub fn follow_roots(index: &ModIndex, db: &Db, paths: &AppPaths, running: &Running) -> Result<()> {
    for e in index.entries() {
        let Some(root) = &e.root else { continue };
        let found: Vec<(String, Option<String>)> = db
            .conn()
            .prepare("SELECT id, archive_sha256 FROM mods WHERE index_id = ?1 AND root IS NOT ?2")?
            .query_map(params![e.id, root], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let old_root = e
            .save_dirs
            .iter()
            .find(|d| d.old_root)
            .map(|d| d.dir.as_str());
        for (mod_id, sha) in found {
            let download = e.downloads.iter().position(|d| {
                sha.as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(&d.sha256))
            });
            let Some(i) = download else {
                tracing::warn!(
                    "mod {mod_id} is no download of {}; left in its folder",
                    e.id
                );
                continue;
            };
            match mods::follow_root(paths, db, running, &mod_id, &Curated::new(&e, i), old_root) {
                Ok(true) => {}
                Ok(false) => tracing::info!("mod {mod_id} runs; it moves to its root later"),
                Err(err) => tracing::warn!("could not move mod {mod_id} to its root: {err}"),
            }
        }
    }
    Ok(())
}

/// An installed mod whose entry has variants it hasn't, or other ones:
/// its id, and the entry.
pub struct ToConvert {
    pub mod_id: String,
    pub entry: IndexEntry,
}

/// The installed index mods to turn into ones with their entry's variants
/// (§9.7, [`mods::convert_to_variants`]): those of the newest download, as
/// variants describe it; older ones get them with their update. Mods still
/// waiting to move to their entry's `root` wait for that.
pub fn variants_to_convert(index: &ModIndex, db: &Db) -> Result<Vec<ToConvert>> {
    let mut out = Vec::new();
    for e in index.entries() {
        if e.variants.is_empty() {
            continue;
        }
        let want = serde_json::to_string(&e.variants).expect("variants serialize");
        // Id, download, root, variants as installed.
        type Row = (String, Option<String>, Option<String>, Option<String>);
        let found: Vec<Row> = db
            .conn()
            .prepare(
                "SELECT id, archive_sha256, root, variants_json FROM mods WHERE index_id = ?1",
            )?
            .query_map([&e.id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        for (mod_id, sha, root, have) in found {
            let newest = sha
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(&e.downloads[0].sha256));
            if !newest || (e.root.is_some() && root != e.root) || have.as_deref() == Some(&want) {
                continue;
            }
            out.push(ToConvert {
                mod_id,
                entry: e.clone(),
            });
        }
    }
    Ok(out)
}

/// Makes the installed index mods follow their entry's recipe and details,
/// for the download they are ([`mods::follow_entry`]). Mods still waiting
/// to move to their entry's `root` wait for that ([`follow_roots`]), and
/// those waiting for their entry's variants for those
/// ([`variants_to_convert`]): its recipe is the default's, not theirs.
pub fn follow_entries(
    index: &ModIndex,
    db: &Db,
    paths: &AppPaths,
    running: &Running,
) -> Result<()> {
    for e in index.entries() {
        let found: Vec<(String, Option<String>, Option<String>, bool)> = db
            .conn()
            .prepare(
                "SELECT id, archive_sha256, root, variants_json IS NOT NULL
                 FROM mods WHERE index_id = ?1",
            )?
            .query_map([&e.id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        for (mod_id, sha, root, has_variants) in found {
            if (e.root.is_some() && root != e.root) || (!e.variants.is_empty() && !has_variants) {
                continue;
            }
            let download = e.downloads.iter().position(|d| {
                sha.as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(&d.sha256))
            });
            let Some(i) = download else { continue };
            if let Err(err) = mods::follow_entry(paths, db, running, &mod_id, &Curated::new(&e, i))
            {
                tracing::warn!("mod {mod_id} could not follow its entry: {err}");
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
    /// In bytes, as the index tells, with its parts.
    pub size: u64,
    /// Key → the further archives of the download, which variants come
    /// from.
    pub parts: BTreeMap<String, PathBuf>,
    downloaded: bool,
}

impl Drop for Archive {
    fn drop(&mut self) {
        if self.downloaded {
            let _ = std::fs::remove_file(&self.path);
        }
        for path in self.parts.values() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Downloads the `parts` of `entry`'s newest download into `cache`, which
/// its variants come from: key → file. Deleted when the archive is dropped.
pub async fn parts(
    cache: &Path,
    entry: &IndexEntry,
    progress: &impl Fn(u64, u64),
) -> Result<BTreeMap<String, PathBuf>> {
    let Some(download) = entry.downloads.first() else {
        return Ok(BTreeMap::new());
    };
    let total: u64 = download.parts.values().map(|p| u64::from(p.size)).sum();
    let mut done = 0;
    let mut out = BTreeMap::new();
    for (key, part) in &download.parts {
        let path = cache
            .join("downloads")
            .join(format!("index-{}-{key}.zip", entry.id));
        let fetched = downloads::fetch(
            &part.url,
            &part.sha256,
            u64::from(part.size),
            &path,
            &|r, _| progress(done + r, total),
        )
        .await;
        if let Err(e) = fetched {
            for path in out.values() {
                let _ = std::fs::remove_file(path);
            }
            return Err(e);
        }
        done += u64::from(part.size);
        out.insert(key.clone(), path);
    }
    Ok(out)
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
            parts: BTreeMap::new(),
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
        parts: BTreeMap::new(),
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

    #[test]
    fn variant_names_in_the_apps_language() {
        let v: Variant = serde_json::from_str(
            r#"{"id": "hard", "name": "Hard", "translations": {"de": {"name": "Schwer"}}}"#,
        )
        .unwrap();
        assert_eq!(v.name_in(Some("de")), "Schwer");
        assert_eq!(v.name_in(Some("de-DE")), "Schwer");
        assert_eq!(v.name_in(Some("fr")), "Hard");
        assert_eq!(v.name_in(None), "Hard");
    }

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
    fn bundled_index_is_signed() {
        // Only in builds that have one (release.yml).
        if !SEED.is_empty() {
            let (_tmp, paths) = setup();
            let index = ModIndex::open(&paths, DEFAULT_URL.into());
            assert!(index.status().mod_count > 0);
        }
    }

    #[test]
    fn embedded_keys_are_valid() {
        for k in PUBLIC_KEYS {
            PublicKey::from_base64(k).unwrap();
        }
    }

    #[test]
    fn reads_a_root_save_folders_and_a_cover_from_one() {
        let index: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        let e: IndexEntry = serde_json::from_value(index["mods"][0].clone()).unwrap();
        assert_eq!(e.root.as_deref(), Some("Levels"));
        assert_eq!(e.save_dirs.len(), 2);
        assert!(e.save_dirs[0].old_root && !e.save_dirs[1].old_root);
        assert_eq!(e.save_dirs[1].translations["de"].name, "Schwer");
        let c = e.cover_from.unwrap();
        assert_eq!((c.dir.as_str(), c.patch.as_str()), ("EASY", "EASY.PAT"));
    }

    #[test]
    fn reads_variants_and_the_parts_they_come_from() {
        let index: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        let e: IndexEntry = serde_json::from_value(index["mods"][0].clone()).unwrap();
        assert_eq!(e.downloads[0].parts["green"].size, 77);
        let [easy, green] = &e.variants[..] else {
            panic!("{:?}", e.variants)
        };
        assert!(easy.default && !green.default);
        assert_eq!(easy.root.as_deref(), Some("Levels/EASY"));
        assert_eq!(easy.recipe, None);
        assert_eq!(green.archive.as_deref(), Some("green"));
        assert_eq!(green.translations["de"].name, "Grün");
        assert_eq!(
            green.copy_to_root.as_deref(),
            Some(&["shared".to_string()][..])
        );
        assert_eq!(green.files["LEVEL80.CK1"], "alt/LEVEL80.GRN");
        assert_eq!(green.cover.as_deref(), Some("TITLE.EXE"));
        assert!(green.fingerprint.is_some());
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
        let worlds = e.worlds.as_ref().unwrap();
        assert_eq!(worlds.seed.as_ref().unwrap().max, 65535);
        assert_eq!(worlds.options[1].default, Some(3));

        let newer = SAMPLE.replace(r#""type": "levelpack""#, r#""type": "minigame""#);
        let newer = newer.replace("9.0.0", "0.0.1");
        assert_eq!(parse(newer.as_bytes()).unwrap().hidden, 1);
        let future = SAMPLE.replace(r#""schemaVersion": 1"#, r#""schemaVersion": 2"#);
        let p = parse(future.as_bytes()).unwrap();
        assert_eq!((p.entries.len(), p.hidden), (0, 2));
    }

    #[test]
    fn a_status_the_app_doesnt_know_hides_nothing() {
        let status = |status: Option<&str>| {
            let mut index: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
            let mods = index["mods"].as_array_mut().unwrap();
            let e = mods.iter_mut().find(|m| m["id"] == "sample-patch").unwrap();
            if let Some(s) = status {
                e["status"] = s.into();
            }
            let p = parse(index.to_string().as_bytes()).unwrap();
            p.entries[0].status
        };
        assert_eq!(status(None), ModStatus::Complete);
        assert_eq!(status(Some("abandoned")), ModStatus::Abandoned);
        assert_eq!(status(Some("gamma")), ModStatus::Other);
    }

    #[test]
    fn the_app_decides_which_standalone_mods_need_their_game() {
        let parsed = |game: &str, requires_game: bool| {
            let mut index: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
            let mods = index["mods"].as_array_mut().unwrap();
            let e = mods.iter_mut().find(|m| m["id"] == "sample-patch").unwrap();
            e["game"] = game.into();
            e["standalone"] = true.into();
            e["requiresGame"] = requires_game.into();
            e["requires"] = serde_json::json!([]);
            let e = parse(index.to_string().as_bytes())
                .unwrap()
                .entries
                .remove(0);
            (e.needs_no_game(), e.requires_game)
        };
        // Whatever the index says: free shareware or not.
        assert_eq!(parsed("keen4", true), (true, false));
        assert_eq!(parsed("kdreams", false), (true, false));
        assert_eq!(parsed("keen6", false), (false, true));
        assert_eq!(parsed("keen2", true), (false, true));
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

    #[test]
    fn knows_a_source_mod_by_its_programs() {
        let tmp = tempfile::tempdir().unwrap();
        let mut entry = sample_entry(&sha256_hex(b"the newest zip"));
        entry.standalone = true;
        let mut old = entry.downloads[0].clone();
        old.version = Some("1.0".into());
        old.sha256 = "ab".repeat(32);
        old.recipe = Some(Recipe::Program {
            program: "OLD.EXE".into(),
            args: Vec::new(),
        });
        old.programs = [("OLD.EXE".to_string(), sha256_hex(b"old"))].into();
        entry.downloads[0].programs = [
            ("GAME.EXE".to_string(), sha256_hex(b"new")),
            ("MENU.EXE".to_string(), sha256_hex(b"menu")),
        ]
        .into();
        entry.downloads.push(old);

        let folder = |name: &str, files: &[(&str, &[u8])]| {
            let dir = tmp.path().join(name);
            crate::source::testutil::write_folder(&dir, files);
            dir
        };
        let found = |path: &Path| players_download(&entry, path);
        // In a subfolder, any case, with saves next to it.
        let new = folder(
            "new",
            &[
                ("Mod/game.exe", b"new"),
                ("Mod/MENU.EXE", b"menu"),
                ("Mod/SAVEGAM0.CK4", b"s"),
            ],
        );
        assert_eq!(found(&new), Some(0));
        assert_eq!(found(&folder("old", &[("old.exe", b"old")])), Some(1));
        // Both programs in one folder, byte for byte.
        let split = folder("split", &[("a/GAME.EXE", b"new"), ("b/MENU.EXE", b"menu")]);
        assert_eq!(found(&split), None);
        assert_eq!(found(&folder("changed", &[("OLD.EXE", b"patched")])), None);
        // The newest download itself installs as that.
        let zip = tmp.path().join("newest.zip");
        std::fs::write(&zip, b"the newest zip").unwrap();
        assert_eq!(found(&zip), None);
    }

    #[test]
    fn a_fingerprint_hashes_sorted_upper_case_names() {
        let lines = format!(
            "LEVEL01.CK4 {}\nX.PAT {}\n",
            sha256_hex(b"l"),
            sha256_hex(b"x")
        );
        let files = [("x.pat", b"x"), ("Level01.ck4", b"l")]
            .map(|(n, d)| (n.to_string(), sha256_hex(d).to_uppercase()));
        assert_eq!(fingerprint_sha256(&files), sha256_hex(lines.as_bytes()));
    }

    /// `entry` with a fingerprint of `files` for its download `i`.
    fn with_fingerprint(entry: &mut IndexEntry, i: usize, files: &[(&str, &[u8])]) {
        let hashes: Vec<(String, String)> = files
            .iter()
            .map(|(n, d)| (n.to_string(), sha256_hex(d)))
            .collect();
        entry.downloads[i].fingerprint = Some(Fingerprint {
            sha256: fingerprint_sha256(&hashes),
            files: files.iter().map(|(n, _)| n.to_string()).collect(),
        });
    }

    #[test]
    fn knows_any_mod_by_its_fingerprint() {
        let tmp = tempfile::tempdir().unwrap();
        let mut entry = sample_entry(&sha256_hex(b"the newest zip"));
        let mut old = entry.downloads[0].clone();
        old.sha256 = "ab".repeat(32);
        entry.downloads.push(old);
        with_fingerprint(
            &mut entry,
            0,
            &[("GAMEMAPS.CK4", b"new maps"), ("MOD.PAT", b"patch")],
        );
        with_fingerprint(
            &mut entry,
            1,
            &[("GAMEMAPS.CK4", b"old maps"), ("MOD.PAT", b"patch")],
        );
        let folder = |name: &str, files: &[(&str, &[u8])]| {
            let dir = tmp.path().join(name);
            crate::source::testutil::write_folder(&dir, files);
            dir
        };
        let found = |path: &Path| players_download(&entry, path);
        // Unpacked into a folder of the game, with saves and readmes.
        let new = folder(
            "new",
            &[
                ("Keen/gamemaps.ck4", b"new maps"),
                ("Keen/mod.pat", b"patch"),
                ("Keen/SAVEGAM0.CK4", b"s"),
                ("Keen/KEEN4E.EXE", b"game"),
                ("Keen/README.TXT", b"r"),
            ],
        );
        assert_eq!(found(&new), Some(0));
        let old = folder(
            "old",
            &[("GAMEMAPS.CK4", b"old maps"), ("MOD.PAT", b"patch")],
        );
        assert_eq!(found(&old), Some(1));
        // A changed level, a missing file: not that mod.
        let changed = folder(
            "changed",
            &[("GAMEMAPS.CK4", b"my maps"), ("MOD.PAT", b"patch")],
        );
        assert_eq!(found(&changed), None);
        assert_eq!(found(&folder("part", &[("MOD.PAT", b"patch")])), None);
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
        assert_eq!(st.error.unwrap().kind, crate::error::ErrorKind::OlderIndex);
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

    #[tokio::test]
    async fn takes_an_older_index_from_another_place() {
        let (tmp, paths) = setup();
        let k = keys();
        let url = publish(tmp.path(), &k, SAMPLE);
        let index = ModIndex::with_keys(&paths, url, &[&k.pk]);
        assert!(index.refresh(true).await.error.is_none());

        let elsewhere = tempfile::tempdir().unwrap();
        let old = SAMPLE.replace("1760000000", "1700000000");
        let other = publish(elsewhere.path(), &k, &old);
        let moved = ModIndex::with_keys(&paths, other, &[&k.pk]);
        let st = moved.refresh(true).await;
        assert!(st.error.is_none());
        assert_eq!(st.generated_at, Some(1_700_000_000));
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
    async fn starts_with_the_bundled_index() {
        let (tmp, paths) = setup();
        let k = keys();
        let more = SAMPLE.replace(r#""minAppVersion": "9.0.0""#, r#""minAppVersion": "0.0.1""#);
        let seed_sig = sign(&k, &more);
        let seed = Some((more.as_bytes(), seed_sig.as_str()));
        let url = tmp.path().join("missing.json").display().to_string();

        // No cache, server unreachable: the bundled index.
        let index = ModIndex::with_seed(&paths, url.clone(), &[&k.pk], seed);
        let st = index.refresh(true).await;
        assert!(st.error.is_some());
        assert_eq!(st.mod_count, 2);

        // A tampered one is ignored.
        let bad = ModIndex::with_seed(&paths, url, &[&k.pk], Some((b"{}", seed_sig.as_str())));
        assert_eq!(bad.status().mod_count, 0);

        // The first download replaces it without marking anything new.
        let newer = more.replace("1760000000", "1760000001");
        let url = publish(tmp.path(), &k, &newer);
        let index = ModIndex::with_seed(&paths, url.clone(), &[&k.pk], seed);
        let st = index.refresh(true).await;
        assert!(st.error.is_none(), "{:?}", st.error);
        assert_eq!(st.generated_at, Some(1_760_000_001));
        let db = Db::open_in_memory().unwrap();
        assert!(list(&index, &db).unwrap().iter().all(|m| !m.new));

        // The newer cache wins over the bundled index on the next start.
        let again = ModIndex::with_seed(&paths, url, &[&k.pk], seed);
        assert_eq!(again.status().generated_at, Some(1_760_000_001));
    }

    #[tokio::test]
    async fn a_newer_bundled_index_replaces_an_older_cache() {
        let (tmp, paths) = setup();
        let k = keys();
        let old = SAMPLE.replace("1760000000", "1700000000");
        let url = publish(tmp.path(), &k, &old);
        let index = ModIndex::with_keys(&paths, url.clone(), &[&k.pk]);
        assert!(index.refresh(true).await.error.is_none());

        let sig = sign(&k, SAMPLE);
        let seed = Some((SAMPLE.as_bytes(), sig.as_str()));
        let updated = ModIndex::with_seed(&paths, url, &[&k.pk], seed);
        assert_eq!(updated.status().generated_at, Some(1_760_000_000));
        // The server still serving the old one is a replay, not news.
        let st = updated.refresh(true).await;
        assert_eq!(st.error.unwrap().kind, crate::error::ErrorKind::OlderIndex);
        assert_eq!(st.generated_at, Some(1_760_000_000));
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
        link_installed(&index, &db, &paths).unwrap();
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

    #[test]
    fn ties_mods_installed_by_hand_by_their_files() {
        let (tmp, paths) = setup();
        let k = keys();
        let mut json: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        let files = [("GAMEMAPS.CK4", b"maps" as &[u8]), ("MOD.PAT", b"patch")];
        let hashes: Vec<(String, String)> = files
            .iter()
            .map(|(n, d)| (n.to_string(), sha256_hex(d)))
            .collect();
        for e in json["mods"].as_array_mut().unwrap() {
            if e["id"] == "sample-patch" {
                e["downloads"][0]["fingerprint"] = serde_json::json!({
                    "files": ["GAMEMAPS.CK4", "MOD.PAT"],
                    "sha256": fingerprint_sha256(&hashes),
                });
            }
        }
        let url = publish(tmp.path(), &k, &json.to_string());
        let index = ModIndex::with_keys(&paths, url, &[&k.pk]);
        tauri::async_runtime::block_on(index.refresh(false));
        let db = Db::open_in_memory().unwrap();
        // From a folder, renamed, with the player's saves; the same files a
        // second time; another mod; another game's.
        for (id, game, files, at) in [
            (
                "mine",
                "keen4",
                &[
                    ("gamemaps.ck4", b"maps" as &[u8]),
                    ("MOD.PAT", b"patch"),
                    ("SAVEGAM0.CK4", b"s"),
                ][..],
                1,
            ),
            ("again", "keen4", &files[..], 2),
            (
                "other",
                "keen4",
                &[("GAMEMAPS.CK4", b"other" as &[u8]), ("MOD.PAT", b"patch")][..],
                3,
            ),
            ("k5", "keen5", &files[..], 4),
        ] {
            crate::source::testutil::write_folder(&paths.mods.join(id).join("files"), files);
            db.conn()
                .execute(
                    "INSERT INTO mods (id, game_id, name, authors, recipe_json, path, source,
                                       added_at)
                     VALUES (?1, ?2, 'My name', '[]', '{}', '/x', 'manual', ?3)",
                    params![id, game, at],
                )
                .unwrap();
        }
        link_installed(&index, &db, &paths).unwrap();
        let tied: Vec<(String, Option<String>, String, Option<String>)> = db
            .conn()
            .prepare("SELECT id, index_id, name, archive_sha256 FROM mods ORDER BY added_at")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let sha = index.entry("sample-patch").unwrap().downloads[0]
            .sha256
            .clone();
        assert_eq!(
            tied[0],
            (
                "mine".into(),
                Some("sample-patch".into()),
                "Sample Patch Mod".into(),
                Some(sha)
            )
        );
        // The entry has its mod now; the others stay as they are.
        assert!(
            tied[1..]
                .iter()
                .all(|(_, i, n, _)| i.is_none() && n == "My name")
        );
        assert!(
            !list(&index, &db)
                .unwrap()
                .iter()
                .any(|m| m.update_available)
        );
    }

    #[test]
    fn tied_mods_follow_their_entry_as_it_becomes_standalone() {
        let (tmp, paths) = setup();
        let k = keys();
        let standalone = SAMPLE.replace(
            r#""id": "sample-patch","#,
            r#""id": "sample-patch", "standalone": true,"#,
        );
        let url = publish(tmp.path(), &k, &standalone);
        let index = ModIndex::with_keys(&paths, url, &[&k.pk]);
        tauri::async_runtime::block_on(index.refresh(false));
        assert!(index.entry("sample-patch").unwrap().standalone);
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO installs (id, game_id, label, source, path, added_at)
                 VALUES ('keen4-1', 'keen4', 'Keen 4', 'folder', '/k4', 0);
                 INSERT INTO mods (id, game_id, index_id, name, authors, recipe_json, path,
                                   source, added_at)
                 VALUES ('m', 'keen4', 'sample-patch', 'x', '[]', '{}', '/x', 'index', 0);
                 INSERT INTO profiles (id, install_id, mod_id, state_path, created_at)
                 VALUES ('m', 'keen4-1', 'm', '/s', 0);",
            )
            .unwrap();
        link_installed(&index, &db, &paths).unwrap();
        let (install, flag): (Option<String>, bool) = db
            .conn()
            .query_row(
                "SELECT p.install_id, m.standalone FROM profiles p JOIN mods m ON m.id = p.mod_id",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((install, flag), (None, true));
    }

    #[test]
    fn mods_of_the_newest_download_get_their_entrys_variants() {
        let (tmp, paths) = setup();
        let k = keys();
        let with = SAMPLE.replace(
            r#""id": "sample-patch","#,
            r#""id": "sample-patch", "variants": [
                {"id": "a", "default": true, "name": "A"}, {"id": "b", "name": "B"}],"#,
        );
        let url = publish(tmp.path(), &k, &with);
        let index = ModIndex::with_keys(&paths, url, &[&k.pk]);
        tauri::async_runtime::block_on(index.refresh(false));
        let entry = index.entry("sample-patch").unwrap();
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute_batch(&format!(
                "INSERT INTO mods (id, game_id, index_id, name, authors, recipe_json, path,
                                   source, added_at, archive_sha256)
                 VALUES ('new', 'keen4', 'sample-patch', 'x', '[]', '{{}}', '/x', 'index', 0,
                         '{0}'),
                        ('old', 'keen4', 'sample-patch', 'x', '[]', '{{}}', '/x', 'index', 0,
                         'beef'),
                        ('done', 'keen4', 'sample-patch', 'x', '[]', '{{}}', '/x', 'index', 0,
                         '{0}');
                 UPDATE mods SET variants_json = '{1}' WHERE id = 'done';",
                entry.downloads[0].sha256,
                serde_json::to_string(&entry.variants).unwrap()
            ))
            .unwrap();
        let todo = variants_to_convert(&index, &db).unwrap();
        assert_eq!(
            todo.iter().map(|t| t.mod_id.as_str()).collect::<Vec<_>>(),
            ["new"]
        );
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

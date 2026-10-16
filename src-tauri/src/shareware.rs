//! Free shareware episodes offered by the setup wizard (PLAN.md §2.3).

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::db::Db;
use crate::detect::detect;
use crate::downloads;
use crate::error::{Error, Result};
use crate::jobs::JobPhase;
use crate::library::{self, InstallSource};
use crate::paths::AppPaths;
use crate::source::Source;

#[derive(Debug, Deserialize)]
struct Manifest {
    episodes: Vec<Episode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Episode {
    game_id: String,
    version_id: String,
    mirrors: Vec<Mirror>,
}

#[derive(Debug, Deserialize)]
struct Mirror {
    url: String,
    sha256: String,
    size: u64,
}

static MANIFEST: LazyLock<Manifest> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../catalog/shareware.json"))
        .expect("bundled shareware manifest is invalid")
});

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FreeEpisode {
    pub game_id: String,
    pub version_id: String,
    /// Download size in bytes.
    pub size: u32,
    /// This version is already in the library.
    pub installed: bool,
}

pub fn list(db: &Db) -> Result<Vec<FreeEpisode>> {
    MANIFEST
        .episodes
        .iter()
        .map(|e| {
            Ok(FreeEpisode {
                game_id: e.game_id.clone(),
                version_id: e.version_id.clone(),
                size: e.mirrors.first().map_or(0, |m| m.size as u32),
                installed: library::has_version(db, &e.version_id)?,
            })
        })
        .collect()
}

/// Downloads a free episode, trying each mirror in turn, and imports it.
pub async fn install(
    paths: &AppPaths,
    db: &Db,
    game_id: &str,
    progress: impl Fn(JobPhase, u64, u64),
) -> Result<()> {
    let episode = MANIFEST
        .episodes
        .iter()
        .find(|e| e.game_id == game_id)
        .ok_or_else(|| Error::NotFound(format!("free episode {game_id}")))?;
    let mut last_err = None;
    for mirror in &episode.mirrors {
        let name = mirror.url.rsplit('/').next().unwrap_or("download.zip");
        let archive = paths
            .cache
            .join("downloads")
            .join(format!("{game_id}-{name}"));
        tracing::info!("downloading {} from {}", episode.version_id, mirror.url);
        let fetched = downloads::fetch(
            &mirror.url,
            &mirror.sha256,
            mirror.size,
            &archive,
            &|received, total| progress(JobPhase::Downloading, received, total),
        )
        .await;
        if let Err(err) = fetched {
            tracing::warn!("{} failed: {err}", mirror.url);
            last_err = Some(err);
            continue;
        }
        progress(JobPhase::Installing, mirror.size, mirror.size);
        // A few hundred KB: importing right here is quick enough.
        let result = import_verified(paths, db, episode, &archive, &mirror.url);
        let _ = fs::remove_file(&archive);
        return result;
    }
    Err(last_err.unwrap_or_else(|| Error::NotFound(format!("mirror for {game_id}"))))
}

/// Imports the archive if it holds exactly the expected episode version.
fn import_verified(
    paths: &AppPaths,
    db: &Db,
    episode: &Episode,
    archive: &Path,
    origin: &str,
) -> Result<()> {
    let found = detect(&Source::open(archive)?)?;
    let expected = |d: &crate::detect::Detection| {
        d.game_id == episode.game_id && d.version_id.as_deref() == Some(&episode.version_id)
    };
    if found.len() != 1 || !expected(&found[0]) {
        return Err(Error::UnexpectedContent(format!(
            "expected only {}",
            episode.version_id
        )));
    }
    library::import_from(paths, db, archive, InstallSource::Download, origin, None)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use crate::source::testutil::*;

    #[test]
    fn manifest_matches_catalog() {
        assert!(!MANIFEST.episodes.is_empty());
        for e in &MANIFEST.episodes {
            let game = catalog::game(&e.game_id).unwrap_or_else(|| panic!("{}", e.game_id));
            let version = game.versions.iter().find(|v| v.id == e.version_id);
            assert!(
                version.is_some_and(|v| v.shareware),
                "{} is not a shareware version",
                e.version_id
            );
            assert!(!e.mirrors.is_empty());
            for m in &e.mirrors {
                assert!(m.url.starts_with("https://") || m.url.starts_with("http://"));
                assert_eq!(m.sha256.len(), 64, "{}", m.url);
                assert!(m.size > 0);
            }
        }
    }

    #[test]
    fn rejects_archives_with_other_content() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        let db = Db::open_in_memory().unwrap();
        // The fixture's executable isn't the known shareware build.
        let zip = tmp.path().join("4keen.zip");
        write_zip(&zip, KEEN4);
        let episode = MANIFEST
            .episodes
            .iter()
            .find(|e| e.game_id == "keen4")
            .unwrap();
        assert!(matches!(
            import_verified(&paths, &db, episode, &zip, "x"),
            Err(Error::UnexpectedContent(_))
        ));
        assert!(library::list(&db).unwrap().is_empty());
    }

    /// Downloads every mirror and checks it holds the expected version.
    /// Run with `cargo test -- --ignored` when changing the manifest.
    #[tokio::test]
    #[ignore = "downloads from the internet"]
    async fn all_mirrors_deliver_the_expected_version() {
        for e in &MANIFEST.episodes {
            for m in &e.mirrors {
                let tmp = tempfile::tempdir().unwrap();
                let paths = AppPaths::new(tmp.path().join("data"));
                paths.ensure().unwrap();
                let db = Db::open_in_memory().unwrap();
                let archive = tmp.path().join("download.zip");
                downloads::fetch(&m.url, &m.sha256, m.size, &archive, &|_, _| {})
                    .await
                    .unwrap_or_else(|err| panic!("{}: {err}", m.url));
                import_verified(&paths, &db, e, &archive, &m.url)
                    .unwrap_or_else(|err| panic!("{}: {err}", m.url));
                assert!(library::has_version(&db, &e.version_id).unwrap());
            }
        }
    }
}

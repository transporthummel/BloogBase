//! Hash-verified downloads with progress (PLAN.md §3).

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::detect::hex;
use crate::error::{Error, Result};

pub const USER_AGENT: &str = concat!(
    "BloogBase/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/transporthummel/BloogBase)"
);

/// Streams `url` to `dest`, hashing on the fly. `dest` only appears once the
/// SHA-256 matched. `progress` gets (received, total) bytes every ~100 ms;
/// `size` is the expected total when the server doesn't send one.
pub async fn fetch(
    url: &str,
    sha256: &str,
    size: u64,
    dest: &Path,
    progress: &impl Fn(u64, u64),
) -> Result<()> {
    let client = reqwest::Client::builder().user_agent(USER_AGENT).build()?;
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(Error::HttpStatus(resp.status().as_u16()));
    }
    stream(resp, size, u64::MAX, dest, progress, |actual| {
        verify(actual, sha256)
    })
    .await
    .map(drop)
}

/// A download whose hash nobody knows yet, e.g. a link from a KeenWiki page.
pub struct Unverified {
    /// The URL after redirects.
    pub final_url: String,
    /// From `Content-Disposition`, when the server sends one.
    pub file_name: Option<String>,
    pub sha256: String,
    pub size: u64,
}

/// Streams `url` to `dest` without a known hash, refusing anything bigger
/// than `max_size`. Dead links are reported as [`Error::DeadLink`].
/// `progress` as for [`fetch`]; the total is 0 when the server doesn't say.
pub async fn fetch_unverified(
    url: &str,
    max_size: u64,
    dest: &Path,
    progress: &impl Fn(u64, u64),
) -> Result<Unverified> {
    let client = reqwest::Client::builder().user_agent(USER_AGENT).build()?;
    let resp = client.get(url).send().await?;
    let status = resp.status().as_u16();
    if is_dead(url, status) {
        return Err(Error::DeadLink(status));
    }
    if !resp.status().is_success() {
        return Err(Error::HttpStatus(status));
    }
    if resp.content_length().is_some_and(|n| n > max_size) {
        return Err(Error::TooLarge(max_size));
    }
    let final_url = resp.url().to_string();
    let file_name = resp
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(disposition_file_name);
    let (sha256, size) = stream(resp, 0, max_size, dest, progress, |_| Ok(())).await?;
    Ok(Unverified {
        final_url,
        file_name,
        sha256,
        size,
    })
}

/// Writes the body to `dest` via a `.part` file; `check` sees the SHA-256
/// before `dest` appears. Returns the hash and the size.
async fn stream(
    mut resp: reqwest::Response,
    size: u64,
    max_size: u64,
    dest: &Path,
    progress: &impl Fn(u64, u64),
    check: impl FnOnce(&str) -> Result<()>,
) -> Result<(String, u64)> {
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    let part = dest.with_extension("part");
    let total = resp.content_length().unwrap_or(size);
    let mut file = tokio::fs::File::create(&part).await?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut last_report = Instant::now();

    let result: Result<()> = async {
        while let Some(chunk) = resp.chunk().await? {
            received += chunk.len() as u64;
            if received > max_size {
                return Err(Error::TooLarge(max_size));
            }
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            if last_report.elapsed() > Duration::from_millis(100) {
                last_report = Instant::now();
                progress(received, total);
            }
        }
        file.flush().await?;
        Ok(())
    }
    .await;
    drop(file);

    let sha256 = hex(&hasher.finalize());
    match result.and_then(|()| check(&sha256)) {
        Ok(()) => {
            progress(received, total);
            fs::rename(&part, dest)?;
            Ok((sha256, received))
        }
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

pub fn verify(actual: &str, expected: &str) -> Result<()> {
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(Error::ChecksumMismatch {
            expected: expected.into(),
            actual: actual.into(),
        })
    }
}

/// `attachment; filename="mod.zip"` → `mod.zip`.
fn disposition_file_name(value: &str) -> Option<String> {
    value.split(';').find_map(|part| {
        let (key, name) = part.trim().split_once('=')?;
        key.eq_ignore_ascii_case("filename")
            .then(|| name.trim().trim_matches('"').to_string())
            .filter(|n| !n.is_empty())
    })
}

/// Whether `status` says nothing is at `url`. The community's file server
/// answers 403 for files it doesn't have (a KeenWiki link to
/// `…/Keen3/m.ally_fleexy.allys_mod_iii.k2.…`, which is `.k3`).
fn is_dead(url: &str, status: u16) -> bool {
    matches!(status, 404 | 410)
        || (status == 403
            && reqwest::Url::parse(url).is_ok_and(|u| u.host_str() == Some("files.shikadi.net")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_on_the_file_server_are_dead_links() {
        let shikadi = "https://files.shikadi.net/keenwiki/mods/Keen3/m.x.k2.2009-08-12.zip";
        assert!(is_dead(shikadi, 403));
        assert!(is_dead("https://example.com/m.zip", 404));
        assert!(!is_dead("https://example.com/m.zip", 403));
        assert!(!is_dead(shikadi, 500));
    }

    #[test]
    fn checksum_is_enforced() {
        assert!(verify("ABCD", "abcd").is_ok());
        assert!(matches!(
            verify("abcd", "beef"),
            Err(Error::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn file_name_from_content_disposition() {
        assert_eq!(
            disposition_file_name(r#"attachment; filename="KEEN.ZIP""#).as_deref(),
            Some("KEEN.ZIP")
        );
        assert_eq!(
            disposition_file_name("inline; filename=mod.zip").as_deref(),
            Some("mod.zip")
        );
        assert_eq!(disposition_file_name("attachment"), None);
    }
}

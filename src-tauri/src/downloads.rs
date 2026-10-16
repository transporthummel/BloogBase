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
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir)?;
    }
    let part = dest.with_extension("part");
    let client = reqwest::Client::builder().user_agent(USER_AGENT).build()?;
    let mut resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(Error::HttpStatus(resp.status().as_u16()));
    }
    let total = resp.content_length().unwrap_or(size);
    let mut file = tokio::fs::File::create(&part).await?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut last_report = Instant::now();

    let result: Result<()> = async {
        while let Some(chunk) = resp.chunk().await? {
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            received += chunk.len() as u64;
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

    let result = result.and_then(|()| verify(&hex(&hasher.finalize()), sha256));
    match result {
        Ok(()) => {
            progress(received, total);
            Ok(fs::rename(&part, dest)?)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_is_enforced() {
        assert!(verify("ABCD", "abcd").is_ok());
        assert!(matches!(
            verify("abcd", "beef"),
            Err(Error::ChecksumMismatch { .. })
        ));
    }
}

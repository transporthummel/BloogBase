//! Downloads an entry's archive (once, into a cache) and checks it holds
//! what the entry promises.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::entry::{Download, Entry};

const USER_AGENT: &str = concat!(
    "BloogBase-index/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/transporthummel/BloogBase)"
);

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}

/// Whether the server still has the file, without downloading it.
pub fn reachable(download: &Download) -> Result<(), String> {
    let client = client()?;
    let mut resp = client.head(&download.url).send();
    // Some servers don't do HEAD; one byte is just as good.
    if resp.as_ref().is_ok_and(|r| !r.status().is_success()) {
        resp = client
            .get(&download.url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .send();
    }
    let resp = resp.map_err(|e| format!("{}: {e}", download.url))?;
    std::thread::sleep(Duration::from_secs(1));
    match resp.status().is_success() {
        true => Ok(()),
        false => Err(format!("{}: HTTP {}", download.url, resp.status())),
    }
}

/// The cached file for `download`, fetched first if missing. Cached files
/// are named by their hash, so a changed URL with the same file is free.
pub fn fetch(download: &Download, cache: &Path) -> Result<PathBuf, String> {
    let path = cache.join(format!("{}.zip", download.sha256.to_ascii_lowercase()));
    if path.is_file() {
        return Ok(path);
    }
    fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    eprintln!("  downloading {}", download.url);
    let client = client()?;
    let resp = client
        .get(&download.url)
        .send()
        .map_err(|e| format!("{}: {e}", download.url))?;
    if !resp.status().is_success() {
        return Err(format!("{}: HTTP {}", download.url, resp.status()));
    }
    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    let part = path.with_extension("part");
    fs::write(&part, &bytes).map_err(|e| e.to_string())?;
    fs::rename(&part, &path).map_err(|e| e.to_string())?;
    // Be gentle with the community's file servers.
    std::thread::sleep(Duration::from_secs(1));
    Ok(path)
}

/// Problems with the downloaded archive of `entry`.
pub fn problems(entry: &Entry, download: &Download, data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let hash = sha256_hex(data);
    if !hash.eq_ignore_ascii_case(&download.sha256) {
        out.push(format!("{}: sha256 is {hash}", download.url));
        return out;
    }
    if data.len() as u64 != download.size {
        out.push(format!("{}: size is {}", download.url, data.len()));
    }
    let names = match zip_files(data) {
        Ok(n) => n,
        Err(e) => {
            out.push(format!("{}: not a readable ZIP file ({e})", download.url));
            return out;
        }
    };
    let wanted = entry.recipe.files();
    // All recipe files have to sit together in one folder, the mod's root.
    let found = names.iter().map(|n| dir_of(n)).any(|dir| {
        wanted.iter().all(|w| {
            names
                .iter()
                .any(|n| dir_of(n) == dir && file_of(n).eq_ignore_ascii_case(w))
        })
    });
    if !wanted.is_empty() && !found {
        out.push(format!(
            "{}: no folder holds all of {}",
            download.url,
            wanted.join(", ")
        ));
    }
    out
}

fn zip_files(data: &[u8]) -> Result<Vec<String>, zip::result::ZipError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data))?;
    let mut names = Vec::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        // Reading proves the entry can be extracted (e.g. no unknown method).
        std::io::copy(&mut file.by_ref().take(u64::MAX), &mut std::io::sink())?;
        if let Some(name) = file.enclosed_name() {
            names.push(name.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(names)
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

fn file_of(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, f)| f)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::entry::parse;

    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn entry_for(data: &[u8]) -> Entry {
        parse(&format!(
            "id: x\nname: X\ngame: keen4\nauthors: [a]\nadded: 2026-01-01\ntype: episode\n\
             downloads:\n  - url: https://example.com/x.zip\n    sha256: {}\n    size: {}\n\
             recipe: {{type: patch, patcher: CK4PATCH.EXE, patch: x.pat}}\n",
            sha256_hex(data),
            data.len()
        ))
        .unwrap()
    }

    #[test]
    fn recipe_files_must_share_a_folder() {
        let good = zip(&[("Mod/ck4patch.exe", b"p"), ("Mod/X.PAT", b"x")]);
        let e = entry_for(&good);
        assert_eq!(problems(&e, &e.downloads[0], &good), Vec::<String>::new());

        let split = zip(&[("a/CK4PATCH.EXE", b"p"), ("b/x.pat", b"x")]);
        let e = entry_for(&split);
        assert_eq!(problems(&e, &e.downloads[0], &split).len(), 1);
    }

    #[test]
    fn hash_and_size_are_checked() {
        let data = zip(&[("CK4PATCH.EXE", b"p"), ("x.pat", b"x")]);
        let mut e = entry_for(&data);
        e.downloads[0].size += 1;
        assert_eq!(problems(&e, &e.downloads[0], &data).len(), 1);
        e.downloads[0].sha256 = "00".repeat(32);
        let p = problems(&e, &e.downloads[0], &data);
        assert!(p[0].contains("sha256"), "{p:?}");
        let e = entry_for(b"not a zip");
        assert!(problems(&e, &e.downloads[0], b"not a zip")[0].contains("ZIP"));
    }
}

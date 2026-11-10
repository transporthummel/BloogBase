//! Downloads an entry's archive (once, into a cache) and checks it holds
//! what the entry promises.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::entry::{Download, Entry, Recipe};

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

/// The names of the files in an archive, without their folders.
pub fn file_names(data: &[u8]) -> Result<Vec<String>, String> {
    Ok(zip_files(data)
        .map_err(|e| e.to_string())?
        .iter()
        .map(|n| file_of(n).to_string())
        .collect())
}

/// Problems with the downloaded archive of `entry`. Recipe files named in
/// `inherited` come from the mod it's played over.
pub fn problems(
    entry: &Entry,
    download: &Download,
    data: &[u8],
    inherited: &[String],
) -> Vec<String> {
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
    let wanted: Vec<&str> = entry
        .recipes_of(download)
        .into_iter()
        .flat_map(Recipe::files)
        .chain(entry.cover.as_deref())
        .filter(|w| !inherited.iter().any(|n| n.eq_ignore_ascii_case(w)))
        .collect();
    // All recipe files have to sit together in one folder, the mod's root,
    // and the folders to copy from have to be right in it.
    let holds_all = |dir: &str| {
        wanted.iter().all(|w| {
            names
                .iter()
                .any(|n| dir_of(n) == dir && file_of(n).eq_ignore_ascii_case(w))
        }) && entry.copy_to_root.iter().all(|folder| {
            names.iter().any(|n| {
                let sub = dir_of(n);
                let inner = if dir.is_empty() {
                    Some(sub)
                } else {
                    sub.strip_prefix(dir).and_then(|s| s.strip_prefix('/'))
                };
                inner.is_some_and(|i| i.eq_ignore_ascii_case(folder))
            })
        })
    };
    let root = names.iter().map(|n| dir_of(n)).find(|d| holds_all(d));
    let found = root.is_some();
    for (file, hash) in &download.programs {
        let path = root.and_then(|dir| {
            names
                .iter()
                .find(|n| dir_of(n) == dir && file_of(n).eq_ignore_ascii_case(file))
        });
        let Some(path) = path else {
            out.push(format!("{}: lacks program `{file}`", download.url));
            continue;
        };
        match zip_read(data, path) {
            Ok(bytes) if sha256_hex(&bytes).eq_ignore_ascii_case(hash) => {}
            Ok(bytes) => out.push(format!(
                "{}: sha256 of `{file}` is {}",
                download.url,
                sha256_hex(&bytes)
            )),
            Err(e) => out.push(format!("{}: {path}: {e}", download.url)),
        }
    }
    if !(wanted.is_empty() && entry.copy_to_root.is_empty()) && !found {
        let all: Vec<String> = wanted
            .iter()
            .map(|w| w.to_string())
            .chain(entry.copy_to_root.iter().map(|f| format!("{f}/")))
            .collect();
        out.push(format!(
            "{}: no folder holds all of {}",
            download.url,
            all.join(", ")
        ));
    }
    for file in &entry.provides {
        if !names.iter().any(|n| file_of(n).eq_ignore_ascii_case(file)) {
            out.push(format!(
                "{}: provides `{file}`, which it lacks",
                download.url
            ));
        }
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

fn zip_read(data: &[u8], name: &str) -> Result<Vec<u8>, zip::result::ZipError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data))?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let same = file
            .enclosed_name()
            .is_some_and(|n| n.to_string_lossy().replace('\\', "/") == name);
        if same {
            let mut out = Vec::new();
            file.read_to_end(&mut out)?;
            return Ok(out);
        }
    }
    Err(zip::result::ZipError::FileNotFound)
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
        assert_eq!(
            problems(&e, &e.downloads[0], &good, &[]),
            Vec::<String>::new()
        );

        let split = zip(&[("a/CK4PATCH.EXE", b"p"), ("b/x.pat", b"x")]);
        let e = entry_for(&split);
        assert_eq!(problems(&e, &e.downloads[0], &split, &[]).len(), 1);

        // Levels only, played over a mod that has the patcher and patch.
        let levels = zip(&[("LEVEL01.CK4", b"l")]);
        let e = entry_for(&levels);
        let below = ["ck4patch.exe".to_string(), "X.PAT".to_string()];
        assert_eq!(
            problems(&e, &e.downloads[0], &levels, &below),
            Vec::<String>::new()
        );
        assert_eq!(problems(&e, &e.downloads[0], &levels, &below[..1]).len(), 1);
    }

    #[test]
    fn folders_to_copy_must_be_in_the_root() {
        let data = zip(&[
            ("Mod/CK4PATCH.EXE", b"p"),
            ("Mod/x.pat", b"x"),
            ("Mod/1/A.CK4", b"a"),
        ]);
        let mut e = entry_for(&data);
        e.copy_to_root = vec!["1".into()];
        assert_eq!(
            problems(&e, &e.downloads[0], &data, &[]),
            Vec::<String>::new()
        );
        e.copy_to_root = vec!["2".into()];
        assert_eq!(problems(&e, &e.downloads[0], &data, &[]).len(), 1);
    }

    #[test]
    fn provided_files_must_be_in_it() {
        let data = zip(&[
            ("Mod/CK4PATCH.EXE", b"p"),
            ("Mod/x.pat", b"x"),
            ("Mod/GO.BAT", b"g"),
        ]);
        let mut e = entry_for(&data);
        e.provides = vec!["go.bat".into()];
        assert_eq!(
            problems(&e, &e.downloads[0], &data, &[]),
            Vec::<String>::new()
        );
        e.provides.push("gone.bat".into());
        assert_eq!(problems(&e, &e.downloads[0], &data, &[]).len(), 1);
    }

    #[test]
    fn the_cover_program_must_be_next_to_the_recipe() {
        let data = zip(&[
            ("Mod/CK4PATCH.EXE", b"p"),
            ("Mod/x.pat", b"x"),
            ("Mod/GAME.EXE", b"g"),
            ("Other/MENU.EXE", b"m"),
        ]);
        let mut e = entry_for(&data);
        e.cover = Some("game.exe".into());
        assert_eq!(
            problems(&e, &e.downloads[0], &data, &[]),
            Vec::<String>::new()
        );
        e.cover = Some("MENU.EXE".into());
        assert_eq!(problems(&e, &e.downloads[0], &data, &[]).len(), 1);
    }

    #[test]
    fn programs_are_checked_in_the_mod_root() {
        let data = zip(&[("v1/OSIDOS32.EXE", b"old"), ("v1/TITLE.EXE", b"t")]);
        let mut e = entry_for(&data);
        e.standalone = true;
        e.requires.clear();
        e.recipe = Recipe::Program {
            program: "OSI.EXE".into(),
            args: vec![],
        };
        e.cover = Some("title.exe".into());
        let d = &mut e.downloads[0];
        d.recipe = Some(Recipe::Program {
            program: "osidos32.exe".into(),
            args: vec![],
        });
        d.programs = [
            ("osidos32.exe", sha256_hex(b"old")),
            ("TITLE.EXE", sha256_hex(b"t")),
        ]
        .map(|(n, h)| (n.to_string(), h))
        .into();
        let d = e.downloads[0].clone();
        assert_eq!(problems(&e, &d, &data, &[]), Vec::<String>::new());

        let mut wrong = d.clone();
        wrong
            .programs
            .insert("TITLE.EXE".into(), sha256_hex(b"other"));
        let p = problems(&e, &wrong, &data, &[]);
        assert!(p.len() == 1 && p[0].contains(&sha256_hex(b"t")), "{p:?}");
    }

    #[test]
    fn hash_and_size_are_checked() {
        let data = zip(&[("CK4PATCH.EXE", b"p"), ("x.pat", b"x")]);
        let mut e = entry_for(&data);
        e.downloads[0].size += 1;
        assert_eq!(problems(&e, &e.downloads[0], &data, &[]).len(), 1);
        e.downloads[0].sha256 = "00".repeat(32);
        let p = problems(&e, &e.downloads[0], &data, &[]);
        assert!(p[0].contains("sha256"), "{p:?}");
        let e = entry_for(b"not a zip");
        assert!(problems(&e, &e.downloads[0], b"not a zip", &[])[0].contains("ZIP"));
    }
}

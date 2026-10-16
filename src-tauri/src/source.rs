//! Read-only access to an import source: a folder or a ZIP archive.
//! Entries are addressed by relative, `/`-separated paths.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use walkdir::WalkDir;
use zip::ZipArchive;

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub enum Source {
    Folder(PathBuf),
    Zip(PathBuf),
}

impl Source {
    pub fn open(path: &Path) -> Result<Self> {
        if path.is_dir() {
            Ok(Source::Folder(path.to_path_buf()))
        } else if path.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        {
            Ok(Source::Zip(path.to_path_buf()))
        } else {
            Err(Error::UnsupportedSource(path.to_path_buf()))
        }
    }

    /// All files (not directories), skipping macOS archive junk.
    pub fn files(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        match self {
            Source::Folder(root) => {
                for entry in WalkDir::new(root).follow_links(false) {
                    let entry = entry.map_err(io::Error::from)?;
                    if entry.file_type().is_file() {
                        let rel = entry
                            .path()
                            .strip_prefix(root)
                            .expect("walkdir yields children");
                        out.push(to_rel_string(rel));
                    }
                }
            }
            Source::Zip(path) => {
                let mut zip = open_zip(path)?;
                for i in 0..zip.len() {
                    let file = zip.by_index(i)?;
                    // `enclosed_name` rejects absolute paths and `..` (zip slip).
                    if let (false, Some(name)) = (file.is_dir(), file.enclosed_name()) {
                        out.push(to_rel_string(&name));
                    }
                }
            }
        }
        out.retain(|p| !p.starts_with("__MACOSX/"));
        out.sort();
        Ok(out)
    }

    pub fn read(&self, rel: &str) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.reader(rel, |r| r.read_to_end(&mut buf).map(drop))?;
        Ok(buf)
    }

    /// Copies an entry to `dest`, creating parent directories.
    pub fn copy_to(&self, rel: &str, dest: &Path) -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = File::create(dest)?;
        self.reader(rel, |r| io::copy(r, &mut out).map(drop))
    }

    fn reader(&self, rel: &str, f: impl FnOnce(&mut dyn Read) -> io::Result<()>) -> Result<()> {
        match self {
            Source::Folder(root) => f(&mut File::open(root.join(rel))?)?,
            Source::Zip(path) => {
                let mut zip = open_zip(path)?;
                let index = (0..zip.len())
                    .find(|&i| {
                        zip.by_index_raw(i)
                            .ok()
                            .and_then(|e| e.enclosed_name())
                            .is_some_and(|n| to_rel_string(&n) == rel)
                    })
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, rel.to_string()))?;
                f(&mut zip.by_index(index)?)?
            }
        }
        Ok(())
    }
}

fn open_zip(path: &Path) -> Result<ZipArchive<File>> {
    Ok(ZipArchive::new(File::open(path)?)?)
}

fn to_rel_string(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Splits `a/b/FILE.EXT` into (`a/b`, `FILE.EXT`).
pub fn split_dir(rel: &str) -> (&str, &str) {
    rel.rsplit_once('/').unwrap_or(("", rel))
}

#[cfg(test)]
pub mod testutil {
    use std::io::Write;
    use std::path::Path;

    use zip::write::SimpleFileOptions;

    /// A minimal fake Keen 4 install with one save and its config.
    pub const KEEN4: &[(&str, &[u8])] = &[
        ("KEEN4E.EXE", b"exe"),
        ("GAMEMAPS.CK4", b"maps"),
        ("AUDIO.CK4", b"audio"),
        ("EGAGRAPH.CK4", b"gfx"),
        ("SAVEGAM0.CK4", b"save"),
        ("config.ck4", b"cfg"),
    ];

    /// Writes `(relative path, contents)` pairs into a folder.
    pub fn write_folder(root: &Path, files: &[(&str, &[u8])]) {
        for (rel, data) in files {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, data).unwrap();
        }
    }

    pub fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (rel, data) in files {
            zip.start_file(*rel, SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    const FILES: &[(&str, &[u8])] = &[
        ("A.TXT", b"a"),
        ("sub/B.TXT", b"bb"),
        ("__MACOSX/sub/._B.TXT", b""),
    ];

    fn check(source: &Source, tmp: &Path) {
        assert_eq!(source.files().unwrap(), ["A.TXT", "sub/B.TXT"]);
        assert_eq!(source.read("sub/B.TXT").unwrap(), b"bb");
        let dest = tmp.join("out/x/B.TXT");
        source.copy_to("sub/B.TXT", &dest).unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), b"bb");
        assert!(source.read("MISSING").is_err());
    }

    #[test]
    fn folder_source() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("src");
        write_folder(&root, FILES);
        check(&Source::open(&root).unwrap(), tmp.path());
    }

    #[test]
    fn zip_source() {
        let tmp = tempfile::tempdir().unwrap();
        let zip = tmp.path().join("game.ZIP");
        write_zip(&zip, FILES);
        check(&Source::open(&zip).unwrap(), tmp.path());
    }

    #[test]
    fn rejects_other_files() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("game.rar");
        std::fs::write(&path, b"").unwrap();
        assert!(matches!(
            Source::open(&path),
            Err(Error::UnsupportedSource(_))
        ));
        assert!(Source::open(&tmp.path().join("missing")).is_err());
    }

    #[test]
    fn split() {
        assert_eq!(split_dir("a/b/C.EXE"), ("a/b", "C.EXE"));
        assert_eq!(split_dir("C.EXE"), ("", "C.EXE"));
    }
}

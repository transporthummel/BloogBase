//! Read-only access to an import source: a folder or a ZIP archive.
//! Entries are addressed by relative, `/`-separated paths.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use walkdir::WalkDir;
use zip::{CompressionMethod, ZipArchive, ZipReadOptions};

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
                f(&mut read_entry(&mut zip, index)?.as_slice())?
            }
        }
        Ok(())
    }

    /// Entries of a ZIP whose data doesn't match its checksum, or can't be
    /// read at all. Reading skips the check, as 7-Zip does: in old archives
    /// a file with a bad checksum often still works.
    pub fn damaged(&self) -> Result<Vec<String>> {
        let Source::Zip(path) = self else {
            return Ok(Vec::new());
        };
        let mut zip = open_zip(path)?;
        let mut out = Vec::new();
        for i in 0..zip.len() {
            let file = zip.by_index_raw(i)?;
            let Some(name) = file.enclosed_name().filter(|_| !file.is_dir()) else {
                continue;
            };
            let expected = file.crc32();
            drop(file);
            let intact = read_entry(&mut zip, i).is_ok_and(|data| {
                let mut crc = flate2::Crc::new();
                crc.update(&data);
                crc.sum() == expected
            });
            if !intact {
                out.push(to_rel_string(&name));
            }
        }
        out.retain(|p| !p.starts_with("__MACOSX/"));
        out.sort();
        Ok(out)
    }
}

/// Sent after importing from a ZIP with damaged entries, so the frontend
/// can warn that they may not work.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveDamaged {
    /// The mod's name, or the archive's for games.
    pub name: String,
    pub files: Vec<String>,
}

/// An entry's data, without checking its checksum.
fn read_entry(zip: &mut ZipArchive<File>, index: usize) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    let options = ZipReadOptions::new().ignore_crc32(true);
    let Err(err) = zip
        .by_index_with_options(index, options)?
        .read_to_end(&mut data)
    else {
        return Ok(data);
    };
    let mut raw = zip.by_index_raw(index)?;
    if raw.compression() != CompressionMethod::Deflated {
        return Err(err.into());
    }
    let size = raw.size();
    let mut compressed = Vec::new();
    raw.read_to_end(&mut compressed)?;
    inflate_with_zero_window(&compressed, size).ok_or_else(|| err.into())
}

/// Inflates deflate data that refers back to before its start, taking the
/// bytes there as zeros. Info-ZIP and 7-Zip do the same, so damaged files
/// come out as they do there.
fn inflate_with_zero_window(compressed: &[u8], size: u64) -> Option<Vec<u8>> {
    let mut inflate = flate2::Decompress::new(false);
    inflate.set_dictionary(&[0; 32 * 1024]).ok()?;
    // Never more than the entry claims to hold.
    let mut out = Vec::with_capacity(usize::try_from(size).ok()?);
    match inflate.decompress_vec(compressed, &mut out, flate2::FlushDecompress::Finish) {
        Ok(flate2::Status::StreamEnd) => Some(out),
        _ => None,
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
    fn zip_source_with_spanning_marker() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        // As old PKZIP wrote it: offsets count the marker in.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("old.zip");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"PK00").unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (rel, data) in FILES {
            zip.start_file(*rel, SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
        assert!(std::fs::read(&path).unwrap().starts_with(b"PK00PK"));
        check(&Source::open(&path).unwrap(), tmp.path());
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
    fn zip_entry_with_bad_checksum_is_read_and_reported() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let tmp = tempfile::tempdir().unwrap();
        let zip = tmp.path().join("mod.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zip).unwrap());
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, data) in [("LEVEL01.CK1", b"level one"), ("LEVEL06.CK1", b"level six")] {
            w.start_file(name, stored).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
        // Uncompressed, so this changes the data but not its checksum.
        let mut bytes = std::fs::read(&zip).unwrap();
        let at = bytes.windows(9).position(|w| w == b"level six").unwrap();
        bytes[at] = b'L';
        std::fs::write(&zip, bytes).unwrap();

        let source = Source::open(&zip).unwrap();
        assert_eq!(source.read("LEVEL06.CK1").unwrap(), b"Level six");
        assert_eq!(source.damaged().unwrap(), ["LEVEL06.CK1"]);
        let folder = tmp.path().join("folder");
        write_folder(&folder, FILES);
        assert!(Source::open(&folder).unwrap().damaged().unwrap().is_empty());
    }

    #[test]
    fn deflate_data_referring_back_past_its_start_reads_zeros_there() {
        use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress};
        // Compressed against a window of zeros, the leading zeros become a
        // reference to before the start.
        let data = [&[0; 300][..], b"keen"].concat();
        let mut deflate = Compress::new(Compression::best(), false);
        deflate.set_dictionary(&[0; 32 * 1024]).unwrap();
        let mut compressed = Vec::with_capacity(100);
        deflate
            .compress_vec(&data, &mut compressed, FlushCompress::Finish)
            .unwrap();
        let mut out = Vec::with_capacity(data.len());
        assert!(
            Decompress::new(false)
                .decompress_vec(&compressed, &mut out, FlushDecompress::Finish)
                .is_err()
        );

        assert_eq!(
            inflate_with_zero_window(&compressed, data.len() as u64).unwrap(),
            data
        );
        assert!(inflate_with_zero_window(&compressed, 10).is_none());
    }

    #[test]
    fn split() {
        assert_eq!(split_dir("a/b/C.EXE"), ("a/b", "C.EXE"));
        assert_eq!(split_dir("C.EXE"), ("", "C.EXE"));
    }
}

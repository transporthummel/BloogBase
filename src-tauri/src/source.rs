//! Read-only access to an import source: a folder or a ZIP archive.
//! Entries are addressed by relative, `/`-separated paths.

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use walkdir::WalkDir;
use zip::extra_fields::ExtraField;
use zip::{CompressionMethod, ZipArchive, ZipReadOptions};

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub enum Source {
    Folder(PathBuf),
    Zip(PathBuf, OpenZip),
}

/// A ZIP opened once with its entries by name: mods with a thousand files
/// (XkyKeen4) took minutes to check when every read opened the archive
/// again and searched it. Clones share it; the file closes with the last.
#[derive(Clone, Default)]
pub struct OpenZip(Arc<Mutex<Option<Indexed>>>);

/// The archive and the index of each entry by path.
type Indexed = (ZipArchive<File>, HashMap<String, usize>);

impl fmt::Debug for OpenZip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OpenZip")
    }
}

type Opened<'a> = MutexGuard<'a, Option<Indexed>>;

impl OpenZip {
    fn get(&self, path: &Path) -> Result<Opened<'_>> {
        let mut guard = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            let mut zip = open_zip(path)?;
            let mut names = HashMap::new();
            for i in 0..zip.len() {
                // `enclosed_name` rejects absolute paths and `..` (zip slip).
                if let Some(name) = zip.by_index_raw(i)?.enclosed_name() {
                    names.entry(to_rel_string(&name)).or_insert(i);
                }
            }
            *guard = Some((zip, names));
        }
        Ok(guard)
    }
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
            Ok(Source::Zip(path.to_path_buf(), OpenZip::default()))
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
            Source::Zip(path, open) => {
                let mut guard = open.get(path)?;
                let (zip, _) = guard.as_mut().expect("opened");
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

    /// Copies an entry to `dest`, creating parent directories. The copy keeps
    /// the entry's modification time, so a save that comes with a mod isn't
    /// shown as saved just now.
    pub fn copy_to(&self, rel: &str, dest: &Path) -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = File::create(dest)?;
        self.reader(rel, |r| io::copy(r, &mut out).map(drop))?;
        if let Some(time) = self.modified(rel) {
            out.set_modified(time)?;
        }
        Ok(())
    }

    /// When an entry was last changed, if the source says.
    fn modified(&self, rel: &str) -> Option<SystemTime> {
        match self {
            Source::Folder(root) => std::fs::metadata(root.join(rel)).ok()?.modified().ok(),
            Source::Zip(path, open) => {
                let mut guard = open.get(path).ok()?;
                let (zip, names) = guard.as_mut().expect("opened");
                let entry = zip.by_index_raw(*names.get(rel)?).ok()?;
                // Info-ZIP's extended timestamp is UTC; the DOS time every
                // entry has is local time of an unknown zone, taken as UTC.
                let unix = entry
                    .extra_data_fields()
                    .find_map(|f| match f {
                        ExtraField::ExtendedTimestamp(t) => t.mod_time().map(i64::from),
                        _ => None,
                    })
                    .or_else(|| entry.last_modified().map(|t| dos_to_unix(&t)))?;
                Some(UNIX_EPOCH + Duration::from_secs(u64::try_from(unix).ok()?))
            }
        }
    }

    fn reader(&self, rel: &str, f: impl FnOnce(&mut dyn Read) -> io::Result<()>) -> Result<()> {
        match self {
            Source::Folder(root) => f(&mut File::open(root.join(rel))?)?,
            Source::Zip(path, open) => {
                let mut guard = open.get(path)?;
                let (zip, names) = guard.as_mut().expect("opened");
                let index = *names
                    .get(rel)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, rel.to_string()))?;
                f(&mut read_entry(zip, index)?.as_slice())?
            }
        }
        Ok(())
    }

    /// Entries of a ZIP whose data doesn't match its checksum, or can't be
    /// read at all. Reading skips the check, as 7-Zip does: in old archives
    /// a file with a bad checksum often still works.
    pub fn damaged(&self) -> Result<Vec<String>> {
        let Source::Zip(path, open) = self else {
            return Ok(Vec::new());
        };
        let mut guard = open.get(path)?;
        let (zip, _) = guard.as_mut().expect("opened");
        let mut out = Vec::new();
        for i in 0..zip.len() {
            let file = zip.by_index_raw(i)?;
            let Some(name) = file.enclosed_name().filter(|_| !file.is_dir()) else {
                continue;
            };
            let expected = file.crc32();
            drop(file);
            let intact = read_entry(zip, i).is_ok_and(|data| {
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

/// Larger entries are refused: Keen's files are small, and an archive that
/// claims or unpacks to more is damaged or made to fill the memory.
const MAX_ENTRY: u64 = 256 * 1024 * 1024;

/// An entry's data, without checking its checksum.
fn read_entry(zip: &mut ZipArchive<File>, index: usize) -> Result<Vec<u8>> {
    let too_large = || {
        Error::Extract(format!(
            "a file in the archive is larger than {} MB",
            MAX_ENTRY >> 20
        ))
    };
    let mut data = Vec::new();
    let options = ZipReadOptions::new().ignore_crc32(true);
    let mut entry = zip.by_index_with_options(index, options)?;
    if entry.size() > MAX_ENTRY {
        return Err(too_large());
    }
    let Err(err) = (&mut entry).take(MAX_ENTRY + 1).read_to_end(&mut data) else {
        if data.len() as u64 > MAX_ENTRY {
            return Err(too_large());
        }
        return Ok(data);
    };
    drop(entry);
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

/// Seconds since 1970 of a DOS date and time read as UTC.
fn dos_to_unix(t: &zip::DateTime) -> i64 {
    // Days from the civil date (Howard Hinnant's algorithm).
    let (m, d) = (i64::from(t.month()), i64::from(t.day()));
    let y = i64::from(t.year()) - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + i64::from(t.hour()) * 3600 + i64::from(t.minute()) * 60 + i64::from(t.second())
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
    fn copies_keep_the_modification_time() {
        let tmp = tempfile::tempdir().unwrap();
        let zip = tmp.path().join("mod.zip");
        {
            let mut w = zip::ZipWriter::new(File::create(&zip).unwrap());
            // A save from 2007, as XkyKeen4 ships.
            let time = zip::DateTime::from_date_and_time(2007, 7, 14, 12, 30, 10).unwrap();
            let options = zip::write::SimpleFileOptions::default().last_modified_time(time);
            w.start_file("SAVEGAM0.CK4", options).unwrap();
            std::io::Write::write_all(&mut w, b"save").unwrap();
            w.finish().unwrap();
        }
        let modified = |p: &Path| {
            std::fs::metadata(p)
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        };
        let from_zip = tmp.path().join("a/SAVEGAM0.CK4");
        Source::open(&zip)
            .unwrap()
            .copy_to("SAVEGAM0.CK4", &from_zip)
            .unwrap();
        assert_eq!(modified(&from_zip), 1_184_416_210);

        let from_folder = tmp.path().join("b/SAVEGAM0.CK4");
        Source::open(&tmp.path().join("a"))
            .unwrap()
            .copy_to("SAVEGAM0.CK4", &from_folder)
            .unwrap();
        assert_eq!(modified(&from_folder), 1_184_416_210);
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
    fn zip_entry_claiming_too_much_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let zip = tmp.path().join("bomb.zip");
        write_zip(&zip, &[("GAMEMAPS.CK4", b"maps")]);
        // Claims 4 GB in the local and the central header.
        let mut bytes = std::fs::read(&zip).unwrap();
        for (signature, at) in [(b"PK\x03\x04", 22), (b"PK\x01\x02", 24)] {
            let header = bytes.windows(4).position(|w| w == signature).unwrap();
            bytes[header + at..header + at + 4].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        }
        std::fs::write(&zip, bytes).unwrap();

        let source = Source::open(&zip).unwrap();
        assert!(matches!(
            source.read("GAMEMAPS.CK4"),
            Err(Error::Extract(e)) if e.contains("larger than 256 MB")
        ));
        assert_eq!(source.damaged().unwrap(), ["GAMEMAPS.CK4"]);
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

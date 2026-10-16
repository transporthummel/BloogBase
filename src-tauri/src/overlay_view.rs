//! What DOS sees on an overlay-mounted drive: the lower layer merged with the
//! profile's `state/` directory, with DOSBox Staging's deletion markers applied
//! (PLAN.md §2.2). Every feature that looks at a profile's files goes through
//! this module so they all agree with DOSBox.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::error::Result;

/// `DBOVERLAY_DEL_<NAME>` in an overlay directory hides the lower file `<NAME>`.
pub const OVERLAY_DELETED_FILE_PREFIX: &str = "DBOVERLAY_DEL_";
/// `DBOVERLAY_RMD_<NAME>` hides the lower directory `<NAME>` and its whole subtree.
pub const OVERLAY_REMOVED_DIR_PREFIX: &str = "DBOVERLAY_RMD_";

/// Whether a file name is one of DOSBox's overlay bookkeeping files.
pub fn is_marker(name: &str) -> bool {
    starts_with_ci(name, "DBOVERLAY_")
}

// Byte-wise so non-ASCII host names can't hit a char boundary panic.
fn starts_with_ci(name: &str, prefix: &str) -> bool {
    name.as_bytes()
        .get(..prefix.len())
        .is_some_and(|b| b.eq_ignore_ascii_case(prefix.as_bytes()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Lower,
    Upper,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `/`-separated path relative to the drive root, as spelled on the host.
    pub path: String,
    pub layer: Layer,
    pub size: u64,
}

/// All files visible through the overlay, sorted by path. Names are compared
/// case-insensitively like DOS does; markers themselves are never listed.
pub fn list(lower: &Path, upper: &Path) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    merge(Some(lower), Some(upper), "", &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Prepares writing `rel` into the overlay from outside DOSBox (e.g. restoring
/// a save): removes the deletion marker that would otherwise keep hiding it.
#[allow(dead_code)] // First caller: save restore (M7).
pub fn prepare_write(upper: &Path, rel: &str) -> Result<()> {
    let (dir, name) = rel.rsplit_once('/').unwrap_or(("", rel));
    let dir = if dir.is_empty() {
        upper.to_path_buf()
    } else {
        upper.join(dir)
    };
    let marker = format!("{OVERLAY_DELETED_FILE_PREFIX}{name}");
    for entry in read_dir(Some(&dir))? {
        if entry.name.eq_ignore_ascii_case(&marker) {
            fs::remove_file(&entry.path)?;
        }
    }
    Ok(())
}

struct DirEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    size: u64,
}

fn read_dir(dir: Option<&Path>) -> Result<Vec<DirEntry>> {
    let Some(dir) = dir else {
        return Ok(Vec::new());
    };
    let iter = match fs::read_dir(dir) {
        Ok(iter) => iter,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for entry in iter {
        let entry = entry?;
        let meta = entry.metadata()?;
        out.push(DirEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: entry.path(),
            is_dir: meta.is_dir(),
            size: meta.len(),
        });
    }
    Ok(out)
}

fn strip_prefix_ci<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    (starts_with_ci(name, prefix) && name.len() > prefix.len()).then(|| &name[prefix.len()..])
}

fn merge(
    lower: Option<&Path>,
    upper: Option<&Path>,
    rel: &str,
    out: &mut Vec<Entry>,
) -> Result<()> {
    let join = |name: &str| {
        if rel.is_empty() {
            name.to_string()
        } else {
            format!("{rel}/{name}")
        }
    };

    let mut deleted = Vec::new();
    let mut removed_dirs = Vec::new();
    // Upper-cased name → (upper entry, lower entry).
    let mut names: BTreeMap<String, (Option<DirEntry>, Option<DirEntry>)> = BTreeMap::new();

    for e in read_dir(upper)? {
        if !e.is_dir {
            if let Some(n) = strip_prefix_ci(&e.name, OVERLAY_DELETED_FILE_PREFIX) {
                deleted.push(n.to_ascii_uppercase());
                continue;
            }
            if let Some(n) = strip_prefix_ci(&e.name, OVERLAY_REMOVED_DIR_PREFIX) {
                removed_dirs.push(n.to_ascii_uppercase());
                continue;
            }
            if is_marker(&e.name) {
                continue;
            }
        }
        let key = e.name.to_ascii_uppercase();
        names.entry(key).or_default().0 = Some(e);
    }
    for e in read_dir(lower)? {
        let key = e.name.to_ascii_uppercase();
        let hidden = if e.is_dir {
            removed_dirs.contains(&key)
        } else {
            deleted.contains(&key)
        };
        if !hidden {
            names.entry(key).or_default().1 = Some(e);
        }
    }

    for (upper, lower) in names.into_values() {
        match (upper, lower) {
            (Some(u), lower) if u.is_dir => {
                // A lower file can't show through an upper directory of the same name.
                let lower_dir = lower.filter(|l| l.is_dir).map(|l| l.path);
                merge(lower_dir.as_deref(), Some(&u.path), &join(&u.name), out)?;
            }
            // An upper file shadows whatever the lower layer has under that name.
            (Some(u), _) => out.push(Entry {
                path: join(&u.name),
                layer: Layer::Upper,
                size: u.size,
            }),
            (None, Some(l)) if l.is_dir => merge(Some(&l.path), None, &join(&l.name), out)?,
            (None, Some(l)) => out.push(Entry {
                path: join(&l.name),
                layer: Layer::Lower,
                size: l.size,
            }),
            (None, None) => unreachable!("every name comes from one of the layers"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::testutil::write_folder;

    struct Fixture {
        _tmp: tempfile::TempDir,
        lower: PathBuf,
        upper: PathBuf,
    }

    fn fixture(lower: &[(&str, &[u8])], upper: &[(&str, &[u8])]) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let (l, u) = (tmp.path().join("lower"), tmp.path().join("upper"));
        fs::create_dir_all(&l).unwrap();
        fs::create_dir_all(&u).unwrap();
        write_folder(&l, lower);
        write_folder(&u, upper);
        Fixture {
            _tmp: tmp,
            lower: l,
            upper: u,
        }
    }

    fn view(f: &Fixture) -> Vec<(String, Layer)> {
        list(&f.lower, &f.upper)
            .unwrap()
            .into_iter()
            .map(|e| (e.path, e.layer))
            .collect()
    }

    fn v(entries: &[(&str, Layer)]) -> Vec<(String, Layer)> {
        entries.iter().map(|(p, l)| (p.to_string(), *l)).collect()
    }

    use Layer::{Lower, Upper};

    #[test]
    fn marker_names() {
        assert!(is_marker("DBOVERLAY_DEL_TEST.TXT"));
        assert!(is_marker("dboverlay_rmd_x"));
        assert!(!is_marker("DBOVERLAY"));
        assert!(!is_marker("SAVEGAM0.CK4"));
    }

    #[test]
    fn merges_layers_and_upper_wins() {
        let f = fixture(
            &[("KEEN4E.EXE", b"exe"), ("CONFIG.CK4", b"old")],
            &[("config.ck4", b"new!"), ("SAVEGAM0.CK4", b"s")],
        );
        assert_eq!(
            view(&f),
            v(&[
                ("KEEN4E.EXE", Lower),
                ("SAVEGAM0.CK4", Upper),
                ("config.ck4", Upper)
            ])
        );
        let sizes: Vec<u64> = list(&f.lower, &f.upper)
            .unwrap()
            .iter()
            .map(|e| e.size)
            .collect();
        assert_eq!(sizes, [3, 1, 4]);
    }

    #[test]
    fn deleted_file_marker_hides_lower_file() {
        let f = fixture(
            &[("TEST.TXT", b"x"), ("KEEP.TXT", b"x")],
            &[("DBOVERLAY_DEL_TEST.TXT", b"empty")],
        );
        assert_eq!(view(&f), v(&[("KEEP.TXT", Lower)]));
    }

    #[test]
    fn markers_match_case_insensitively() {
        let f = fixture(&[("test.txt", b"x")], &[("dboverlay_del_TEST.TXT", b"")]);
        assert!(view(&f).is_empty());
    }

    #[test]
    fn markers_in_subdirectories() {
        let f = fixture(
            &[("SUB/A.TXT", b"x"), ("SUB/B.TXT", b"x")],
            &[("SUB/DBOVERLAY_DEL_A.TXT", b"empty")],
        );
        assert_eq!(view(&f), v(&[("SUB/B.TXT", Lower)]));
    }

    #[test]
    fn removed_dir_marker_hides_lower_subtree() {
        let f = fixture(
            &[
                ("TEST/A.TXT", b"x"),
                ("TEST/DEEP/B.TXT", b"x"),
                ("C.TXT", b"x"),
            ],
            &[("DBOVERLAY_RMD_TEST", b"empty")],
        );
        assert_eq!(view(&f), v(&[("C.TXT", Lower)]));
    }

    #[test]
    fn recreated_dir_shows_only_upper_content() {
        let f = fixture(
            &[("TEST/OLD.TXT", b"x")],
            &[("DBOVERLAY_RMD_TEST", b"empty"), ("TEST/NEW.TXT", b"x")],
        );
        assert_eq!(view(&f), v(&[("TEST/NEW.TXT", Upper)]));
    }

    #[test]
    fn recreated_file_wins_over_stale_marker() {
        let f = fixture(
            &[("TEST.TXT", b"x")],
            &[("DBOVERLAY_DEL_TEST.TXT", b"empty"), ("TEST.TXT", b"y")],
        );
        assert_eq!(view(&f), v(&[("TEST.TXT", Upper)]));
    }

    #[test]
    fn delete_marker_does_not_hide_directories() {
        let f = fixture(&[("SUB/A.TXT", b"x")], &[("DBOVERLAY_DEL_SUB", b"empty")]);
        assert_eq!(view(&f), v(&[("SUB/A.TXT", Lower)]));
    }

    #[test]
    fn missing_layers_are_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            list(&tmp.path().join("a"), &tmp.path().join("b"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn prepare_write_removes_matching_marker_only() {
        let f = fixture(
            &[("SUB/A.TXT", b"x"), ("B.TXT", b"x")],
            &[
                ("SUB/dboverlay_del_a.txt", b"empty"),
                ("DBOVERLAY_DEL_B.TXT", b"empty"),
            ],
        );
        prepare_write(&f.upper, "SUB/A.TXT").unwrap();
        assert_eq!(view(&f), v(&[("SUB/A.TXT", Lower)]));
        assert!(f.upper.join("DBOVERLAY_DEL_B.TXT").exists());
        // Nothing to do is fine, also for missing directories.
        prepare_write(&f.upper, "NEW/C.TXT").unwrap();
    }
}

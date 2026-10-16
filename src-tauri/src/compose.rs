//! Composed lower layers for mod profiles: the install's base files with the
//! mod's files copied on top (PLAN.md §4.2). Disposable; rebuilt whenever
//! the inputs change.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::params;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::db::Db;
use crate::detect::hex;
use crate::error::Result;
use crate::paths::AppPaths;
use crate::recipe::{Recipe, has_ext};

/// Archives inside a mod (extras, sources) are never needed by the game.
const SKIPPED: &[&str] = &["ZIP", "RAR", "7Z"];

struct Input {
    path: PathBuf,
    /// As spelled on disk, `/`-separated.
    rel: String,
    size: u64,
    modified: u128,
}

/// Returns the composed directory of `profile_id`, building it if it's
/// missing or out of date.
pub fn ensure(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    base: &Path,
    mod_files: &Path,
    recipe: &Recipe,
) -> Result<PathBuf> {
    // Upper-cased path → file. DOS names are case-insensitive, so a mod's
    // `level01.ck1` replaces the base's `LEVEL01.CK1`.
    let mut inputs: BTreeMap<String, Input> = BTreeMap::new();
    for (root, is_mod) in [(base, false), (mod_files, true)] {
        for input in scan(root)? {
            if is_mod && SKIPPED.iter().any(|e| has_ext(&input.rel, e)) {
                continue;
            }
            inputs.insert(input.rel.to_ascii_uppercase(), input);
        }
    }

    let hash = composition_hash(&inputs, recipe);
    let dir = paths.composed(profile_id);
    let stored: Option<String> = db.conn().query_row(
        "SELECT composed_hash FROM profiles WHERE id = ?1",
        [profile_id],
        |r| r.get(0),
    )?;
    if stored.as_deref() == Some(hash.as_str()) && dir.is_dir() {
        return Ok(dir);
    }

    tracing::info!("composing {profile_id}: {} files", inputs.len());
    let tmp = dir.with_extension("tmp");
    let _ = fs::remove_dir_all(&tmp);
    for input in inputs.values() {
        let dest = tmp.join(&input.rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&input.path, dest)?;
    }
    fs::create_dir_all(&tmp)?;
    match fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    fs::rename(&tmp, &dir)?;
    db.conn().execute(
        "UPDATE profiles SET composed_hash = ?2 WHERE id = ?1",
        params![profile_id, hash],
    )?;
    Ok(dir)
}

fn scan(root: &Path) -> Result<Vec<Input>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(io::Error::from)?;
        if !entry.file_type().is_file() {
            continue;
        }
        let meta = entry.metadata().map_err(io::Error::from)?;
        let rel = entry
            .path()
            .strip_prefix(root)
            .expect("walkdir yields children")
            .to_string_lossy()
            .replace('\\', "/");
        out.push(Input {
            path: entry.path().to_path_buf(),
            rel,
            size: meta.len(),
            modified: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos()),
        });
    }
    Ok(out)
}

/// Base and mod files are never modified in place, so names, sizes and
/// modification times identify them well enough without reading them.
fn composition_hash(inputs: &BTreeMap<String, Input>, recipe: &Recipe) -> String {
    let mut h = Sha256::new();
    h.update(b"composed v1\n");
    h.update(serde_json::to_string(recipe).expect("recipes serialize"));
    for input in inputs.values() {
        h.update(format!(
            "\n{}\t{}\t{}\t{}",
            input.rel,
            input.size,
            input.modified,
            input.path.display()
        ));
    }
    hex(&h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::testutil::*;

    #[test]
    fn composes_mod_over_base_case_insensitively_and_reuses_result() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO installs (id, game_id, label, source, path, added_at)
                 VALUES ('keen1-1', 'keen1', 'x', 'folder', '/x', 0);
                 INSERT INTO profiles (id, install_id, state_path, created_at)
                 VALUES ('p', 'keen1-1', '/s', 0);",
            )
            .unwrap();
        let base = tmp.path().join("base");
        let modf = tmp.path().join("mod");
        write_folder(&base, &[("KEEN1.EXE", b"exe"), ("LEVEL01.CK1", b"old")]);
        write_folder(
            &modf,
            &[
                ("level01.ck1", b"new"),
                ("sub/X.TXT", b"x"),
                ("EXTRAS.rar", b"big"),
            ],
        );

        let dir = ensure(&paths, &db, "p", &base, &modf, &Recipe::Episode).unwrap();
        assert_eq!(dir, paths.composed("p"));
        let mut names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["KEEN1.EXE", "level01.ck1", "sub"]);
        assert_eq!(fs::read(dir.join("level01.ck1")).unwrap(), b"new");
        assert!(dir.join("sub/X.TXT").is_file());

        // Unchanged inputs: the directory is reused as is.
        fs::write(dir.join("MARK"), b"").unwrap();
        ensure(&paths, &db, "p", &base, &modf, &Recipe::Episode).unwrap();
        assert!(dir.join("MARK").exists());

        // A different recipe or changed file rebuilds it.
        let batch = Recipe::Batch {
            file: "GO.BAT".into(),
        };
        ensure(&paths, &db, "p", &base, &modf, &batch).unwrap();
        assert!(!dir.join("MARK").exists());
        fs::write(modf.join("NEW.CK1"), b"n").unwrap();
        ensure(&paths, &db, "p", &base, &modf, &batch).unwrap();
        assert!(dir.join("NEW.CK1").exists());
        assert!(!paths.cache.join("composed/p.tmp").exists());
    }
}

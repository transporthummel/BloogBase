//! The files of an index mod's variants (PLAN.md §9.7): each variant is a
//! folder of the download or of one of its `parts`, with its own recipe,
//! folders copied into its root and renamed files. Installed into
//! `variants/<id>/` of the mod's folder, each a whole mod root.

use std::path::{Path, PathBuf};

use crate::catalog;
use crate::detect::sha256_hex;
use crate::error::{Error, Result};
use crate::mod_index::{Fingerprint, Variant, fingerprint_sha256};
use crate::recipe::Recipe;
use crate::source::{Source, split_dir};

use super::analyze::{Analysis, analyze_in};
use super::install::Curated;

/// A variant's files, as found in its archive.
pub(super) struct VariantFiles {
    pub(super) id: String,
    pub(super) source: Source,
    pub(super) analysis: Analysis,
}

/// The recipes variant `v` of `c` starts with: its own, else the entry's.
pub(super) fn recipes_of<'a>(c: &'a Curated, v: &'a Variant) -> Vec<&'a Recipe> {
    match &v.recipe {
        Some(r) => vec![r],
        None => std::iter::once(&c.recipe)
            .chain(c.version_recipes.values())
            .collect(),
    }
}

/// The archive variant `v` of `c` is installed from: `main`, or the part it
/// names, which the caller downloaded (`c.part_files`).
fn archive_of<'a>(c: &'a Curated, v: &Variant, main: &'a Path) -> Result<&'a Path> {
    match &v.archive {
        None => Ok(main),
        Some(key) => c
            .part_files
            .get(key)
            .map(PathBuf::as_path)
            .ok_or_else(|| Error::NotFound(format!("archive {key} of {}", c.index_id))),
    }
}

/// The files of each variant of `c`, from the download `main` and its parts.
pub(super) fn analyze_variants(c: &Curated, main: &Path) -> Result<Vec<VariantFiles>> {
    let mut out = Vec::new();
    for v in &c.variants {
        let path = archive_of(c, v, main)?;
        let source = Source::open(path)?;
        let root = match v
            .root
            .as_deref()
            .or(c.root.as_deref().filter(|_| v.archive.is_none()))
        {
            Some(r) => Some(r.to_string()),
            None => recipe_root(&source, &recipes_of(c, v))?,
        };
        let mut analysis = analyze_in(&source, path, Some(&c.game_id), root.as_deref())
            .map_err(|e| in_variant(&v.id, e))?;
        let folders = v.copy_to_root.as_ref().unwrap_or(&c.copy_to_root);
        analysis.copies =
            super::install::copies_to_root(&analysis, folders).map_err(|e| in_variant(&v.id, e))?;
        // Keen 1-3's settings stay out, as `analyze` leaves them out of the root.
        let game = catalog::game(&analysis.info.game_id);
        analysis.copies.retain(|(_, name)| {
            !(name.to_ascii_uppercase().starts_with("CTLPANEL.")
                && game.is_some_and(|g| g.is_state_file(name)))
        });
        for (name, from) in &v.files {
            let rel = if analysis.root.is_empty() {
                from.clone()
            } else {
                format!("{}/{from}", analysis.root)
            };
            let found = analysis
                .files
                .iter()
                .find(|f| f.eq_ignore_ascii_case(&rel))
                .ok_or_else(|| {
                    in_variant(
                        &v.id,
                        Error::UnexpectedContent(format!("{from} is missing")),
                    )
                })?;
            analysis.copies.push((found.clone(), name.clone()));
        }
        out.push(VariantFiles {
            id: v.id.clone(),
            source,
            analysis,
        });
    }
    Ok(out)
}

fn in_variant(id: &str, e: Error) -> Error {
    match e {
        Error::UnexpectedContent(m) => Error::UnexpectedContent(format!("variant {id}: {m}")),
        e => e,
    }
}

/// The first folder of `source` holding all files `recipes` run; `None`
/// when they run nothing, so the folder is found as for any mod.
fn recipe_root(source: &Source, recipes: &[&Recipe]) -> Result<Option<String>> {
    let wanted: Vec<&str> = recipes.iter().flat_map(|r| r.files()).collect();
    if wanted.is_empty() {
        return Ok(None);
    }
    let files = source.files()?;
    let holds = |dir: &str| {
        wanted.iter().all(|w| {
            files.iter().any(|f| {
                let (d, n) = split_dir(f);
                d == dir && n.eq_ignore_ascii_case(w)
            })
        })
    };
    files
        .iter()
        .map(|f| split_dir(f).0)
        .find(|d| holds(d))
        .map(|d| Some(d.to_string()))
        .ok_or_else(|| {
            Error::UnexpectedContent(format!("no folder holds all of {}", wanted.join(", ")))
        })
}

/// The variant of `c` whose folder of the download `main` a mod installed
/// before the entry had variants holds: the one the app found then.
pub(super) fn found_before(c: &Curated, main: &Path) -> Option<String> {
    let source = Source::open(main).ok()?;
    let found = analyze_in(&source, main, Some(&c.game_id), c.root.as_deref())
        .ok()?
        .root;
    c.variants
        .iter()
        .filter(|v| v.archive.is_none())
        .find(|v| {
            let root = match v.root.as_deref().or(c.root.as_deref()) {
                Some(r) => Some(r.to_string()),
                None => recipe_root(&source, &recipes_of(c, v)).ok().flatten(),
            };
            root.is_some_and(|r| r.eq_ignore_ascii_case(&found))
        })
        .map(|v| v.id.clone())
}

/// Checks that the files the recipes of each variant run are in its root.
pub(super) fn check_recipes(c: &Curated, variants: &[VariantFiles]) -> Result<()> {
    for (v, files) in c.variants.iter().zip(variants) {
        let names = root_names(&files.analysis);
        for file in recipes_of(c, v).into_iter().flat_map(|r| r.files()) {
            if !names.iter().any(|n| n.eq_ignore_ascii_case(file)) {
                return Err(Error::UnexpectedContent(format!(
                    "variant {}: {file} is missing",
                    v.id
                )));
            }
        }
    }
    Ok(())
}

/// The names right in the root as installed: its own files and copies.
fn root_names(a: &Analysis) -> Vec<String> {
    a.files
        .iter()
        .filter(|f| split_dir(f).0 == a.root)
        .map(|f| split_dir(f).1.to_string())
        .chain(a.copies.iter().map(|(_, name)| name.clone()))
        .collect()
}

/// Copies each variant's files into `staging/variants/<id>/`.
pub(super) fn stage(variants: &[VariantFiles], staging: &Path) -> Result<()> {
    for v in variants {
        let dir = staging.join("variants").join(&v.id);
        std::fs::create_dir_all(&dir)?;
        let a = &v.analysis;
        for rel in &a.files {
            let inner = if a.root.is_empty() {
                rel.as_str()
            } else {
                &rel[a.root.len() + 1..]
            };
            v.source.copy_to(rel, &dir.join(inner))?;
        }
        for (rel, inner) in &a.copies {
            // Another spelling of the name would be a second file on Linux.
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.eq_ignore_ascii_case(inner) && name != *inner {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
            v.source.copy_to(rel, &dir.join(inner))?;
        }
    }
    Ok(())
}

/// The fingerprint of a variant's root as installed (`dir`), as the index
/// keeps it: the files right in it but documents, saves and settings.
#[cfg(test)]
pub fn fingerprint_of(dir: &Path, game_id: &str) -> Result<Option<Fingerprint>> {
    let game = catalog::game(game_id);
    let mut files: Vec<(String, String)> = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        if !e.file_type()?.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if super::index_add::is_document(&name) || game.is_some_and(|g| g.is_state_file(&name)) {
            continue;
        }
        files.push((name, sha256_hex(&std::fs::read(e.path())?)));
    }
    if files.is_empty() {
        return Ok(None);
    }
    files.sort_by_key(|(name, _)| name.to_ascii_uppercase());
    Ok(Some(Fingerprint {
        sha256: fingerprint_sha256(&files),
        files: files.into_iter().map(|(name, _)| name).collect(),
    }))
}

/// Whether the files of `dir` are those of `f`, byte for byte; others
/// beside them don't matter.
pub fn matches(dir: &Path, f: &Fingerprint) -> bool {
    let names: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let hashes: Option<Vec<(String, String)>> = f
        .files
        .iter()
        .map(|file| {
            let name = names.iter().find(|n| n.eq_ignore_ascii_case(file))?;
            Some((
                file.clone(),
                sha256_hex(&std::fs::read(dir.join(name)).ok()?),
            ))
        })
        .collect();
    hashes.is_some_and(|h| fingerprint_sha256(&h).eq_ignore_ascii_case(&f.sha256))
}

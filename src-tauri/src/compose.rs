//! Composed lower layers: the install's base files with a mod's files copied
//! on top (PLAN.md §4.2), the episode unpacked for patchers that need it, a
//! DPMI host for DJGPP programs, CKPatch for mods without one, and
//! one-button firing patched in where it's on. Disposable; rebuilt whenever the inputs change.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use keen_graphics::lzexe;
use rusqlite::params;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::catalog::Game;
use crate::ckpatch;
use crate::db::Db;
use crate::detect::hex;
use crate::error::Result;
use crate::firing;
use crate::paths::AppPaths;
use crate::profiles::ModLayer;
use crate::recipe::{self, Recipe, has_ext};

/// Archives inside a mod (extras, sources) are never needed by the game.
const SKIPPED: &[&str] = &["ZIP", "RAR", "7Z"];

/// CWSDPMI r7, unmodified, which DJGPP programs load for DPMI; DOSBox has
/// none built in. Its license and source: `assets/cwsdpmi/cwsdpmi.doc`.
const CWSDPMI: &[u8] = include_bytes!("../assets/cwsdpmi/CWSDPMI.EXE");

struct Input {
    path: PathBuf,
    /// As spelled on disk, `/`-separated.
    rel: String,
    size: u64,
    modified: u128,
}

/// Returns the composed directory of `profile_id`, a profile of `game`,
/// building it if it's missing or out of date. With `firing`, one-button
/// firing is patched in where it fits.
pub fn ensure(
    paths: &AppPaths,
    db: &Db,
    profile_id: &str,
    base: &Path,
    mod_layer: Option<&ModLayer>,
    game: &Game,
    firing: bool,
) -> Result<PathBuf> {
    let recipe = mod_layer.map_or(&Recipe::Episode, |m| &m.recipe);
    // Upper-cased path → file. DOS names are case-insensitive, so a mod's
    // `level01.ck1` replaces the base's `LEVEL01.CK1`.
    let mut inputs: BTreeMap<String, Input> = BTreeMap::new();
    let mod_root = mod_layer.map(|m| (m.files.as_path(), true));
    for (root, is_mod) in std::iter::once((base, false)).chain(mod_root) {
        for input in scan(root)? {
            if is_mod && SKIPPED.iter().any(|e| has_ext(&input.rel, e)) {
                continue;
            }
            inputs.insert(input.rel.to_ascii_uppercase(), input);
        }
    }

    let hash = composition_hash(&inputs, recipe, firing);
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
    if patcher_needs_unpacked(recipe, &inputs) {
        unpack_episode(game, &inputs, &tmp)?;
    }
    if needs_dpmi(&inputs) {
        fs::write(tmp.join("CWSDPMI.EXE"), CWSDPMI)?;
    }
    if let Some((name, exe)) = missing_patcher(game, &inputs) {
        fs::write(tmp.join(name), exe)?;
    }
    if firing {
        let status = firing::apply(game, recipe, &tmp)?;
        tracing::info!("one-button firing for {profile_id}: {status:?}");
    }
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

/// The file at the root that DOS runs for `name`, trying `exts` if it has
/// none.
fn find<'a>(inputs: &'a BTreeMap<String, Input>, name: &str, exts: &[&str]) -> Option<&'a Input> {
    let name = name
        .trim_start_matches(".\\")
        .trim_start_matches("C:\\")
        .to_ascii_uppercase();
    inputs
        .get(&name)
        .or_else(|| exts.iter().find_map(|e| inputs.get(&format!("{name}.{e}"))))
}

/// Whether `recipe` runs a patcher that only takes executables unpacked with
/// UNLZEXE, as CKPatch before 0.9 does. Its error says so: "Did you remember
/// to UNLZEXE the executable?"; mods using it may leave that to the player.
fn patcher_needs_unpacked(recipe: &Recipe, inputs: &BTreeMap<String, Input>) -> bool {
    let patchers: Vec<&Input> = match recipe {
        Recipe::Patch { patcher, .. } => find(inputs, patcher, &[]).into_iter().collect(),
        Recipe::Batch { file } => find(inputs, file, &[])
            .and_then(|b| fs::read(&b.path).ok())
            .map(|text| recipe::batch_commands(&String::from_utf8_lossy(&text)))
            .unwrap_or_default()
            .iter()
            .filter_map(|cmd| find(inputs, &cmd[0], &["EXE", "COM"]))
            .filter(|p| recipe::is_patcher_at(&p.path))
            .collect(),
        Recipe::Program { .. } | Recipe::Episode => Vec::new(),
    };
    patchers.iter().any(|p| {
        fs::read(&p.path)
            .ok()
            .and_then(|exe| lzexe::load_image(&exe).ok())
            .is_some_and(|image| image.windows(7).any(|w| w == b"UNLZEXE"))
    })
}

/// Writes the episode's executables in `dir` unpacked, as UNLZEXE does.
fn unpack_episode(game: &Game, inputs: &BTreeMap<String, Input>, dir: &Path) -> Result<()> {
    for input in game.executables.iter().filter_map(|e| find(inputs, e, &[])) {
        let exe = fs::read(&input.path)?;
        if !lzexe::is_lzexe91(&exe) {
            continue;
        }
        match lzexe::unpack_exe(&exe) {
            Ok(unpacked) => fs::write(dir.join(&input.rel), unpacked)?,
            Err(e) => tracing::warn!("could not unpack {}: {e}", input.rel),
        }
    }
    Ok(())
}

/// Whether a program at the root was built with DJGPP and has no
/// CWSDPMI.EXE next to it, as Simulator Keen's level generator. Without it,
/// it stops with "no DPMI - Get csdpmi*b.zip".
fn needs_dpmi(inputs: &BTreeMap<String, Input>) -> bool {
    !inputs.contains_key("CWSDPMI.EXE")
        && inputs
            .values()
            .any(|i| !i.rel.contains('/') && has_ext(&i.rel, "EXE") && is_djgpp(&i.path))
}

/// The bundled CKPatch, where a patch file at the root has no patcher next
/// to it.
fn missing_patcher(
    game: &Game,
    inputs: &BTreeMap<String, Input>,
) -> Option<(&'static str, &'static [u8])> {
    let root: Vec<&Input> = inputs.values().filter(|i| !i.rel.contains('/')).collect();
    let text = |i: &Input| {
        (i.size <= 1 << 20)
            .then(|| fs::read(&i.path).ok())
            .flatten()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    let is_patcher = |name: &str| {
        recipe::is_patcher(name)
            || root
                .iter()
                .any(|i| i.rel == name && recipe::is_patcher_at(&i.path))
    };
    ckpatch::needed(
        &game.id,
        root.iter().map(|i| (i.rel.as_str(), text(i))),
        is_patcher,
    )
    .then(|| ckpatch::bundled(&game.id))
    .flatten()
}

/// DJGPP's stub loader fills the first 2 KB of its programs.
fn is_djgpp(path: &Path) -> bool {
    let mut stub = Vec::new();
    File::open(path)
        .and_then(|f| f.take(2048).read_to_end(&mut stub))
        .is_ok()
        && stub.windows(8).any(|w| w == b"go32stub")
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
fn composition_hash(inputs: &BTreeMap<String, Input>, recipe: &Recipe, firing: bool) -> String {
    let mut h = Sha256::new();
    // v2: the episode unpacked for old patchers; v3: CWSDPMI; v4: CKPatch.
    h.update(b"composed v4\n");
    h.update(serde_json::to_string(recipe).expect("recipes serialize"));
    if firing {
        h.update(b"\none-button firing v1");
    }
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
    use crate::catalog;
    use crate::source::testutil::*;

    fn keen1() -> &'static Game {
        catalog::game("keen1").unwrap()
    }

    fn setup(tmp: &Path) -> (AppPaths, Db) {
        let paths = AppPaths::new(tmp.join("data"));
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
        (paths, db)
    }

    /// An executable packed with LZEXE 0.91 whose program is `ABC`.
    fn packed_exe() -> Vec<u8> {
        let mut exe = vec![0u8; 0x20];
        let mut set = |at: usize, w: u16| exe[at..at + 2].copy_from_slice(&w.to_le_bytes());
        set(0, 0x5A4D);
        set(8, 2); // header paragraphs
        set(0x16, 1); // the unpacker's segment
        exe[0x1C..0x20].copy_from_slice(b"LZ91");
        // Three literals, then the end: bits 1 1 1 0 1, a zero word and byte.
        exe.extend([0x17, 0x00, b'A', b'B', b'C', 0, 0, 0]);
        exe.resize(0x30, 0);
        // Registers and sizes, then no relocations.
        exe.resize(0x30 + 0x158, 0);
        exe.extend([0, 1, 0]);
        exe
    }

    #[test]
    fn old_patchers_get_the_episode_unpacked() {
        let tmp = tempfile::tempdir().unwrap();
        let (paths, db) = setup(tmp.path());
        let base = tmp.path().join("base");
        let modf = tmp.path().join("mod");
        write_folder(&base, &[("KEEN1.EXE", &packed_exe())]);
        let old = [b"MZ".as_slice(), &[0; 0x1E], b"Did you remember to UNLZEXE"].concat();
        let new = [b"MZ".as_slice(), &[0; 0x1E], b"Commander Keen Patch"].concat();
        let unpacked = |patcher: &[u8], recipe: Recipe| {
            write_folder(
                &modf,
                &[("ck1patch.exe", patcher), ("GO.BAT", b"ck1patch m.pat")],
            );
            let layer = ModLayer {
                files: modf.clone(),
                recipe,
            };
            let dir = ensure(&paths, &db, "p", &base, Some(&layer), keen1(), false).unwrap();
            let exe = fs::read(dir.join("KEEN1.EXE")).unwrap();
            assert_eq!(lzexe::load_image(&exe).unwrap(), b"ABC");
            !lzexe::is_lzexe91(&exe)
        };
        let patch = Recipe::Patch {
            patcher: "CK1PATCH.EXE".into(),
            patch: "m.pat".into(),
            args: Vec::new(),
        };
        let batch = Recipe::Batch {
            file: "GO.BAT".into(),
        };
        assert!(unpacked(&old, patch.clone()));
        assert!(unpacked(&old, batch.clone()));
        assert!(!unpacked(&new, patch));
        assert!(!unpacked(&new, batch));
        assert!(!unpacked(&old, Recipe::Episode));

        // A renamed one, run by a batch file, as Norp the Yorp 3 ships.
        let renamed = [old.as_slice(), b"\0Usage: CK1PATCH {patchfile|patchdir}"].concat();
        write_folder(
            &modf,
            &[("norper.exe", &renamed), ("RUN.BAT", b"norper m.pat")],
        );
        let layer = ModLayer {
            files: modf.clone(),
            recipe: Recipe::Batch {
                file: "RUN.BAT".into(),
            },
        };
        let dir = ensure(&paths, &db, "p", &base, Some(&layer), keen1(), false).unwrap();
        assert!(!lzexe::is_lzexe91(
            &fs::read(dir.join("KEEN1.EXE")).unwrap()
        ));
    }

    #[test]
    fn djgpp_programs_get_a_dpmi_host() {
        let tmp = tempfile::tempdir().unwrap();
        let (paths, db) = setup(tmp.path());
        let base = tmp.path().join("base");
        let modf = tmp.path().join("mod");
        write_folder(&base, &[("KEEN1.EXE", b"MZ")]);
        let djgpp = [b"MZ".as_slice(), &[0; 0x200], b"go32stub, v 2.05T"].concat();
        let compose = |files: &[(&str, &[u8])]| {
            let _ = fs::remove_dir_all(&modf);
            write_folder(&modf, files);
            let layer = ModLayer {
                files: modf.clone(),
                recipe: Recipe::Episode,
            };
            let dir = ensure(&paths, &db, "p", &base, Some(&layer), keen1(), false).unwrap();
            fs::read(dir.join("CWSDPMI.EXE")).ok()
        };
        assert_eq!(compose(&[("random.exe", &djgpp)]).as_deref(), Some(CWSDPMI));
        // The mod's own copy stays; deeper programs and others don't count.
        assert_eq!(
            compose(&[("random.exe", &djgpp), ("CWSDPMI.EXE", b"own")]).as_deref(),
            Some(b"own".as_slice())
        );
        assert_eq!(compose(&[("tools/random.exe", &djgpp)]), None);
        assert_eq!(compose(&[("MENU.EXE", b"MZ go32")]), None);
    }

    #[test]
    fn patch_files_without_a_patcher_get_ckpatch() {
        let tmp = tempfile::tempdir().unwrap();
        let (paths, db) = setup(tmp.path());
        let base = tmp.path().join("base");
        let modf = tmp.path().join("mod");
        write_folder(&base, &[("KEEN1.EXE", b"MZ")]);
        let pat: &[u8] = b"%ext ck1\n%patch $10 $01\n%end\n";
        let compose = |files: &[(&str, &[u8])]| {
            let _ = fs::remove_dir_all(&modf);
            write_folder(&modf, files);
            let layer = ModLayer {
                files: modf.clone(),
                recipe: Recipe::Program {
                    program: "ROBOKEEN.EXE".into(),
                    args: Vec::new(),
                },
            };
            let dir = ensure(&paths, &db, "p", &base, Some(&layer), keen1(), false).unwrap();
            fs::read(dir.join("CK1PATCH.EXE")).ok()
        };
        // As Robot Rebellion, whose ROBOKEEN.EXE runs `ck1patch ckrr.pat`.
        let bundled = ckpatch::bundled("keen1").unwrap().1;
        assert_eq!(
            compose(&[("ROBOKEEN.EXE", b"MZ"), ("ckrr.pat", pat)]).as_deref(),
            Some(bundled)
        );
        // The mod's own patcher stays, renamed ones count; no patch, no patcher.
        assert_eq!(
            compose(&[("ckrr.pat", pat), ("CK1PATCH.EXE", b"own")]).as_deref(),
            Some(b"own".as_slice())
        );
        let renamed = [b"MZ".as_slice(), &[0; 0x1E], b"Usage: CK1PATCH {patchfile}"].concat();
        assert_eq!(
            compose(&[("ckrr.pat", pat), ("NORPER.EXE", &renamed)]),
            None
        );
        assert_eq!(compose(&[("ROBOKEEN.EXE", b"MZ")]), None);
    }

    #[test]
    fn composes_mod_over_base_case_insensitively_and_reuses_result() {
        let tmp = tempfile::tempdir().unwrap();
        let (paths, db) = setup(tmp.path());
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

        let episode = ModLayer {
            files: modf.clone(),
            recipe: Recipe::Episode,
        };
        let dir = ensure(&paths, &db, "p", &base, Some(&episode), keen1(), false).unwrap();
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
        ensure(&paths, &db, "p", &base, Some(&episode), keen1(), false).unwrap();
        assert!(dir.join("MARK").exists());

        // A different recipe or changed file rebuilds it.
        let batch = ModLayer {
            files: modf.clone(),
            recipe: Recipe::Batch {
                file: "GO.BAT".into(),
            },
        };
        ensure(&paths, &db, "p", &base, Some(&batch), keen1(), false).unwrap();
        assert!(!dir.join("MARK").exists());
        fs::write(modf.join("NEW.CK1"), b"n").unwrap();
        ensure(&paths, &db, "p", &base, Some(&batch), keen1(), false).unwrap();
        assert!(dir.join("NEW.CK1").exists());
        assert!(!paths.cache.join("composed/p.tmp").exists());
    }
}

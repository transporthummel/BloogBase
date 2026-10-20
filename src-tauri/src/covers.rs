//! Covers: the title screen of a profile's game or mod, extracted from its own
//! files (PLAN.md §15) and cached as PNG. Nothing is shipped; a profile
//! whose title can't be read keeps the placeholder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use keen_graphics::Image;
use sha2::{Digest, Sha256};

use crate::catalog::{self, Engine, Game};
use crate::db::Db;
use crate::detect::hex;
use crate::error::{Error, Result};
use crate::paths::AppPaths;
use crate::profiles;
use crate::recipe::{Recipe, has_ext};

/// Bump to extract all covers again, e.g. after an extraction fix.
const VERSION: &str = "covers v2";

/// The profile's cover PNG, extracting it first if its files changed since
/// the last time. `None` if the title can't be read from them.
pub fn cover(paths: &AppPaths, db: &Db, profile_id: &str) -> Result<Option<PathBuf>> {
    let (profile, layers) = profiles::get(db, profile_id)?;
    let game = catalog::game(&profile.game_id)
        .ok_or_else(|| Error::NotFound(format!("game {}", profile.game_id)))?;
    let recipe = layers.mod_layer.as_ref().map(|m| &m.recipe);
    // A mod waiting for its game may have all it takes; once the game is
    // there, the changed files make for a new try.
    let files = Files::new(&layers.sources())?;

    let (png, stamp) = cache_files(paths, profile_id);
    let key = files.fingerprint(recipe);
    let wanted = |state: &str| format!("{key} {state}");
    match fs::read_to_string(&stamp) {
        Ok(s) if s == wanted("png") && png.is_file() => return Ok(Some(png)),
        Ok(s) if s == wanted("none") => return Ok(None),
        _ => {}
    }

    let title = extract(game, profile.version_id.as_deref(), recipe, &files);
    fs::create_dir_all(stamp.parent().expect("covers have a folder"))?;
    let state = match title {
        Ok(image) => {
            write_png(&image, &png)?;
            tracing::info!("extracted the title screen of {profile_id}");
            "png"
        }
        Err(e) => {
            tracing::info!("no title screen for {profile_id}: {e}");
            let _ = fs::remove_file(&png);
            "none"
        }
    };
    fs::write(&stamp, wanted(state))?;
    Ok((state == "png").then_some(png))
}

/// Forgets the cover of a removed profile.
pub fn remove(paths: &AppPaths, profile_id: &str) {
    for file in <[PathBuf; 2]>::from(cache_files(paths, profile_id)) {
        if let Err(e) = fs::remove_file(&file)
            && e.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!("could not remove {}: {e}", file.display());
        }
    }
}

/// The folder the frontend may show covers from.
pub fn dir(paths: &AppPaths) -> PathBuf {
    paths.cache.join("covers")
}

/// The PNG and the stamp saying which files it was made from.
fn cache_files(paths: &AppPaths, profile_id: &str) -> (PathBuf, PathBuf) {
    let dir = dir(paths);
    (
        dir.join(format!("{profile_id}.png")),
        dir.join(format!("{profile_id}.stamp")),
    )
}

/// The files DOS sees in the drive root: the mod's over the base game's.
struct Files {
    /// Bare name, path, size and modification time; earlier folders win.
    entries: Vec<(String, PathBuf, u64, u128)>,
}

impl Files {
    fn new(dirs: &[&Path]) -> Result<Self> {
        let mut entries: Vec<(String, PathBuf, u64, u128)> = Vec::new();
        for dir in dirs {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let meta = entry.metadata()?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if !meta.is_file() || entries.iter().any(|e| e.0.eq_ignore_ascii_case(&name)) {
                    continue;
                }
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                entries.push((name, entry.path(), meta.len(), modified));
            }
        }
        entries.sort();
        Ok(Files { entries })
    }

    /// Like the composed layer, files aren't changed in place, so names,
    /// sizes and times identify them.
    fn fingerprint(&self, recipe: Option<&Recipe>) -> String {
        let mut h = Sha256::new();
        h.update(VERSION);
        h.update(serde_json::to_string(&recipe).expect("recipes serialize"));
        for (name, path, size, modified) in &self.entries {
            h.update(format!("\n{name}\t{}\t{size}\t{modified}", path.display()));
        }
        hex(&h.finalize())
    }

    fn path(&self, name: &str) -> Option<&Path> {
        self.entries
            .iter()
            .find(|e| e.0.eq_ignore_ascii_case(name))
            .map(|e| e.1.as_path())
    }

    fn read(&self, name: &str) -> Option<Vec<u8>> {
        fs::read(self.path(name)?).ok()
    }

    fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.0.as_str())
    }
}

/// What a mod's patch file says about the graphics files.
#[derive(Debug, Default, PartialEq, Eq)]
struct PatchedNames {
    egahead: Option<String>,
    egadict: Option<String>,
    /// File names patched into the executable as text, e.g. a graphics
    /// file renamed to `KDREAMS2.CMP`.
    renamed: Vec<String>,
}

fn patched_names(text: &str) -> PatchedNames {
    let mut names = PatchedNames::default();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let Some(first) = words.next() else { continue };
        let first = first.to_ascii_lowercase();
        let argument = || {
            words
                .clone()
                .next()
                .map(|w| w.trim_matches('"').to_string())
        };
        match first.as_str() {
            "%egahead" => names.egahead = argument(),
            "%egadict" => names.egadict = argument(),
            "%patch" => {
                // Text comes in quotes; file names have a dot.
                names.renamed.extend(
                    line.split('"')
                        .skip(1)
                        .step_by(2)
                        .filter(|s| s.contains('.') && !s.contains(' '))
                        .map(str::to_string),
                );
            }
            "%end" => break,
            _ => {}
        }
    }
    names
}

/// `CK1`-`CK6`, `KDR`.
fn extension(game: &Game) -> String {
    match game.id.strip_prefix("keen") {
        Some(n) => format!("CK{n}"),
        None => "KDR".into(),
    }
}

fn extract(
    game: &Game,
    version_id: Option<&str>,
    recipe: Option<&Recipe>,
    files: &Files,
) -> std::result::Result<Image, String> {
    let ext = extension(game);
    let patch = recipe
        .and_then(Recipe::patch_file)
        .and_then(|p| files.read(p))
        .map(|t| patched_names(&String::from_utf8_lossy(&t)))
        .unwrap_or_default();
    let need = |name: &str| files.read(name).ok_or(format!("{name} missing"));
    // Renamed files come first; only existing ones of the right kind count.
    let candidates = |ext: &str, standard: String| -> Vec<String> {
        patch
            .renamed
            .iter()
            .filter(|n| has_ext(n, ext) && files.path(n).is_some())
            .cloned()
            .chain(std::iter::once(standard))
            .collect()
    };
    let mut last = String::from("no title screen");

    match game.engine {
        Engine::Vorticons => keen_graphics::vorticons_title(
            &need(&format!("EGAHEAD.{ext}"))?,
            &need(&format!("EGALATCH.{ext}"))?,
            &need(&format!("LEVEL90.{ext}"))?,
        )
        .map_err(|e| e.to_string()),
        Engine::Dreams => {
            for name in candidates("CMP", "KDREAMS.CMP".into()) {
                match files.read(&name).map(|c| keen_graphics::dreams_title(&c)) {
                    Some(Ok(image)) => return Ok(image),
                    Some(Err(e)) => last = format!("{name}: {e}"),
                    None => last = format!("{name} missing"),
                }
            }
            Err(last)
        }
        Engine::Galaxy => {
            // A mod's own program may be a changed game executable that
            // holds its own header and dictionary.
            let mut exes: Vec<String> = match recipe {
                Some(Recipe::Program { program, .. }) => vec![program.clone()],
                _ => Vec::new(),
            };
            let names: Vec<&str> = files.names().collect();
            exes.extend(
                version_id
                    .and_then(|id| game.versions.iter().find(|v| v.id == id))
                    .and_then(|v| v.start.clone())
                    .into_iter()
                    .chain(game.executables.iter().cloned())
                    .filter(|e| names.iter().any(|n| n.eq_ignore_ascii_case(e))),
            );
            let head = files.read(
                patch
                    .egahead
                    .as_deref()
                    .unwrap_or(&format!("EGAHEAD.{ext}")),
            );
            let dict = files.read(
                patch
                    .egadict
                    .as_deref()
                    .unwrap_or(&format!("EGADICT.{ext}")),
            );
            for graph_name in candidates(&ext, format!("EGAGRAPH.{ext}")) {
                let Some(graph) = files.read(&graph_name) else {
                    last = format!("{graph_name} missing");
                    continue;
                };
                for exe_name in &exes {
                    let Some(exe) = files.read(exe_name) else {
                        continue;
                    };
                    match keen_graphics::galaxy_title(
                        &exe,
                        &graph,
                        head.as_deref(),
                        dict.as_deref(),
                    ) {
                        Ok(image) => return Ok(image),
                        Err(e) => last = format!("{graph_name} with {exe_name}: {e}"),
                    }
                }
            }
            Err(last)
        }
    }
}

fn write_png(image: &Image, path: &Path) -> Result<()> {
    write_rgb_png(
        image.width as u32,
        image.height as u32,
        &image.to_rgb(),
        path,
    )
}

pub fn write_rgb_png(width: u32, height: u32, rgb: &[u8], path: &Path) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let file = io::BufWriter::new(fs::File::create(&tmp)?);
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(io::Error::other)?;
        writer
            .write_image_data(rgb)
            .and_then(|()| writer.finish())
            .map_err(io::Error::other)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library;
    use crate::source::testutil::*;

    #[test]
    fn reads_graphics_names_from_patch_files() {
        let names = patched_names(
            "%ext ck4\n%version 1.4\n%egahead EGAHEAD.CK4\n%EGADICT \"dict.ck4\"\n\
             %patch $23DFD \"KDREAMS2.CMP\"\n%patch $2F1A5 \"meminfo.txt\" 0\n\
             %patch $1000 \"Press a key\" $00\n%patchfile $1EE40 \"STORY.CK4\"\n\
             %end\n%patch $1 \"AFTER.END\"\n",
        );
        assert_eq!(
            names,
            PatchedNames {
                egahead: Some("EGAHEAD.CK4".into()),
                egadict: Some("dict.ck4".into()),
                renamed: vec!["KDREAMS2.CMP".into(), "meminfo.txt".into()],
            }
        );
    }

    #[test]
    fn caches_missing_titles_and_notices_changed_files() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join("data"));
        paths.ensure().unwrap();
        let db = Db::open_in_memory().unwrap();
        let src = tmp.path().join("k4");
        write_folder(&src, KEEN4);
        library::import(&paths, &db, &src).unwrap();

        // The test game's files are stand-ins, so there's no title.
        assert_eq!(cover(&paths, &db, "keen4-1").unwrap(), None);
        let (png, stamp) = cache_files(&paths, "keen4-1");
        let first = fs::read_to_string(&stamp).unwrap();
        assert!(first.ends_with(" none"));
        assert!(!png.exists());

        let base = paths.library.join("keen4-1").join("base");
        fs::write(base.join("EGAGRAPH.CK4"), b"changed").unwrap();
        assert_eq!(cover(&paths, &db, "keen4-1").unwrap(), None);
        assert_ne!(fs::read_to_string(&stamp).unwrap(), first);

        remove(&paths, "keen4-1");
        assert!(!stamp.exists());
        assert!(cover(&paths, &db, "nope").is_err());
    }

    #[test]
    fn extension_follows_the_game() {
        let ext = |id: &str| extension(catalog::game(id).unwrap());
        assert_eq!(
            [ext("keen1"), ext("keen6"), ext("kdreams")],
            ["CK1", "CK6", "KDR"]
        );
    }

    /// Extracts the titles of the mods in `$KEEN_MODS/<folder>` on top of
    /// the games in `$KEEN_GAMES` (as in `.local/games`) into `$KEEN_OUT`.
    ///
    /// `KEEN_MODS=… KEEN_GAMES=… KEEN_OUT=… cargo test covers::tests::mods -- --ignored`
    #[test]
    #[ignore]
    fn mods() {
        let dir = |var: &str| PathBuf::from(std::env::var(var).expect(var));
        let (games, out) = (dir("KEEN_GAMES"), dir("KEEN_OUT"));
        let mut failed = Vec::new();
        for entry in fs::read_dir(dir("KEEN_MODS")).unwrap() {
            let root = entry.unwrap().path();
            let info = crate::mods::inspect(&root, None).unwrap();
            let game = catalog::game(&info.game_id).unwrap();
            let recipe = &info.recipes[0].recipe;
            let base = games.join(match game.id.as_str() {
                "keen6" => "keen6-14",
                "kdreams" => "kdreams113",
                id => id,
            });
            let files = Files::new(&[&root, &base]).unwrap();
            let name = root.file_name().unwrap().to_string_lossy().into_owned();
            match extract(game, None, Some(recipe), &files) {
                Ok(image) => write_png(&image, &out.join(format!("{name}.png"))).unwrap(),
                Err(e) => failed.push(format!("{name}: {e}")),
            }
        }
        assert!(failed.is_empty(), "{failed:#?}");
    }
}

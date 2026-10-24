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
use crate::recipe::{self, Recipe, has_ext};

/// Bump to extract all covers again, e.g. after an extraction fix.
const VERSION: &str = "covers v12";

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
        Ok((image, colors)) => {
            write_png(&image, &colors, &png)?;
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

/// What a mod's patch file says about the graphics.
#[derive(Debug, Default, PartialEq, Eq)]
struct Patched {
    egahead: Option<String>,
    egadict: Option<String>,
    /// File names patched into the executable as text, e.g. a graphics
    /// file renamed to `KDREAMS2.CMP`.
    renamed: Vec<String>,
    /// The colors the game sets instead of the EGA ones.
    palette: Option<[[u8; 3]; 16]>,
    /// Files patched into the executable, e.g. a CGA mod's `C16HEAD.CK4`.
    patched_in: Vec<String>,
    /// The game draws for a CGA composite monitor.
    composite: bool,
    /// `%patch <address> "text"` lines: text written over the executable.
    texts: Vec<(usize, String)>,
}

/// How a title's pixels become colors.
#[derive(Debug, PartialEq, Eq)]
enum Colors {
    Palette([[u8; 3]; 16]),
    /// CGA pixels as a composite monitor shows them.
    Composite,
}

/// Where palette patches put their 16 VGA colors, each red, green and blue
/// from 0 to 63: in Keen 1, adurdin's patch over the Turbo C++ message at
/// DS:0004 and the fading palette patch, whose first palette is the bright
/// one; in Keen 4, K1n9_Duk3's patch over the old CheckCutFile code (as in
/// Revival of Bliss).
const PALETTES: [(&str, u32); 3] = [("CK1", 0x13054), ("CK1", 0x21850), ("CK4", 0x3D5E)];

fn patched(text: &str, ext: &str) -> Patched {
    let mut names = Patched::default();
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
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
            "%patchfile" => names
                .patched_in
                .extend(words.nth(1).map(|w| w.trim_matches('"').to_string())),
            "%patch" => {
                if names.palette.is_none() {
                    names.palette = words
                        .clone()
                        .next()
                        .and_then(|a| u32::from_str_radix(a.strip_prefix('$')?, 16).ok())
                        .filter(|&a| PALETTES.contains(&(ext, a)))
                        .and_then(|_| palette(&lines[i..]));
                }
                let address = words.clone().next().and_then(|a| {
                    let hex = a.strip_prefix('$').or_else(|| a.strip_prefix("0x"))?;
                    usize::from_str_radix(hex, 16).ok()
                });
                if let (Some(address), Some(text)) = (address, line.split('"').nth(1)) {
                    names.texts.push((address, text.to_string()));
                }
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
    names.composite = recipe::sets_composite_mode(text);
    names
}

/// The 16 colors at the start of the `%patch` on the first of `lines`, if
/// it starts with 48 plain numbers up to 63 (decimal unless `$` hex).
fn palette(lines: &[&str]) -> Option<[[u8; 3]; 16]> {
    let values = lines
        .iter()
        .enumerate()
        .take_while(|(i, l)| *i == 0 || !l.trim_start().starts_with('%'))
        .flat_map(|(i, l)| {
            let code = l.split('#').next().unwrap_or_default();
            // Skips `%patch` and the address.
            code.split_whitespace().skip(if i == 0 { 2 } else { 0 })
        })
        .take(48)
        .map(|w| match w.strip_prefix('$') {
            Some(h) => u8::from_str_radix(h, 16).ok(),
            None => w.parse::<u8>().ok(),
        })
        .collect::<Option<Vec<u8>>>()?;
    if values.len() < 48 || values.iter().any(|&v| v > 63) {
        return None;
    }
    let mut colors = [[0; 3]; 16];
    for (color, rgb) in colors.iter_mut().zip(values.chunks(3)) {
        for (c, &v) in color.iter_mut().zip(rgb) {
            *c = v << 2 | v >> 4;
        }
    }
    Some(colors)
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
) -> std::result::Result<(Image, Colors), String> {
    let ext = extension(game);
    let patch = recipe
        .and_then(Recipe::patch_file)
        .and_then(|p| files.read(p))
        .map(|t| patched(&String::from_utf8_lossy(&t), &ext))
        .unwrap_or_default();
    let (image, cga) = title(game, version_id, recipe, files, &ext, &patch)?;
    let colors = match patch.palette {
        _ if cga && patch.composite => Colors::Composite,
        Some(palette) => Colors::Palette(palette),
        None => Colors::Palette(keen_graphics::EGA_PALETTE),
    };
    Ok((image, colors))
}

/// The names a Vorticons game opens its files under, from a stem like
/// `EGAHEAD` or `LEVEL90`. A patch may write new names over the executable's
/// own (`EGAHEAD`, `LEVEL`, the extension), as Norp the Yorp 3 does to open
/// `TITLES1.NY3`; they count where a `%patch` starts right on one of those,
/// found in the unpacked executable.
fn vorticons_names(
    game: &Game,
    files: &Files,
    texts: &[(usize, String)],
    ext: &str,
) -> impl Fn(&str) -> String {
    let image = if texts.is_empty() {
        None
    } else {
        game.executables
            .iter()
            .find_map(|e| files.read(e))
            .and_then(|exe| keen_graphics::load_image(&exe).ok())
    };
    let patched = move |original: &str| -> String {
        let renamed = image.as_ref().and_then(|image| {
            let at: Vec<usize> = image
                .windows(original.len())
                .enumerate()
                .filter(|(_, w)| *w == original.as_bytes())
                .map(|(i, _)| i)
                .collect();
            texts
                .iter()
                .find(|(address, _)| at.contains(address))
                .map(|(_, text)| text.trim_end_matches('\0').to_string())
        });
        renamed.unwrap_or_else(|| original.to_string())
    };
    let ext = patched(ext);
    move |stem: &str| {
        let digits = stem.trim_start_matches(|c: char| !c.is_ascii_digit());
        let base = &stem[..stem.len() - digits.len()];
        format!("{}{digits}.{ext}", patched(base))
    }
}

fn title(
    game: &Game,
    version_id: Option<&str>,
    recipe: Option<&Recipe>,
    files: &Files,
    ext: &str,
    patch: &Patched,
) -> std::result::Result<(Image, bool), String> {
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
        Engine::Vorticons => {
            // Mods with the tileset loader patch keep the title level's
            // tiles in a file of their own. Some ship a TILEHEAD naming files
            // they lack, like Fireblaze; their game draws from EGALATCH.
            let name = vorticons_names(game, files, &patch.texts, ext);
            let tileset = files
                .read(&name("TILEHEAD"))
                .and_then(|t| keen_graphics::vorticons_tileset_name(&t, 90))
                .and_then(|name| files.read(&name));
            keen_graphics::vorticons_title(
                &need(&name("EGAHEAD"))?,
                &need(&name("EGALATCH"))?,
                &need(&name("LEVEL90"))?,
                tileset.as_deref(),
            )
            .map(|image| (image, false))
            .map_err(|e| e.to_string())
        }
        Engine::Dreams => {
            for name in candidates("CMP", "KDREAMS.CMP".into()) {
                match files.read(&name).map(|c| keen_graphics::dreams_title(&c)) {
                    Some(Ok(image)) => return Ok((image, false)),
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
            for graph_name in candidates(ext, format!("EGAGRAPH.{ext}")) {
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
                        Ok(image) => return Ok((image, false)),
                        Err(e) => last = format!("{graph_name} with {exe_name}: {e}"),
                    }
                }
            }
            // The CGA releases. Mods may rename CGAGRAPH (as C16GRAPH in
            // Keen 4 Composite CGA Edition) and patch their own header in.
            let cga = format!("CGAGRAPH.{ext}");
            let mut cga_names: Vec<String> = files
                .names()
                .filter(|n| {
                    let upper = n.to_ascii_uppercase();
                    upper.ends_with(&format!("GRAPH.{ext}"))
                        && upper != format!("EGAGRAPH.{ext}")
                        && upper != cga
                })
                .map(str::to_string)
                .collect();
            cga_names.push(cga);
            let heads: Vec<Vec<u8>> = patch
                .patched_in
                .iter()
                .filter(|n| n.to_ascii_uppercase().ends_with(&format!("HEAD.{ext}")))
                .filter_map(|n| files.read(n))
                .collect();
            for cga_name in cga_names {
                let Some(graph) = files.read(&cga_name) else {
                    continue;
                };
                for exe_name in &exes {
                    let Some(exe) = files.read(exe_name) else {
                        continue;
                    };
                    let heads = heads.iter().map(|h| Some(h.as_slice())).chain([None]);
                    for head in heads {
                        match keen_graphics::galaxy_cga_title(&exe, &graph, head) {
                            Ok(image) => return Ok((image, true)),
                            Err(e) => last = format!("{cga_name} with {exe_name}: {e}"),
                        }
                    }
                }
            }
            Err(last)
        }
    }
}

fn write_png(image: &Image, colors: &Colors, path: &Path) -> Result<()> {
    let rgb = match colors {
        Colors::Palette(palette) => image.to_rgb_with(palette),
        Colors::Composite => image.to_composite_rgb(),
    };
    write_rgb_png(image.width as u32, image.height as u32, &rgb, path)
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
        let names = patched(
            "%ext ck4\n%version 1.4\n%egahead EGAHEAD.CK4\n%EGADICT \"dict.ck4\"\n\
             %patch $23DFD \"KDREAMS2.CMP\"\n%patch $2F1A5 \"meminfo.txt\" 0\n\
             %patch $1000 \"Press a key\" $00\n%patchfile $1EE40 \"STORY.CK4\"\n\
             %end\n%patch $1 \"AFTER.END\"\n",
            "CK4",
        );
        assert_eq!(
            names,
            Patched {
                egahead: Some("EGAHEAD.CK4".into()),
                egadict: Some("dict.ck4".into()),
                renamed: vec!["KDREAMS2.CMP".into(), "meminfo.txt".into()],
                palette: None,
                patched_in: vec!["STORY.CK4".into()],
                composite: false,
                texts: vec![
                    (0x23DFD, "KDREAMS2.CMP".into()),
                    (0x2F1A5, "meminfo.txt".into()),
                    (0x1000, "Press a key".into()),
                ],
            }
        );
    }

    #[test]
    fn reads_palettes_from_patch_files() {
        // As in Sunset: decimal values over several lines, with comments.
        let mut text = String::from("%ext ck1\n#Change the palette\n%patch $13054 \n");
        for i in 0..16 {
            text += &format!("       {:02} 00 $3F  #color {i}\n", i * 4);
        }
        text += "%patch $6E46  $E8 $D3 $D5 $90 $90\n";
        let palette = patched(&text, "CK1").palette.unwrap();
        assert_eq!(palette[0], [0, 0, 255]);
        assert_eq!(palette[1], [16, 0, 255]);
        assert_eq!(palette[15], [243, 0, 255]);

        // Only Keen 1 has them there; too few colors don't count.
        assert_eq!(patched(&text, "CK2").palette, None);
        let short = "%patch $21850 00 00 00 63 63 63\n%patch $1 $00\n";
        assert_eq!(patched(short, "CK1").palette, None);
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
    fn reads_hex_palettes_at_the_fading_patch() {
        let colors: Vec<String> = (0..48).map(|i| format!("${:02X}", i % 64)).collect();
        let text = format!("%patch $21850 {}\n%end\n", colors.join(" "));
        let palette = patched(&text, "CK1").palette.unwrap();
        assert_eq!(palette[0], [0, 4, 8]);
        assert_eq!(palette[15], [182, 186, 190]);

        // Elsewhere, or with a value over 63, it's no palette.
        assert_eq!(
            patched(&text.replace("$21850", "$21852"), "CK1").palette,
            None
        );
        assert_eq!(patched(&text.replace("$2F", "$40"), "CK1").palette, None);
    }

    #[test]
    fn reads_keen4_palettes() {
        // As in Revival of Bliss: the address with a comment, then the colors.
        let mut text = String::from(
            "%ext ck4
%patch $3D5E	# K1n9_Duk3 mod
",
        );
        for _ in 0..16 {
            text.push_str(
                "       00 00 42  #Blue
",
            );
        }
        text.push_str(
            "
#Set the palette
%patch $3CE5  $90 $90
",
        );
        assert_eq!(patched(&text, "CK4").palette, Some([[0, 0, 170]; 16]));
        assert_eq!(patched(&text, "CK5").palette, None);
    }

    /// The files of a Keen 1 title: one 16x16 tile (blue), an 8x1 title
    /// bitmap (dark grey) and a 22x15 level of that tile, as in
    /// `keen-graphics/tests/formats.rs`.
    fn keen1_title() -> Vec<(&'static str, Vec<u8>)> {
        let mut head = vec![0u8; 48];
        head[0..4].copy_from_slice(&33u32.to_le_bytes()); // plane size
        head[8..12].copy_from_slice(&48u32.to_le_bytes()); // bitmap table
        head[28..30].copy_from_slice(&1u16.to_le_bytes()); // 16x16 tiles
        head[34..36].copy_from_slice(&1u16.to_le_bytes()); // bitmaps
        head[36..40].copy_from_slice(&32u32.to_le_bytes());
        head.extend([1, 0, 1, 0, 0, 0, 0, 0]); // 1 byte wide, 1 row, at 0
        head.extend(b"TITLE\0\0\0");

        let mut latch = Vec::new();
        for plane in 0..4 {
            latch.extend([if plane == 0 { 0xFF } else { 0 }; 32]);
            latch.push(if plane == 3 { 0xFF } else { 0 });
        }

        let (w, h) = (22u16, 15u16);
        let mut level = ((16 + 2 * u32::from(w * h)) * 2).to_le_bytes().to_vec();
        for word in [w, h, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
            level.extend(word.to_le_bytes());
        }
        for word in [0xFEFE, w * h, 0, 0xFEFE, w * h, 0] {
            level.extend(word.to_le_bytes());
        }
        vec![
            ("EGAHEAD.CK1", head),
            ("EGALATCH.CK1", latch),
            ("LEVEL90.CK1", level),
        ]
    }

    /// A tileset loader file with one red tile, and the TILEHEAD naming it
    /// for the title level.
    fn tileset(name: &str) -> [(&'static str, Vec<u8>); 2] {
        let mut tls = vec![0u8; 64];
        tls[0..2].copy_from_slice(&1u16.to_le_bytes()); // tiles
        tls[4..6].copy_from_slice(&32u16.to_le_bytes()); // plane length
        tls[14..18].copy_from_slice(&64u32.to_le_bytes());
        for plane in 0..4 {
            tls.extend([if plane == 2 { 0xFF } else { 0 }; 32]);
        }
        let mut tilehead = vec![0u8; 91 * 16];
        tilehead[90 * 16..][..name.len()].copy_from_slice(name.as_bytes());
        [("TILEHEAD.CK1", tilehead), ("TITLE.TLS", tls)]
    }

    /// Extracts the title of `game` from `files` with a patch recipe.
    fn extract_from(
        game: &str,
        files: &[(&str, Vec<u8>)],
    ) -> std::result::Result<(Image, Colors), String> {
        let tmp = tempfile::tempdir().unwrap();
        for (name, data) in files {
            fs::write(tmp.path().join(name), data).unwrap();
        }
        let recipe = Recipe::Patch {
            patcher: "CKPATCH.EXE".into(),
            patch: "MOD.PAT".into(),
            args: Vec::new(),
        };
        let files = Files::new(&[tmp.path()]).unwrap();
        extract(catalog::game(game).unwrap(), None, Some(&recipe), &files)
    }

    #[test]
    fn finds_vorticons_files_renamed_in_the_executable() {
        // As Norp the Yorp 3: new names written over the executable's.
        let strings = b"EGAHEAD\0EGALATCH\0LEVEL\0CK1\0";
        // A header of two paragraphs; the load image starts after it.
        let exe = [b"MZ".as_slice(), &[0; 6], &[2, 0], &[0; 0x16], strings].concat();
        let patch = "%patch 0x0 \"TITLES1\"\n%patch 0x8 \"LANDSCAP\"\n\
                     %patch 0x11 \"N3MAP\"\n%patch 0x17 \"NY1\"\n";
        let mut files: Vec<(&str, Vec<u8>)> = keen1_title()
            .into_iter()
            .zip(["TITLES1.NY1", "LANDSCAP.NY1", "N3MAP90.NY1"])
            .map(|((_, data), name)| (name, data))
            .collect();
        files.push(("KEEN1.EXE", exe));
        files.push(("MOD.PAT", patch.as_bytes().to_vec()));
        let pixels = extract_from("keen1", &files).unwrap().0.pixels;
        assert_eq!((pixels[0], pixels[320 + 32]), (1, 8));
    }

    #[test]
    fn draws_keen1_levels_from_the_named_tileset_or_egalatch() {
        let pixel = |files: &[(&str, Vec<u8>)]| extract_from("keen1", files).unwrap().0.pixels;

        let plain = keen1_title();
        let pixels = pixel(&plain);
        assert_eq!((pixels[0], pixels[320 + 32]), (1, 8));

        // DOS names are case-insensitive: title.tls is TITLE.TLS.
        let with_tileset = [plain.clone(), tileset("title.tls LS\0").into()].concat();
        let pixels = pixel(&with_tileset);
        assert_eq!((pixels[0], pixels[320 + 32]), (4, 8));

        // A TILEHEAD naming a file the mod lacks, like Fireblaze's.
        let [tilehead, _] = tileset("MISSING.TLS");
        assert_eq!(pixel(&[plain.clone(), vec![tilehead]].concat())[0], 1);

        let without_level: Vec<_> = plain.into_iter().filter(|f| f.0 != "LEVEL90.CK1").collect();
        assert_eq!(
            extract_from("keen1", &without_level).unwrap_err(),
            "LEVEL90.CK1 missing"
        );
    }

    #[test]
    fn draws_covers_in_the_palette_the_patch_sets() {
        let (image, colors) = extract_from("keen1", &keen1_title()).unwrap();
        assert_eq!(colors, Colors::Palette(keen_graphics::EGA_PALETTE));

        let mut text = String::from("%ext ck1\n%patch $13054\n");
        for i in 0..16 {
            text += &format!("{} 0 0\n", i * 4);
        }
        let files = [keen1_title(), vec![("MOD.PAT", text.into_bytes())]].concat();
        let (patched_image, colors) = extract_from("keen1", &files).unwrap();
        assert_eq!(patched_image, image);
        let Colors::Palette(palette) = colors else {
            panic!("{colors:?}")
        };
        assert_eq!(palette[1], [16, 0, 0]);
        assert_eq!(palette[8], [130, 0, 0]);

        // The PNG has the patch's colors: the blue tile, the grey title.
        let tmp = tempfile::tempdir().unwrap();
        let png_path = tmp.path().join("cover.png");
        write_png(&image, &Colors::Palette(palette), &png_path).unwrap();
        let decoder = png::Decoder::new(io::BufReader::new(fs::File::open(&png_path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut rgb = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut rgb).unwrap();
        assert_eq!(&rgb[..3], &[16, 0, 0]);
        let title = (320 + 32) * 3;
        assert_eq!(&rgb[title..title + 3], &[130, 0, 0]);
    }

    /// A Softdisk library holding a 16x1 one-plane `TITLESCR.LBM` whose
    /// first `left` pixels are set.
    fn dreams_cmp(left: u8) -> Vec<u8> {
        let mut bmhd = [0u8; 20];
        bmhd[0..2].copy_from_slice(&16u16.to_be_bytes());
        bmhd[2..4].copy_from_slice(&1u16.to_be_bytes());
        bmhd[8] = 1; // one plane
        let row = (!(u16::MAX >> left)).to_be_bytes();
        let mut form = b"ILBM".to_vec();
        for (id, data) in [(b"BMHD", &bmhd[..]), (b"BODY", &row[..])] {
            form.extend(id);
            form.extend((data.len() as u32).to_be_bytes());
            form.extend(data);
        }
        let mut lbm = b"FORM".to_vec();
        lbm.extend((form.len() as u32).to_be_bytes());
        lbm.extend(form);

        let mut lib = b"SLIB\x02\x00\x01\x00".to_vec();
        lib.extend(b"TITLESCR.LBM\0\0\0\0");
        lib.extend(0u32.to_le_bytes());
        lib.extend(((lbm.len() + 10) as u32).to_le_bytes());
        lib.extend((lbm.len() as u32).to_le_bytes());
        lib.extend(0u16.to_le_bytes());
        lib.extend(b"CUNK");
        lib.extend((lbm.len() as u32).to_le_bytes());
        lib.extend(0u16.to_le_bytes());
        lib.extend(lbm);
        lib
    }

    #[test]
    fn prefers_a_renamed_keen_dreams_library_that_works() {
        let lit = |files: &[(&str, Vec<u8>)]| {
            let image = extract_from("kdreams", files).unwrap().0;
            image.pixels.iter().filter(|&&p| p != 0).count()
        };
        let patch = (
            "MOD.PAT",
            b"%ext kdr\n%patch $23DFD \"KDREAMS2.CMP\"\n".to_vec(),
        );
        let original = ("KDREAMS.CMP", dreams_cmp(1));

        assert_eq!(lit(std::slice::from_ref(&original)), 1);
        let renamed = ("KDREAMS2.CMP", dreams_cmp(3));
        assert_eq!(lit(&[patch.clone(), original.clone(), renamed]), 3);
        // A renamed file that's missing or no library leaves the original.
        assert_eq!(lit(&[patch.clone(), original.clone()]), 1);
        let broken = ("KDREAMS2.CMP", b"nonsense".to_vec());
        assert_eq!(lit(&[patch.clone(), original, broken.clone()]), 1);
        assert!(extract_from("kdreams", &[patch, broken]).is_err());
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
                Ok((image, colors)) => {
                    write_png(&image, &colors, &out.join(format!("{name}.png"))).unwrap()
                }
                Err(e) => failed.push(format!("{name}: {e}")),
            }
        }
        assert!(failed.is_empty(), "{failed:#?}");
    }
}

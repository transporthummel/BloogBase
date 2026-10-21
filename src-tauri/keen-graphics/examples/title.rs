//! Writes a game's or mod's title screen as PNG, to compare it with the real
//! thing. Files are looked up in the mod folder first, then the game folder.
//!
//! `cargo run -p keen-graphics --example title -- <out.png> <folder>...`

use std::fs;
use std::path::{Path, PathBuf};

fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((out, dirs)) = args.split_first() else {
        eprintln!("usage: title <out.png> <folder>...");
        std::process::exit(2);
    };
    let read = |name: &str| {
        dirs.iter()
            .find_map(|d| find(Path::new(d), name))
            .and_then(|p| fs::read(p).ok())
    };
    let vorticons = ["1", "2", "3"]
        .into_iter()
        .find(|n| read(&format!("EGALATCH.CK{n}")).is_some());
    let image = if let Some(n) = vorticons {
        keen_graphics::vorticons_title(
            &read(&format!("EGAHEAD.CK{n}")).expect("EGAHEAD"),
            &read(&format!("EGALATCH.CK{n}")).unwrap(),
            &read(&format!("LEVEL90.CK{n}")).expect("LEVEL90"),
            read(&format!("TILEHEAD.CK{n}"))
                .and_then(|t| keen_graphics::vorticons_tileset_name(&t, 90))
                .map(|name| read(&name).expect("tileset"))
                .as_deref(),
        )
    } else if let Some(cmp) = read("KDREAMS.CMP") {
        keen_graphics::dreams_title(&cmp)
    } else {
        let n = ["4", "5", "6"]
            .into_iter()
            .find(|n| read(&format!("EGAGRAPH.CK{n}")).is_some())
            .expect("no Keen 4-6 or Keen Dreams 1.13 files found");
        let exe = [format!("KEEN{n}E.EXE"), format!("KEEN{n}.EXE")]
            .iter()
            .find_map(|e| read(e))
            .expect("no executable");
        keen_graphics::galaxy_title(
            &exe,
            &read(&format!("EGAGRAPH.CK{n}")).unwrap(),
            read(&format!("EGAHEAD.CK{n}")).as_deref(),
            read(&format!("EGADICT.CK{n}")).as_deref(),
        )
    }
    .unwrap_or_else(|e| {
        eprintln!("failed: {e}");
        std::process::exit(1);
    });

    let file = fs::File::create(out).unwrap();
    let mut encoder = png::Encoder::new(file, image.width as u32, image.height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&image.to_rgb())
        .unwrap();
    println!("{}x{} -> {out}", image.width, image.height);
}

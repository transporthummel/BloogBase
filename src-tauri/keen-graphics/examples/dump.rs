//! Dumps every full-screen picture of a Keen 4-6 game as PNG.
//!
//! `cargo run -p keen-graphics --example dump -- <exe> <egagraph> <out dir> [egahead] [egadict]`

use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exe = keen_graphics::lzexe::load_image(&fs::read(&args[0]).unwrap()).unwrap();
    let data = fs::read(&args[1]).unwrap();
    let head = args.get(3).map(|p| fs::read(p).unwrap());
    let dict = args.get(4).map(|p| fs::read(p).unwrap());
    let graph = match &head {
        Some(h) => {
            keen_graphics::egagraph::EgaGraph::from_files(&data, h, 3, dict.as_deref(), &exe)
        }
        None => keen_graphics::egagraph::EgaGraph::from_exe(&data, &exe, 3),
    }
    .unwrap();
    println!("{} pictures", graph.pictures().unwrap().len());
    for i in graph.full_screen().unwrap() {
        let image = graph.picture(i).unwrap();
        let out = format!("{}/pic{i:03}.png", args[2]);
        let file = fs::File::create(&out).unwrap();
        let mut encoder = png::Encoder::new(file, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&image.to_rgb())
            .unwrap();
        let mut used = [false; 16];
        image
            .pixels
            .iter()
            .for_each(|&c| used[usize::from(c)] = true);
        println!("{out} colors={}", used.iter().filter(|&&u| u).count());
    }
}

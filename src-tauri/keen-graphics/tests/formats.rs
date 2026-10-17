//! Format tests with hand-made data. Real game files can't be checked in;
//! `corpus` runs against a local folder of them.

use keen_graphics::huffman::Dictionary;
use keen_graphics::{Image, egagraph::EgaGraph, ilbm, slib};

/// Root: 0 -> 'A', 1 -> node 253; node 253: 0 -> 'B', 1 -> 'C'.
fn dictionary() -> Vec<u8> {
    let mut d = vec![0u8; 1024];
    let mut set = |node: usize, branch: usize, value: u16| {
        d[node * 4 + branch * 2..][..2].copy_from_slice(&value.to_le_bytes());
    };
    set(254, 0, u16::from(b'A'));
    set(254, 1, 256 + 253);
    set(253, 0, u16::from(b'B'));
    set(253, 1, u16::from(b'C'));
    d
}

#[test]
fn huffman_reads_bits_least_significant_first() {
    let dict = Dictionary::parse(&dictionary()).unwrap();
    // A=0, B=10, C=11 (in reading order): 0 1 0 1 1 0 -> 0b011010
    assert_eq!(dict.decode(&[0b0001_1010], 4).unwrap(), b"ABCA");
    assert!(dict.decode(&[0b0001_1010], 9).is_err());
}

#[test]
fn huffman_rejects_branches_outside_the_tree() {
    let mut d = dictionary();
    d[0..2].copy_from_slice(&600u16.to_le_bytes());
    assert!(Dictionary::parse(&d).is_err());
}

#[test]
fn egagraph_finds_header_and_dictionary_in_the_executable() {
    // Chunk 0, the picture table: one 8x1 picture, stored raw.
    let table = [1u8, 0, 1, 0];
    let mut graph = (table.len() as u32).to_le_bytes().to_vec();
    graph.extend_from_slice(&table);
    // Chunks 1-2 empty, chunk 3: the picture itself (4 planes of 1 byte).
    let pic_at = graph.len();
    graph.extend_from_slice(&4u32.to_le_bytes());
    graph.extend_from_slice(&[0xFF, 0x00, 0x00, 0x00]);
    let len = graph.len();

    let mut exe = vec![0x55u8; 40];
    for o in [0usize, 0xFFFFFF, 0xFFFFFF, pic_at, len] {
        exe.extend_from_slice(&(o as u32).to_le_bytes()[..3]);
    }
    exe.extend(vec![0x55u8; 10]);
    exe.extend(dictionary());

    let g = EgaGraph::from_exe(&graph, &exe, 3).unwrap();
    assert_eq!(g.pictures().unwrap(), vec![(1, 1)]);
    let pic = g.picture(0).unwrap();
    assert_eq!((pic.width, pic.height), (8, 1));
    assert_eq!(pic.pixels, vec![1; 8]); // plane 0 = blue
}

#[test]
fn planar_pictures_combine_four_planes() {
    // Pixel 0 in all planes (white), pixel 1 only in red and intensity.
    let data = [0b1000_0000, 0b1000_0000, 0b1100_0000, 0b1100_0000];
    let image = Image::from_planar(&data, 8, 1).unwrap();
    assert_eq!(&image.pixels[..3], &[15, 12, 0]);
    assert_eq!(&image.to_rgb()[3..6], &[0xFF, 0x55, 0x55]);
    assert_eq!(image.color_count(), 3);
}

fn lbm(compressed: bool) -> Vec<u8> {
    let mut bmhd = vec![0u8; 20];
    bmhd[0..2].copy_from_slice(&16u16.to_be_bytes());
    bmhd[2..4].copy_from_slice(&1u16.to_be_bytes());
    bmhd[8] = 1; // one plane
    bmhd[10] = u8::from(compressed);
    let body: Vec<u8> = if compressed {
        vec![0x01, 0xF0, 0x0F] // two literal bytes
    } else {
        vec![0xF0, 0x0F]
    };
    let mut form = b"ILBM".to_vec();
    for (id, data) in [(b"BMHD", &bmhd), (b"BODY", &body)] {
        form.extend_from_slice(id);
        form.extend_from_slice(&(data.len() as u32).to_be_bytes());
        form.extend_from_slice(data);
        if data.len() % 2 == 1 {
            form.push(0);
        }
    }
    let mut file = b"FORM".to_vec();
    file.extend_from_slice(&(form.len() as u32).to_be_bytes());
    file.extend(form);
    file
}

#[test]
fn ilbm_reads_raw_and_packed_bodies() {
    for compressed in [false, true] {
        let image = ilbm::decode(&lbm(compressed)).unwrap();
        assert_eq!((image.width, image.height), (16, 1));
        let expected: Vec<u8> = [[1u8; 4], [0; 4], [0; 4], [1; 4]].concat();
        assert_eq!(image.pixels, expected);
    }
}

fn library(entry: &[u8], stated_size: usize) -> Vec<u8> {
    let mut lib = b"SLIB\x02\x00\x01\x00".to_vec();
    let mut name = b"TITLESCR.LBM".to_vec();
    name.resize(16, 0);
    lib.extend(name);
    lib.extend_from_slice(&0u32.to_le_bytes());
    lib.extend_from_slice(&((entry.len() + 10) as u32).to_le_bytes());
    lib.extend_from_slice(&(stated_size as u32).to_le_bytes());
    lib.extend_from_slice(&0u16.to_le_bytes());
    lib.extend_from_slice(b"CUNK");
    lib.extend_from_slice(&(stated_size as u32).to_le_bytes());
    lib.extend_from_slice(&0u16.to_le_bytes());
    lib.extend_from_slice(entry);
    lib
}

#[test]
fn slib_finds_stored_entries_also_with_sizes_too_large() {
    let picture = lbm(true);
    for stated in [picture.len(), picture.len() + 8] {
        let lib = library(&picture, stated);
        assert_eq!(slib::entry(&lib, "titlescr.lbm").unwrap(), picture);
        assert!(keen_graphics::dreams_title(&lib).is_ok());
    }
    assert!(slib::entry(&library(&picture, picture.len()), "OTHER").is_err());
    assert!(slib::entry(b"nonsense", "TITLESCR.LBM").is_err());
}

/// Checks the real games in `$KEEN_GAMES/<folder>` (game folders as in
/// `.local/games`): every title is a full 320x200 picture with 16 colors.
///
/// `KEEN_GAMES=<absolute path> cargo test -p keen-graphics corpus -- --ignored`
#[test]
#[ignore]
fn corpus() {
    let root = std::path::PathBuf::from(std::env::var("KEEN_GAMES").expect("KEEN_GAMES"));
    let read = |dir: &str, name: &str| std::fs::read(root.join(dir).join(name)).unwrap();
    let titles = [
        (
            "keen4",
            keen_graphics::galaxy_title(
                &read("keen4", "KEEN4E.EXE"),
                &read("keen4", "EGAGRAPH.CK4"),
                None,
                None,
            ),
        ),
        (
            "keen5",
            keen_graphics::galaxy_title(
                &read("keen5", "KEEN5E.EXE"),
                &read("keen5", "EGAGRAPH.CK5"),
                None,
                None,
            ),
        ),
        (
            "keen6",
            keen_graphics::galaxy_title(
                &read("keen6", "KEEN6.EXE"),
                &read("keen6", "EGAGRAPH.CK6"),
                None,
                None,
            ),
        ),
        (
            "kdreams113",
            keen_graphics::dreams_title(&read("kdreams113", "KDREAMS.CMP")),
        ),
    ];
    for (game, title) in titles {
        let image = title.unwrap_or_else(|e| panic!("{game}: {e}"));
        assert_eq!((image.width, image.height), (320, 200), "{game}");
        assert_eq!(image.color_count(), 16, "{game}");
    }
}

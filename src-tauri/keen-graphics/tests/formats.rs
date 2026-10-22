//! Format tests with hand-made data. Real game files can't be checked in;
//! `corpus` runs against a local folder of them.

use keen_graphics::huffman::Dictionary;
use keen_graphics::{Image, egagraph::EgaGraph, ilbm, lzexe, pklite, slib, vorticons};

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
fn egagraph_reads_chunks_as_stored_without_a_dictionary() {
    // As in Skater Squirrel: no Huffman compression, so the executable has
    // no dictionary that fits, and chunk 2 points into chunk 0.
    let table = [1u8, 0, 1, 0, 1, 0, 1, 0];
    let mut graph = (table.len() as u32).to_le_bytes().to_vec();
    graph.extend_from_slice(&table);
    let pic_at = graph.len();
    graph.extend_from_slice(&4u32.to_le_bytes());
    graph.extend_from_slice(&[0xFF, 0x00, 0x00, 0x00]);
    let head: Vec<u8> = [0usize, 0xFFFFFF, 6, pic_at, pic_at]
        .iter()
        .flat_map(|o| (*o as u32).to_le_bytes()[..3].to_vec())
        .collect();

    let g = EgaGraph::from_files(&graph, &head, 3, None, &[0x55; 2048]).unwrap();
    assert_eq!(g.pictures().unwrap(), vec![(1, 1), (1, 1)]);
    assert_eq!(g.picture(0).unwrap().pixels, vec![1; 8]);
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

/// Packs codes of `width` bits, most significant bit first.
fn pack(codes: &[(u16, u32)]) -> Vec<u8> {
    let mut bits: Vec<bool> = Vec::new();
    for &(code, width) in codes {
        bits.extend((0..width).rev().map(|i| code >> i & 1 == 1));
    }
    bits.chunks(8)
        .map(|c| {
            c.iter()
                .enumerate()
                .fold(0, |b, (i, &on)| b | u8::from(on) << (7 - i))
        })
        .collect()
}

#[test]
fn lzw_builds_its_dictionary_from_the_codes() {
    // A, B, AB, then the entry being defined (ABA), then the end.
    let mut data = 7u32.to_le_bytes().to_vec();
    data.extend_from_slice(&12u16.to_le_bytes());
    data.extend(pack(&[
        (65, 9),
        (66, 9),
        (0x102, 9),
        (0x104, 9),
        (0x101, 9),
    ]));
    assert_eq!(vorticons::lzw(&data).unwrap(), b"ABABABA");
    // A code that isn't defined yet.
    let mut bad = 7u32.to_le_bytes().to_vec();
    bad.extend_from_slice(&12u16.to_le_bytes());
    bad.extend(pack(&[(65, 9), (0x1F0, 9)]));
    assert!(vorticons::lzw(&bad).is_err());
}

#[test]
fn vorticons_title_draws_the_bitmap_over_the_level() {
    // One 16x16 tile at 0 and an 8x1 TITLE bitmap at 32, in four planes.
    let plane_size = 33u32;
    let mut head = vec![0u8; 48];
    head[0..4].copy_from_slice(&plane_size.to_le_bytes());
    head[8..12].copy_from_slice(&48u32.to_le_bytes()); // bitmap table
    head[28..30].copy_from_slice(&1u16.to_le_bytes()); // 16x16 tiles
    head[34..36].copy_from_slice(&1u16.to_le_bytes()); // bitmaps
    head[36..40].copy_from_slice(&32u32.to_le_bytes());
    head.extend_from_slice(&1u16.to_le_bytes()); // 1 byte wide
    head.extend_from_slice(&1u16.to_le_bytes()); // 1 row
    head.extend_from_slice(&0u32.to_le_bytes());
    head.extend_from_slice(b"TITLE\0\0\0");

    let mut latch = Vec::new();
    for plane in 0..4 {
        // The tile is blue, the bitmap dark grey.
        latch.extend(vec![if plane == 0 { 0xFF } else { 0 }; 32]);
        latch.push(if plane == 3 { 0xFF } else { 0 });
    }

    // A 22x15 level of tile 0: header, the tiles as one run, sprites.
    let (w, h) = (22u16, 15u16);
    let words: u32 = 16 + 2 * u32::from(w * h);
    let mut level = (words * 2).to_le_bytes().to_vec();
    for word in [w, h, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] {
        level.extend_from_slice(&word.to_le_bytes());
    }
    for word in [0xFEFE, w * h, 0, 0xFEFE, w * h, 0] {
        level.extend_from_slice(&word.to_le_bytes());
    }

    let image = keen_graphics::vorticons_title(&head, &latch, &level, None).unwrap();
    assert_eq!((image.width, image.height), (320, 200));
    assert_eq!(image.pixels[0], 1);
    assert_eq!(
        &image.pixels[320 + 31..320 + 41],
        &[1, 8, 8, 8, 8, 8, 8, 8, 8, 1]
    );
    assert_eq!(image.pixels[199 * 320 + 319], 1);

    assert!(keen_graphics::vorticons_title(&head, &latch[..100], &level, None).is_err());
    assert!(keen_graphics::vorticons_title(&head[..40], &latch, &level, None).is_err());

    // Keen 1 1.32 and later: 6 more bytes before the tile count, a 64-byte
    // header, the same picture.
    let mut long = head[..28].to_vec();
    long.extend([0; 6]);
    long.extend(&head[28..48]);
    long.resize(64, 0);
    long[8..12].copy_from_slice(&64u32.to_le_bytes());
    long.extend(&head[48..]);
    assert_eq!(
        keen_graphics::vorticons_title(&long, &latch, &level, None).unwrap(),
        image
    );

    // A tileset file of the loader patch: one red tile, its planes after a
    // 64-byte header.
    let mut tls = vec![0u8; 64];
    tls[0..2].copy_from_slice(&1u16.to_le_bytes()); // tiles
    tls[4..6].copy_from_slice(&32u16.to_le_bytes()); // plane length
    tls[14..18].copy_from_slice(&64u32.to_le_bytes());
    for plane in 0..4 {
        tls.extend([if plane == 2 { 0xFF } else { 0 }; 32]);
    }
    let image = keen_graphics::vorticons_title(&head, &latch, &level, Some(&tls)).unwrap();
    assert_eq!(image.pixels[0], 4);
    assert_eq!(image.pixels[320 + 32], 8);
    assert!(keen_graphics::vorticons_title(&head, &latch, &level, Some(&tls[..100])).is_err());

    let mut tilehead = vec![0u8; 91 * 16];
    tilehead[90 * 16..90 * 16 + 15].copy_from_slice(b"title.tls LS  \0");
    assert_eq!(
        keen_graphics::vorticons_tileset_name(&tilehead, 90).as_deref(),
        Some("title.tls")
    );
    assert_eq!(keen_graphics::vorticons_tileset_name(&tilehead, 0), None);
    assert_eq!(keen_graphics::vorticons_tileset_name(&tilehead, 91), None);
}

#[test]
fn vorticons_full_screen_titles_hide_the_level() {
    // A 320x200 TITLE bitmap after one tile; like Mura, the level's tiles
    // aren't in EGALATCH, here not even the level is readable.
    let plane_size = 32 + 40 * 200;
    let mut head = vec![0u8; 48];
    head[0..4].copy_from_slice(&(plane_size as u32).to_le_bytes());
    head[8..12].copy_from_slice(&48u32.to_le_bytes());
    head[28..30].copy_from_slice(&1u16.to_le_bytes());
    head[34..36].copy_from_slice(&1u16.to_le_bytes());
    head[36..40].copy_from_slice(&32u32.to_le_bytes());
    head.extend_from_slice(&40u16.to_le_bytes());
    head.extend_from_slice(&200u16.to_le_bytes());
    head.extend_from_slice(&0u32.to_le_bytes());
    head.extend_from_slice(b"TITLE\0\0\0");
    let mut latch = Vec::new();
    for plane in 0..4 {
        latch.extend([0u8; 32]);
        latch.extend(vec![if plane == 3 { 0xFF } else { 0 }; plane_size - 32]);
    }

    let image = keen_graphics::vorticons_title(&head, &latch, b"broken", Some(b"x")).unwrap();
    assert!(image.pixels.iter().all(|&p| p == 8));

    // One row less, and the level shows below it.
    head[50..52].copy_from_slice(&199u16.to_le_bytes());
    assert!(keen_graphics::vorticons_title(&head, &latch, b"broken", None).is_err());
}

/// Checks the real games in `$KEEN_GAMES/<folder>` (game folders as in
/// `.local/games`): every title is a full, colorful 320x200 picture.
///
/// `KEEN_GAMES=<absolute path> cargo test -p keen-graphics corpus -- --ignored`
#[test]
#[ignore]
fn corpus() {
    let root = std::path::PathBuf::from(std::env::var("KEEN_GAMES").expect("KEEN_GAMES"));
    let read = |dir: &str, name: &str| std::fs::read(root.join(dir).join(name)).unwrap();
    let titles = [
        (
            "keen1",
            keen_graphics::vorticons_title(
                &read("keen1", "EGAHEAD.CK1"),
                &read("keen1", "EGALATCH.CK1"),
                &read("keen1", "LEVEL90.CK1"),
                None,
            ),
        ),
        (
            "keen2",
            keen_graphics::vorticons_title(
                &read("keen2", "EGAHEAD.CK2"),
                &read("keen2", "EGALATCH.CK2"),
                &read("keen2", "LEVEL90.CK2"),
                None,
            ),
        ),
        (
            "keen3",
            keen_graphics::vorticons_title(
                &read("keen3", "EGAHEAD.CK3"),
                &read("keen3", "EGALATCH.CK3"),
                &read("keen3", "LEVEL90.CK3"),
                None,
            ),
        ),
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
        // Keen 1-3 don't need all 16 colors on their title.
        let colors = if ["keen1", "keen2", "keen3"].contains(&game) {
            12
        } else {
            16
        };
        assert!(image.color_count() >= colors, "{game}");
    }
}

#[test]
fn lzexe_unpacks_to_a_runnable_executable() {
    let word = |w: u16| w.to_le_bytes();
    // Load image: the packed bytes (literals "AB", then the end mark), and
    // in the next paragraph the unpacker's segment.
    let mut image = vec![0x0B, 0x00, b'A', b'B', 0x00, 0x00, 0x00];
    image.resize(16, 0);
    // Original IP, CS, SP, SS; packed size; growth; unpacker size; checksum.
    for w in [0x10, 0x20, 0x80, 0x30, 1, 0x40, 0x160, 0] {
        image.extend(word(w));
    }
    image.resize(16 + 0x158, 0x90);
    // Relocations: 5, then +0x20, then a 0xFFF0 jump and +1, then the end.
    image.extend([5, 0x20, 0, 0, 0, 1, 0, 1, 0]);

    let mut exe = vec![0u8; 0x20];
    exe[..2].copy_from_slice(b"MZ");
    exe[8..10].copy_from_slice(&word(2)); // header paragraphs
    exe[0xA..0xC].copy_from_slice(&word(0x100)); // minimum memory
    exe[0xC..0xE].copy_from_slice(&word(0xFFFF));
    exe[0x16..0x18].copy_from_slice(&word(1)); // CS: the unpacker
    exe[0x1C..0x20].copy_from_slice(b"LZ91");
    exe.extend(image);

    // Also without the signature (Keen 6 1.5), told by the unpacker code
    // at the entry point.
    let mut unsigned = exe.clone();
    unsigned[0x1C..0x20].fill(0);
    unsigned[0x14..0x16].copy_from_slice(&word(0x0E));
    let unpacker = [
        0x06, 0x0E, 0x1F, 0x8B, 0x0E, 0x0C, 0x00, 0x8B, 0xF1, 0x4E, 0x89, 0xF7, 0x8C, 0xDB, 0x03,
        0x1E, 0x0A, 0x00,
    ];
    unsigned[0x30 + 0x0E..0x30 + 0x0E + unpacker.len()].copy_from_slice(&unpacker);
    assert!(lzexe::is_lzexe91(&unsigned));
    assert_eq!(lzexe::load_image(&unsigned).unwrap(), b"AB");
    unsigned[0x14..0x16].copy_from_slice(&word(0));
    assert!(!lzexe::is_lzexe91(&unsigned));

    let out = lzexe::unpack_exe(&exe).unwrap();
    let field = |i: usize| u16::from_le_bytes([out[i * 2], out[i * 2 + 1]]);
    assert_eq!(out.len(), 0x200 + 2);
    assert_eq!(&out[0x200..], b"AB");
    assert_eq!(field(0), 0x5A4D);
    assert_eq!((field(1), field(2)), (2, 2)); // bytes in the last page, pages
    assert_eq!((field(3), field(4)), (3, 0x20)); // relocations, header paragraphs
    assert_eq!(field(5), 0x100 - (0x40 + 0x16 + 9));
    assert_eq!(field(6), 0xFFFF);
    assert_eq!(
        (field(7), field(8), field(10), field(11)),
        (0x30, 0x80, 0x10, 0x20)
    );
    assert_eq!(field(12), 0x1C);
    let relocs: Vec<u16> = (0..6).map(|i| field(0xE + i)).collect();
    assert_eq!(relocs, [5, 0, 5, 2, 6, 0x1001]);

    // Unpacked executables stay as they are.
    assert_eq!(lzexe::unpack_exe(&out).unwrap(), out);
}

#[test]
fn pklite_unpacks_the_load_image() {
    let mut exe = vec![0u8; 0x80];
    exe[..2].copy_from_slice(b"MZ");
    exe[8] = 2; // header paragraphs
    exe[0x1C..0x20].copy_from_slice(&[0x05, 0x21, b'P', b'K']); // 1.05, large
    exe[0x20 + 0x4E] = 0x16; // data at (2 + 0x16 - 0x10) paragraphs
    // One word of bits: literal, literal, copy (length 5: 0010, distance
    // high byte 0: 1), copy (escape 011100), first bit first. Bytes follow
    // in the order they are read: "A", "B", distance low byte 2, end mark.
    let bits = [0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 1, 1, 0, 0];
    let word: u16 = bits.iter().rev().fold(0, |w, &b| (w << 1) | b);
    exe.extend(word.to_le_bytes());
    exe.extend([b'A', b'B', 2, 0xFF]);

    assert!(pklite::is_pklite(&exe));
    assert_eq!(keen_graphics::load_image(&exe).unwrap(), b"ABABABA");
    exe[0x1D] = 0x01; // small model
    assert!(keen_graphics::load_image(&exe).is_err());
}

#[test]
fn cga_pictures_have_four_pixels_per_byte() {
    // Black, cyan, magenta, white; then white, magenta, cyan, black.
    let image = Image::from_cga(&[0b00_01_10_11, 0b11_10_01_00], 8, 1).unwrap();
    assert_eq!(image.pixels, [0, 11, 13, 15, 15, 13, 11, 0]);
    assert!(Image::from_cga(&[0], 8, 1).is_err());
}

#[test]
fn composite_colors_follow_the_pattern_of_four_pixels() {
    // White and black CGA pixels alternating: one-bit pixels 1100 in each
    // carrier cycle, orange on a composite monitor.
    let image = Image::from_cga(&[0b11_00_11_00; 4], 16, 1).unwrap();
    let rgb = image.to_composite_rgb();
    assert_eq!(rgb.len(), 16 * 3);
    assert_eq!(&rgb[8 * 3..9 * 3], &[0xDB, 0x56, 0x12]);
    // All white stays white, all black black.
    let white = Image::from_cga(&[0xFF; 4], 16, 1)
        .unwrap()
        .to_composite_rgb();
    assert_eq!(&white[8 * 3..9 * 3], &[0xFD; 3]);
    let black = Image::from_cga(&[0; 4], 16, 1).unwrap().to_composite_rgb();
    assert!(black.iter().all(|&c| c == 0));
}

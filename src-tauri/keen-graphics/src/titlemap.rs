//! The title screen of Keen Dreams v1.0 and the CGA release v1.05: no picture
//! but map 20 of `GAMEMAPS.KDR`, drawn from the 16x16 tiles of the graphics
//! file. The map's header table (MAPHEAD) and its Huffman dictionary (MAPDICT)
//! are built into the executable. See `DemoLoop` in `kd_demo.c` and
//! `CA_CacheMap` in `id_ca.c` of the game's source.

use crate::egagraph::EgaGraph;
use crate::huffman::Dictionary;
use crate::{CGA_COLORS, Error, Image};

const TITLE_MAP: usize = 20;
/// The tag of RLEW runs, which MAPHEAD starts with.
const RLEW_TAG: u16 = 0xABCD;
/// The tag, 100 offsets of 4 bytes and 100 header sizes of 1 byte.
const MAP_HEAD_LEN: usize = 2 + 400 + 100;
const MAP_HEADER_LEN: usize = 38;
const START_TILE16: usize = 370;
const START_TILE16M: usize = 1810;
const TILE: usize = 16;
/// Where the game puts the screen on the map: 5.5 and 2.75 tiles in.
const ORIGIN: (usize, usize) = (88, 44);
const SCREEN: (usize, usize) = (320, 200);

/// The title map's size and its background and foreground tile numbers.
struct TitleMap {
    width: usize,
    height: usize,
    background: Vec<u16>,
    foreground: Vec<u16>,
}

/// Draws the title map from `maps` and `graph`, with the header table and
/// dictionary from the unpacked executable `image`.
pub fn title(image: &[u8], maps: &[u8], graph: &EgaGraph) -> Result<Image, Error> {
    let map = find_map(image, maps)?;
    let cga = graph.is_cga();
    let cell = |x: usize, y: usize| {
        (x < map.width && y < map.height)
            .then_some(y * map.width + x)
            .ok_or(Error::Format("the title map is too small"))
    };
    let mut pixels = vec![0u8; SCREEN.0 * SCREEN.1];
    for tile_y in ORIGIN.1 / TILE..=(ORIGIN.1 + SCREEN.1 - 1) / TILE {
        for tile_x in ORIGIN.0 / TILE..=(ORIGIN.0 + SCREEN.0 - 1) / TILE {
            let at = cell(tile_x, tile_y)?;
            let mut tile = background(graph, map.background[at], cga)?;
            if map.foreground[at] != 0 {
                let (mask, over) = foreground(graph, map.foreground[at], cga)?;
                for (i, value) in tile.iter_mut().enumerate() {
                    *value = (*value & mask[i]) | over[i];
                }
            }
            for (i, &value) in tile.iter().enumerate() {
                let x = (tile_x * TILE + i % TILE).checked_sub(ORIGIN.0);
                let y = (tile_y * TILE + i / TILE).checked_sub(ORIGIN.1);
                if let (Some(x), Some(y)) = (x, y)
                    && x < SCREEN.0
                    && y < SCREEN.1
                {
                    pixels[y * SCREEN.0 + x] = if cga {
                        CGA_COLORS[usize::from(value & 3)]
                    } else {
                        value
                    };
                }
            }
        }
    }
    Ok(Image {
        width: SCREEN.0,
        height: SCREEN.1,
        pixels,
    })
}

/// A tile's pixels as stored: EGA color indexes or two-bit CGA values.
fn tile_pixels(data: &[u8], cga: bool) -> Result<Vec<u8>, Error> {
    if cga {
        Ok((0..TILE * TILE)
            .map(|i| (data[i / 4] >> (6 - 2 * (i % 4))) & 3)
            .collect())
    } else {
        Ok(Image::from_planar(data, TILE, TILE)?.pixels)
    }
}

fn background(graph: &EgaGraph, tile: u16, cga: bool) -> Result<Vec<u8>, Error> {
    let len = if cga { 64 } else { 128 };
    let data = graph.sized_chunk(START_TILE16 + usize::from(tile), len)?;
    tile_pixels(&data, cga)
}

/// A masked tile: the bits kept of what is behind it, and what it adds.
fn foreground(graph: &EgaGraph, tile: u16, cga: bool) -> Result<(Vec<u8>, Vec<u8>), Error> {
    let (mask_len, len) = if cga { (64, 128) } else { (32, 160) };
    let data = graph.sized_chunk(START_TILE16M + usize::from(tile), len)?;
    let (mask, over) = data.split_at(mask_len);
    let mask = if cga {
        tile_pixels(mask, true)?
    } else {
        (0..TILE * TILE)
            .map(|i| ((mask[i / 8] >> (7 - i % 8)) & 1) * 15)
            .collect()
    };
    Ok((mask, tile_pixels(over, cga)?))
}

/// Searches `image` for the header table and dictionary that read the title
/// map of `maps`. Neither has a mark, so the proof is a map that decodes.
fn find_map(image: &[u8], maps: &[u8]) -> Result<TitleMap, Error> {
    let header = image
        .windows(MAP_HEAD_LEN)
        .filter(|w| w[..2] == RLEW_TAG.to_le_bytes())
        .find_map(|head| title_header(head, maps))
        .ok_or(Error::Format("MAPHEAD not found in the executable"))?;
    (0..image.len())
        .filter_map(|at| Dictionary::parse(&image[at..]).ok())
        .find_map(|dict| read_map(header, maps, &dict))
        .ok_or(Error::Format("MAPDICT not found in the executable"))
}

/// The compressed header of the title map, if `head` is a header table for
/// `maps`: every offset is empty or inside the file.
fn title_header<'a>(head: &[u8], maps: &'a [u8]) -> Option<&'a [u8]> {
    let offset = |i: usize| i32::from_le_bytes(head[2 + i * 4..][..4].try_into().expect("4 bytes"));
    let valid = (0..100).all(|i| {
        let o = offset(i);
        o <= 0 || (o as usize) < maps.len()
    });
    let start = usize::try_from(offset(TITLE_MAP)).ok().filter(|_| valid)?;
    let size = usize::from(head[402 + TITLE_MAP]);
    (start > 0 && size > 0).then(|| maps.get(start..start + size))?
}

fn read_map(header: &[u8], maps: &[u8], dict: &Dictionary) -> Option<TitleMap> {
    let header = dict.decode(header, MAP_HEADER_LEN).ok()?;
    let word = |at: usize| usize::from(u16::from_le_bytes([header[at], header[at + 1]]));
    let long = |at: usize| u32::from_le_bytes(header[at..at + 4].try_into().expect("4 bytes"));
    let (width, height) = (word(18), word(20));
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return None;
    }
    let plane = |i: usize| -> Option<Vec<u16>> {
        let start = long(i * 4) as usize;
        let data = maps.get(start..start.checked_add(word(12 + i * 2))?)?;
        let (size, data) = data.split_first_chunk::<2>()?;
        let expanded = dict
            .decode(data, usize::from(u16::from_le_bytes(*size)))
            .ok()?;
        rlew_expand(expanded.get(2..)?, width * height)
    };
    Some(TitleMap {
        width,
        height,
        background: plane(0)?,
        foreground: plane(1)?,
    })
}

/// Expands id's run-length coding to `len` words: a tag, a count and a value
/// make a run, anything else is itself.
fn rlew_expand(data: &[u8], len: usize) -> Option<Vec<u16>> {
    let mut words = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|w| u16::from_le_bytes(*w));
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let word = words.next()?;
        if word == RLEW_TAG {
            let (count, value) = (usize::from(words.next()?), words.next()?);
            let count = count.min(len - out.len());
            out.extend(std::iter::repeat_n(value, count));
        } else {
            out.push(word);
        }
    }
    Some(out)
}

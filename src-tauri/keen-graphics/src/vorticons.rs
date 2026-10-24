//! The title screen of Keen 1-3: the title bitmap drawn over the title
//! level (`LEVEL90`). See
//! <https://moddingwiki.shikadi.net/wiki/Commander_Keen_1-3_EGA_Header>,
//! <https://moddingwiki.shikadi.net/wiki/Keen_1-3_LZW_compression> and
//! <https://moddingwiki.shikadi.net/wiki/Commander_Keen_1-3_Level_format>.

use crate::{Error, Image};

/// The bit in the header's compression flags for a compressed EGALATCH.
const LATCH_COMPRESSED: u16 = 2;

struct Header<'a> {
    head: &'a [u8],
    /// How much later than usual the fields from the tile count on are.
    shift: usize,
    plane_size: usize,
    tiles16: usize,
    tile_count: usize,
    bitmaps: usize,
}

impl<'a> Header<'a> {
    fn parse(head: &'a [u8]) -> Result<Self, Error> {
        if head.len() < 48 {
            return Err(Error::Format("EGAHEAD too short"));
        }
        let u16_at = |at: usize| usize::from(u16::from_le_bytes([head[at], head[at + 1]]));
        let u32_at = |at: usize| {
            u32::from_le_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]) as usize
        };
        // Keen 1 1.32 and later have one more count and location after
        // the screen graphics', a 64-byte header instead of 48: the bitmap
        // table starts later.
        let shift = if u32_at(8) >= 54 { 6 } else { 0 };
        if head.len() < 48 + shift {
            return Err(Error::Format("EGAHEAD too short"));
        }
        Ok(Header {
            head,
            shift,
            plane_size: u32_at(0),
            tile_count: u16_at(28 + shift),
            tiles16: u32_at(30 + shift),
            bitmaps: u32_at(36 + shift),
        })
    }

    fn compressed(&self) -> bool {
        u16::from_le_bytes([self.head[46 + self.shift], self.head[47 + self.shift]])
            & LATCH_COMPRESSED
            != 0
    }

    /// Width in bytes, height and location of the first bitmap, the title.
    /// The game picks it by number; mod tools often leave the names blank.
    fn title(&self) -> Option<(usize, usize, usize)> {
        let table = u32::from_le_bytes(self.head[8..12].try_into().ok()?) as usize;
        let count = u16::from_le_bytes([self.head[34 + self.shift], self.head[35 + self.shift]]);
        let e = self.head.get(table..table + 16).filter(|_| count > 0)?;
        Some((
            usize::from(u16::from_le_bytes([e[0], e[1]])),
            usize::from(u16::from_le_bytes([e[2], e[3]])),
            u32::from_le_bytes([e[4], e[5], e[6], e[7]]) as usize,
        ))
    }
}

/// The 16x16 tiles in a file of the tileset loader patch some mods use,
/// like Mura and Prelude. Its header has the tile count, the frames and the
/// length of a plane, then where the tile info, the animations and the four
/// planes of tiles are.
struct Tileset<'a> {
    count: usize,
    planes: Vec<&'a [u8]>,
}

impl<'a> Tileset<'a> {
    fn parse(tls: &'a [u8]) -> Result<Self, Error> {
        let field = |at: usize, len: usize| {
            tls.get(at..at + len)
                .map(|b| b.iter().rev().fold(0, |n, &b| n << 8 | usize::from(b)))
                .ok_or(Error::Format("tileset too short"))
        };
        let (count, plane_size, at) = (field(0, 2)?, field(4, 2)?, field(14, 4)?);
        let planes = (0..4)
            .map(|p| tls.get(at + p * plane_size..at + (p + 1) * plane_size))
            .collect::<Option<Vec<_>>>()
            .ok_or(Error::Format("tileset too short"))?;
        Ok(Tileset { count, planes })
    }
}

/// The tileset file the loader patch takes for `level` from `TILEHEAD`:
/// 16 bytes per level, a name ending in a zero byte.
pub fn tileset_name(tilehead: &[u8], level: usize) -> Option<String> {
    let entry = tilehead.get(level * 16..level * 16 + 16)?;
    let name = entry.split(|&b| b == 0).next()?;
    // DOS cuts the name off at a space.
    let name = String::from_utf8_lossy(name).split(' ').next()?.to_string();
    (!name.is_empty()).then_some(name)
}

/// Whether `EGALATCH` is shorter than `EGAHEAD` says, so the game leaves
/// parts of its graphics empty and scrambles the rest.
pub fn latch_is_short(egahead: &[u8], egalatch: &[u8]) -> bool {
    Header::parse(egahead).is_ok_and(|h| !h.compressed() && egalatch.len() < h.plane_size * 4)
}

/// The title screen from `EGAHEAD`, `EGALATCH` and `LEVEL90` of Keen 1-3,
/// with the tiles from a `tileset` file if the mod has one.
pub fn title(
    egahead: &[u8],
    egalatch: &[u8],
    level: &[u8],
    tileset: Option<&[u8]>,
) -> Result<Image, Error> {
    let header = Header::parse(egahead)?;
    let mut latch = if header.compressed() {
        lzw(egalatch)?
    } else {
        egalatch.to_vec()
    };
    if header.plane_size == 0 || latch.len() < header.plane_size {
        return Err(Error::Format("EGALATCH too short"));
    }
    // The game reads what the file has and leaves the rest of the planes
    // empty, as Keen Static's scrambled graphics show: a Keen 3 header
    // over Keen 1's compressed EGALATCH, read as is.
    if !header.compressed() {
        latch.resize(latch.len().max(header.plane_size * 4), 0);
    } else if latch.len() < header.plane_size * 4 {
        return Err(Error::Format("EGALATCH too short"));
    }
    let planes: Vec<&[u8]> = latch.chunks_exact(header.plane_size).take(4).collect();
    let mut screen = Screen {
        pixels: vec![0; WIDTH * HEIGHT],
    };

    let (w, h, at) = header.title().ok_or(Error::Format("no title bitmap"))?;
    // The level as the game shows it behind the title: scrolled past the
    // two-tile border. A full-screen title hides it.
    if w * 8 < WIDTH || h < HEIGHT {
        let (tile_planes, tile_count, tiles_at) = match tileset {
            Some(tls) => {
                let tileset = Tileset::parse(tls)?;
                (tileset.planes, tileset.count, 0)
            }
            None => (planes.clone(), header.tile_count, header.tiles16),
        };
        let (map_w, map_h, tiles) = level_tiles(level)?;
        for ty in 0..HEIGHT.div_ceil(16).min(map_h.saturating_sub(2)) {
            for tx in 0..(WIDTH / 16).min(map_w.saturating_sub(2)) {
                let tile = usize::from(tiles[(ty + 2) * map_w + tx + 2]);
                if tile < tile_count {
                    screen.draw(&tile_planes, tiles_at + tile * 32, 2, 16, tx * 16, ty * 16)?;
                }
            }
        }
    }
    // Mods with a bigger title get it in the corner, as far as it fits.
    let (x, y) = TITLE_AT;
    let x = if w * 8 + x > WIDTH { 0 } else { x };
    let y = if h + y > HEIGHT { 0 } else { y };
    screen.draw(&planes, header.bitmaps + at, w, h, x, y)?;
    Ok(Image {
        width: WIDTH,
        height: HEIGHT,
        pixels: screen.pixels,
    })
}

const WIDTH: usize = 320;
const HEIGHT: usize = 200;

/// Where the game draws the title bitmap. Its "Press F1 for help" sign is
/// left out: it sits somewhere else in each episode.
const TITLE_AT: (usize, usize) = (32, 1);

struct Screen {
    pixels: Vec<u8>,
}

impl Screen {
    /// Draws the planar graphic at `start` in each of `planes`, `w` bytes
    /// wide and `h` rows high, with its top left corner at `x`, `y`; cut off
    /// at the screen edges.
    fn draw(
        &mut self,
        planes: &[&[u8]],
        start: usize,
        w: usize,
        h: usize,
        x: usize,
        y: usize,
    ) -> Result<(), Error> {
        for (p, plane) in planes.iter().enumerate() {
            let data = plane
                .get(start..start + w * h)
                .ok_or(Error::Format("graphic outside EGALATCH"))?;
            for row in 0..h.min(HEIGHT.saturating_sub(y)) {
                for col in 0..(w * 8).min(WIDTH.saturating_sub(x)) {
                    let bit = (data[row * w + col / 8] >> (7 - col % 8)) & 1;
                    let px = &mut self.pixels[(y + row) * WIDTH + x + col];
                    *px = if p == 0 { bit } else { *px | bit << p };
                }
            }
        }
        Ok(())
    }
}

/// Width, height and tile plane of a level file.
fn level_tiles(level: &[u8]) -> Result<(usize, usize, Vec<u16>), Error> {
    let words = rlew(level)?;
    if words.len() < 16 {
        return Err(Error::Format("level too short"));
    }
    let (w, h) = (usize::from(words[0]), usize::from(words[1]));
    let tiles = words
        .get(16..16 + w * h)
        .ok_or(Error::Format("level too short"))?;
    Ok((w, h, tiles.to_vec()))
}

/// RLEW with the marker `FEFE`, after a 4-byte decompressed size.
fn rlew(data: &[u8]) -> Result<Vec<u16>, Error> {
    if data.len() < 4 {
        return Err(Error::Format("level too short"));
    }
    let size = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize / 2;
    if size > 1 << 20 {
        return Err(Error::Format("level implausibly large"));
    }
    let mut words = data[4..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&w| u16::from_le_bytes(w));
    let mut out = Vec::with_capacity(size);
    while out.len() < size {
        let Some(w) = words.next() else { break };
        if w == 0xFEFE {
            let (count, value) = words
                .next()
                .zip(words.next())
                .ok_or(Error::Format("level ends early"))?;
            out.extend(std::iter::repeat_n(value, usize::from(count)));
        } else {
            out.push(w);
        }
    }
    out.truncate(size);
    Ok(out)
}

/// LZW with 9 to 12-bit codes, most significant bit first, after a 4-byte
/// decompressed size and a 2-byte maximum code width. `100` is unused,
/// `101` ends the data.
pub fn lzw(data: &[u8]) -> Result<Vec<u8>, Error> {
    if data.len() < 6 {
        return Err(Error::Format("LZW data too short"));
    }
    let size = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if size > 1 << 21 {
        return Err(Error::Format("LZW data implausibly large"));
    }
    let data = &data[6..];
    let mut pos = 0usize;
    let mut read = |width: usize| -> Option<usize> {
        let mut code = 0;
        for _ in 0..width {
            let byte = *data.get(pos / 8)?;
            code = (code << 1) | usize::from((byte >> (7 - pos % 8)) & 1);
            pos += 1;
        }
        Some(code)
    };
    // Each entry: (prefix code, last byte); codes below 256 are bytes.
    let mut dict: Vec<(usize, u8)> = (0..=255).map(|b| (usize::MAX, b as u8)).collect();
    dict.push((usize::MAX, 0));
    dict.push((usize::MAX, 0));
    let expand = |dict: &[(usize, u8)], mut code: usize, buf: &mut Vec<u8>| {
        let start = buf.len();
        loop {
            let (prefix, byte) = dict[code];
            buf.push(byte);
            if prefix == usize::MAX {
                break;
            }
            code = prefix;
        }
        buf[start..].reverse();
    };
    let mut out = Vec::with_capacity(size);
    let mut width = 9;
    let mut prev: Option<usize> = None;
    while out.len() < size {
        let code = read(width).ok_or(Error::Format("LZW data ends early"))?;
        if code == 0x101 {
            break;
        }
        if code == 0x100 || code > dict.len() || (code == dict.len() && prev.is_none()) {
            return Err(Error::Format("bad LZW code"));
        }
        let start = out.len();
        if code < dict.len() {
            expand(&dict, code, &mut out);
        } else {
            // The entry being defined: the previous string plus its first byte.
            expand(&dict, prev.unwrap_or(0), &mut out);
            out.push(out[start]);
        }
        if let Some(p) = prev
            && dict.len() < 4096
        {
            dict.push((p, out[start]));
        }
        prev = Some(code);
        if dict.len() + 1 >= 1 << width && width < 12 {
            width += 1;
        }
    }
    out.truncate(size);
    Ok(out)
}

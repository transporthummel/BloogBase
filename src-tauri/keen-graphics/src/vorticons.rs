//! The title screen of Keen 1-3: the `TITLE` bitmap drawn over the title
//! level (`LEVEL90`). See
//! <https://moddingwiki.shikadi.net/wiki/Commander_Keen_1-3_EGA_Header>,
//! <https://moddingwiki.shikadi.net/wiki/Keen_1-3_LZW_compression> and
//! <https://moddingwiki.shikadi.net/wiki/Commander_Keen_1-3_Level_format>.

use crate::{Error, Image};

/// The bit in the header's compression flags for a compressed EGALATCH.
const LATCH_COMPRESSED: u16 = 2;

struct Header<'a> {
    head: &'a [u8],
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
        Ok(Header {
            head,
            plane_size: u32_at(0),
            tile_count: u16_at(28),
            tiles16: u32_at(30),
            bitmaps: u32_at(36),
        })
    }

    fn compressed(&self) -> bool {
        u16::from_le_bytes([self.head[46], self.head[47]]) & LATCH_COMPRESSED != 0
    }

    /// Width in bytes, height and location of the bitmap called `name`.
    fn bitmap(&self, name: &str) -> Option<(usize, usize, usize)> {
        let table = u32::from_le_bytes(self.head[8..12].try_into().ok()?) as usize;
        let count = usize::from(u16::from_le_bytes([self.head[34], self.head[35]]));
        (0..count).find_map(|i| {
            let e = self.head.get(table + i * 16..table + (i + 1) * 16)?;
            let entry_name: Vec<u8> = e[8..].iter().copied().take_while(|&b| b != 0).collect();
            entry_name.eq_ignore_ascii_case(name.as_bytes()).then(|| {
                (
                    usize::from(u16::from_le_bytes([e[0], e[1]])),
                    usize::from(u16::from_le_bytes([e[2], e[3]])),
                    u32::from_le_bytes([e[4], e[5], e[6], e[7]]) as usize,
                )
            })
        })
    }
}

/// The title screen from `EGAHEAD`, `EGALATCH` and `LEVEL90` of Keen 1-3.
pub fn title(egahead: &[u8], egalatch: &[u8], level: &[u8]) -> Result<Image, Error> {
    let header = Header::parse(egahead)?;
    let latch = if header.compressed() {
        lzw(egalatch)?
    } else {
        egalatch.to_vec()
    };
    if header.plane_size == 0 || latch.len() < header.plane_size * 4 {
        return Err(Error::Format("EGALATCH too short"));
    }
    let mut screen = Screen {
        planes: latch.chunks_exact(header.plane_size).take(4).collect(),
        pixels: vec![0; WIDTH * HEIGHT],
    };

    // The level as the game shows it behind the title: scrolled past the
    // two-tile border.
    let (map_w, map_h, tiles) = level_tiles(level)?;
    for ty in 0..HEIGHT.div_ceil(16).min(map_h.saturating_sub(2)) {
        for tx in 0..(WIDTH / 16).min(map_w.saturating_sub(2)) {
            let tile = usize::from(tiles[(ty + 2) * map_w + tx + 2]);
            if tile < header.tile_count {
                screen.draw(header.tiles16 + tile * 32, 2, 16, tx * 16, ty * 16)?;
            }
        }
    }
    let (w, h, at) = header
        .bitmap("TITLE")
        .ok_or(Error::Format("no TITLE bitmap"))?;
    screen.draw(header.bitmaps + at, w, h, TITLE_AT.0, TITLE_AT.1)?;
    Ok(Image {
        width: WIDTH,
        height: HEIGHT,
        pixels: screen.pixels,
    })
}

const WIDTH: usize = 320;
const HEIGHT: usize = 200;

/// Where the game draws the `TITLE` bitmap. Its "Press F1 for help" sign is
/// left out: it sits somewhere else in each episode.
const TITLE_AT: (usize, usize) = (32, 1);

struct Screen<'a> {
    planes: Vec<&'a [u8]>,
    pixels: Vec<u8>,
}

impl Screen<'_> {
    /// Draws the planar graphic at `start` in each plane, `w` bytes wide and
    /// `h` rows high, with its top left corner at `x`, `y`; cut off at the
    /// screen edges.
    fn draw(&mut self, start: usize, w: usize, h: usize, x: usize, y: usize) -> Result<(), Error> {
        for (p, plane) in self.planes.iter().enumerate() {
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

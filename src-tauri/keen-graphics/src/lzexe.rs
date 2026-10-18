//! Unpacks executables compressed with LZEXE 0.91, as most Keen executables
//! are: the load image, to read the data embedded in it, or the whole
//! executable with its relocations, to run it patched.
//!
//! Format: <https://moddingwiki.shikadi.net/wiki/LZEXE> and Mitugu Kurizono's
//! description of the 0.91 bit stream.

use crate::Error;

/// Whether `exe` is packed with LZEXE 0.91 (`LZ91` in the MZ header).
pub fn is_lzexe91(exe: &[u8]) -> bool {
    exe.len() > 0x20 && exe.starts_with(b"MZ") && &exe[0x1C..0x20] == b"LZ91"
}

/// The unpacked load image, or the load image as is if it isn't packed.
pub fn load_image(exe: &[u8]) -> Result<Vec<u8>, Error> {
    if exe.len() < 0x20 || !exe.starts_with(b"MZ") {
        return Err(Error::Format("not a DOS executable"));
    }
    let header = usize::from(u16::from_le_bytes([exe[8], exe[9]])) * 16;
    let data = exe
        .get(header..)
        .ok_or(Error::Format("truncated executable"))?;
    if !is_lzexe91(exe) {
        return Ok(data.to_vec());
    }
    unpack(data)
}

/// The executable unpacked, with its relocations restored, so it runs in
/// place of the packed one; an executable that isn't packed as is. The load
/// image starts at the header size in both, as with [`load_image`].
pub fn unpack_exe(exe: &[u8]) -> Result<Vec<u8>, Error> {
    let image = load_image(exe)?;
    if !is_lzexe91(exe) {
        return Ok(exe.to_vec());
    }
    let word = |at: usize| {
        exe.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .ok_or(Error::Format("truncated executable"))
    };
    // The unpacker's segment starts with the original registers, followed
    // by its code and the packed relocation table.
    let loader = (usize::from(word(8)?) + usize::from(word(0x16)?)) * 16;
    let [ip, cs, sp, ss] = [0, 2, 4, 6].map(|o| word(loader + o));
    let relocs = relocations(
        exe.get(loader + 0x158..)
            .ok_or(Error::Format("truncated executable"))?,
    )?;

    let header_len = (0x1C + relocs.len() * 4).next_multiple_of(0x200);
    let size = header_len + image.len();
    // Less the room the unpacking took, as UNLZEXE computes it.
    let min_alloc =
        word(0xA)?.saturating_sub(word(loader + 0xA)? + word(loader + 0xC)?.div_ceil(16) + 9);
    let fields = [
        0x5A4D,
        (size % 512) as u16,
        size.div_ceil(512) as u16,
        relocs.len() as u16,
        (header_len / 16) as u16,
        min_alloc,
        word(0xC)?,
        ss?,
        sp?,
        0,
        ip?,
        cs?,
        0x1C,
        0,
    ];
    let mut out: Vec<u8> = fields.iter().flat_map(|w| w.to_le_bytes()).collect();
    for (offset, segment) in relocs {
        out.extend(offset.to_le_bytes());
        out.extend(segment.to_le_bytes());
    }
    out.resize(header_len, 0);
    out.extend(image);
    Ok(out)
}

/// LZEXE 0.91's relocation table: each entry as the distance from the one
/// before, in a byte, or a word after a zero byte. A zero word moves on by
/// 0xFFF0 bytes, a word of 1 ends the table.
fn relocations(data: &[u8]) -> Result<Vec<(u16, u16)>, Error> {
    let mut bytes = data.iter().copied();
    let mut next = || {
        bytes
            .next()
            .ok_or(Error::Format("LZEXE relocations end early"))
    };
    let (mut segment, mut offset) = (0u16, 0u16);
    let mut out = Vec::new();
    loop {
        let mut span = u16::from(next()?);
        if span == 0 {
            span = u16::from_le_bytes([next()?, next()?]);
            match span {
                0 => {
                    segment = segment.wrapping_add(0xFFF);
                    continue;
                }
                1 => break,
                _ => {}
            }
        }
        offset = offset.wrapping_add(span);
        segment = segment.wrapping_add(offset >> 4);
        offset &= 0xF;
        out.push((offset, segment));
    }
    Ok(out)
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u16,
    left: u8,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Result<Self, Error> {
        let mut bits = Bits {
            data,
            pos: 0,
            buf: 0,
            left: 0,
        };
        bits.buf = bits.word()?;
        bits.left = 16;
        Ok(bits)
    }

    fn byte(&mut self) -> Result<u8, Error> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or(Error::Format("LZEXE data ends early"))?;
        self.pos += 1;
        Ok(b)
    }

    fn word(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes([self.byte()?, self.byte()?]))
    }

    /// Bits come least significant first; the next word is loaded as soon as
    /// the current one is used up, before any bytes that follow.
    fn bit(&mut self) -> Result<u16, Error> {
        let bit = self.buf & 1;
        self.buf >>= 1;
        self.left -= 1;
        if self.left == 0 {
            self.buf = self.word()?;
            self.left = 16;
        }
        Ok(bit)
    }
}

fn unpack(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut bits = Bits::new(data)?;
    let mut out: Vec<u8> = Vec::with_capacity(data.len() * 2);
    loop {
        if bits.bit()? == 1 {
            out.push(bits.byte()?);
            continue;
        }
        // How far back the copy starts, and how many bytes it has.
        let (distance, len) = if bits.bit()? == 0 {
            let len = usize::from((bits.bit()? << 1) | bits.bit()?) + 2;
            (0x100 - usize::from(bits.byte()?), len)
        } else {
            let lo = usize::from(bits.byte()?);
            let hi = usize::from(bits.byte()?);
            let distance = 0x2000 - (lo | ((hi & 0xF8) << 5));
            let len = match hi & 7 {
                0 => match bits.byte()? {
                    0 => break,
                    1 => continue, // segment change, nothing to copy
                    n => usize::from(n) + 1,
                },
                n => n + 2,
            };
            (distance, len)
        };
        let start = out
            .len()
            .checked_sub(distance)
            .ok_or(Error::Format("LZEXE reference before start"))?;
        for i in start..start + len {
            out.push(out[i]);
        }
    }
    Ok(out)
}

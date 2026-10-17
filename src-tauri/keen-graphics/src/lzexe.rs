//! Unpacks executables compressed with LZEXE 0.91, as the Keen 4-6 and Keen
//! Dreams executables are. Only the load image is restored (no relocations):
//! enough to read the data that's embedded in it.
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

//! Unpacks the load image of executables compressed with PKLITE 1.x, as the
//! Keen 4 special demo and Keen Dreams 1.0 are, to read the data embedded in
//! it. Only the "large" model, the one those use.
//!
//! Format: <https://moddingwiki.shikadi.net/wiki/PKLite> (header, where the
//! data starts) and OpenTESArena's PKLITE 1.12 specification (bit stream and
//! code tables).

use crate::Error;

/// Whether `exe` is packed with PKLITE (`PK` copyright after the version).
pub fn is_pklite(exe: &[u8]) -> bool {
    exe.len() > 0x20 && exe.starts_with(b"MZ") && &exe[0x1E..0x20] == b"PK"
}

/// The unpacked load image.
pub fn load_image(exe: &[u8]) -> Result<Vec<u8>, Error> {
    let version = u16::from_le_bytes([exe[0x1C], exe[0x1D]]);
    if version & 0x2000 == 0 {
        return Err(Error::Format("PKLITE small model not supported"));
    }
    let extra = version & 0x1000 != 0;
    let header = usize::from(u16::from_le_bytes([exe[8], exe[9]]));
    // The unpacker adds this many paragraphs (from load address 0x100) to
    // find the packed data.
    let skip = usize::from(
        *exe.get(header * 16 + 0x4E)
            .ok_or(Error::Format("truncated executable"))?,
    );
    let start = (header + skip)
        .checked_sub(0x10)
        .map(|p| p * 16)
        .filter(|&s| s < exe.len())
        .ok_or(Error::Format("PKLITE data not found"))?;
    unpack(&exe[start..], extra)
}

/// Copy lengths; `None` is the escape to a length byte.
const LENGTHS: &[(&str, Option<usize>)] = &[
    ("10", Some(2)),
    ("11", Some(3)),
    ("000", Some(4)),
    ("0010", Some(5)),
    ("0011", Some(6)),
    ("0100", Some(7)),
    ("01010", Some(8)),
    ("01011", Some(9)),
    ("01100", Some(10)),
    ("011010", Some(11)),
    ("011011", Some(12)),
    ("011100", None),
    ("0111010", Some(13)),
    ("0111011", Some(14)),
    ("0111100", Some(15)),
    ("01111010", Some(16)),
    ("01111011", Some(17)),
    ("01111100", Some(18)),
    ("011111010", Some(19)),
    ("011111011", Some(20)),
    ("011111100", Some(21)),
    ("011111101", Some(22)),
    ("011111110", Some(23)),
    ("011111111", Some(24)),
];

/// The high byte of copy distances.
const DISTANCES: &[(&str, usize)] = &[
    ("1", 0),
    ("0000", 1),
    ("0001", 2),
    ("00100", 3),
    ("00101", 4),
    ("00110", 5),
    ("00111", 6),
    ("010000", 7),
    ("010001", 8),
    ("010010", 9),
    ("010011", 10),
    ("010100", 11),
    ("010101", 12),
    ("010110", 13),
    ("0101110", 14),
    ("0101111", 15),
    ("0110000", 16),
    ("0110001", 17),
    ("0110010", 18),
    ("0110011", 19),
    ("0110100", 20),
    ("0110101", 21),
    ("0110110", 22),
    ("0110111", 23),
    ("0111000", 24),
    ("0111001", 25),
    ("0111010", 26),
    ("0111011", 27),
    ("0111100", 28),
    ("0111101", 29),
    ("0111110", 30),
    ("0111111", 31),
];

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
            left: 16,
        };
        bits.buf = bits.word()?;
        Ok(bits)
    }

    fn byte(&mut self) -> Result<u8, Error> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or(Error::Format("PKLITE data ends early"))?;
        self.pos += 1;
        Ok(b)
    }

    fn word(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes([self.byte()?, self.byte()?]))
    }

    /// Bits come least significant first; the next word is loaded as soon as
    /// the current one is used up, before any bytes that follow.
    fn bit(&mut self) -> Result<bool, Error> {
        let bit = self.buf & 1 == 1;
        self.buf >>= 1;
        self.left -= 1;
        if self.left == 0 {
            self.buf = self.word()?;
            self.left = 16;
        }
        Ok(bit)
    }

    /// Reads bits until they spell one of the `codes`.
    fn code<T: Copy>(&mut self, codes: &[(&str, T)]) -> Result<T, Error> {
        let mut read = String::new();
        while read.len() < 9 {
            read.push(if self.bit()? { '1' } else { '0' });
            if let Some(&(_, value)) = codes.iter().find(|(c, _)| *c == read) {
                return Ok(value);
            }
        }
        Err(Error::Format("bad PKLITE code"))
    }
}

fn unpack(data: &[u8], extra: bool) -> Result<Vec<u8>, Error> {
    let mut bits = Bits::new(data)?;
    let mut out: Vec<u8> = Vec::with_capacity(data.len() * 2);
    loop {
        if !bits.bit()? {
            // With "extra" compression literals are XORed with the number of
            // bits left in the current word.
            let key = if extra { bits.left } else { 0 };
            out.push(bits.byte()? ^ key);
            continue;
        }
        let len = match bits.code(LENGTHS)? {
            Some(len) => len,
            None => match bits.byte()? {
                0xFF => break,
                0xFE => continue, // segment change, nothing to copy
                n => usize::from(n) + 25,
            },
        };
        let hi = if len == 2 { 0 } else { bits.code(DISTANCES)? };
        let distance = (hi << 8) | usize::from(bits.byte()?);
        let start = out
            .len()
            .checked_sub(distance)
            .filter(|_| distance > 0)
            .ok_or(Error::Format("PKLITE reference before start"))?;
        for i in start..start + len {
            out.push(out[i]);
        }
    }
    Ok(out)
}

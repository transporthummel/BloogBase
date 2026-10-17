//! Softdisk Library files (`SLIB`), which hold the Keen Dreams title screen
//! in `KDREAMS.CMP`. See
//! <https://moddingwiki.shikadi.net/wiki/Softdisk_Library_Format>.
//!
//! The LZH variant is LZSS with adaptive Huffman coding as described by
//! Haruyasu Yoshizaki and Haruhiko Okumura (1988), with a 30-byte look-ahead.

use crate::Error;

/// The decompressed entry named `name` (case-insensitive).
pub fn entry(lib: &[u8], name: &str) -> Result<Vec<u8>, Error> {
    if lib.len() < 8 || &lib[..4] != b"SLIB" {
        return Err(Error::Format("not a Softdisk library"));
    }
    let count = usize::from(u16::from_le_bytes([lib[6], lib[7]]));
    let data_start = 8 + count * 30;
    for i in 0..count {
        let e = lib
            .get(8 + i * 30..8 + (i + 1) * 30)
            .ok_or(Error::Format("Softdisk library header ends early"))?;
        let entry_name: String = e[..16]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| b as char)
            .collect();
        if !entry_name.eq_ignore_ascii_case(name) {
            continue;
        }
        let le32 =
            |at: usize| u32::from_le_bytes([e[at], e[at + 1], e[at + 2], e[at + 3]]) as usize;
        // The end field isn't used: tools disagree on whether it counts the
        // chunk header. The decoders stop at the decompressed size.
        let (start, size) = (le32(16), le32(24));
        let chunk = lib
            .get(data_start + start..)
            .filter(|c| c.len() >= 10 && &c[..4] == b"CUNK")
            .ok_or(Error::Format("Softdisk library entry outside the file"))?;
        let compression = u16::from_le_bytes([chunk[8], chunk[9]]);
        let data = &chunk[10..];
        return match compression {
            // Some mod tools write sizes 8 bytes too large; keep what's there.
            0 => Ok(data[..size.min(data.len())].to_vec()),
            2 => lzh(data, size),
            _ => Err(Error::Format("unsupported Softdisk compression")),
        };
    }
    Err(Error::Format("entry not in the Softdisk library"))
}

const N: usize = 4096;
const F: usize = 30;
const THRESHOLD: usize = 2;
const N_CHAR: usize = 256 - THRESHOLD + F;
const T: usize = N_CHAR * 2 - 1;
const R: usize = T - 1;
const MAX_FREQ: u32 = 0x8000;

/// Bits most significant first; past the end there are zeros.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    len: u32,
}

impl Bits<'_> {
    fn fill(&mut self) {
        while self.len <= 8 {
            let b = self.data.get(self.pos).copied().unwrap_or(0);
            self.pos += 1;
            self.buf |= u32::from(b) << (8 - self.len);
            self.len += 8;
        }
    }

    fn bit(&mut self) -> usize {
        self.fill();
        let bit = (self.buf >> 15) & 1;
        self.buf = (self.buf << 1) & 0xFFFF;
        self.len -= 1;
        bit as usize
    }

    fn byte(&mut self) -> usize {
        self.fill();
        let byte = (self.buf >> 8) & 0xFF;
        self.buf = (self.buf << 8) & 0xFFFF;
        self.len -= 8;
        byte as usize
    }
}

/// The adaptive Huffman tree for literals and match lengths.
struct Tree {
    freq: [u32; T + 1],
    parent: [usize; T + N_CHAR],
    son: [usize; T],
}

impl Tree {
    fn new() -> Box<Self> {
        let mut t = Box::new(Tree {
            freq: [0; T + 1],
            parent: [0; T + N_CHAR],
            son: [0; T],
        });
        for i in 0..N_CHAR {
            t.freq[i] = 1;
            t.son[i] = i + T;
            t.parent[i + T] = i;
        }
        let (mut i, mut j) = (0, N_CHAR);
        while j <= R {
            t.freq[j] = t.freq[i] + t.freq[i + 1];
            t.son[j] = i;
            t.parent[i] = j;
            t.parent[i + 1] = j;
            i += 2;
            j += 1;
        }
        t.freq[T] = 0xFFFF;
        t.parent[R] = 0;
        t
    }

    fn rebuild(&mut self) {
        let mut j = 0;
        for i in 0..T {
            if self.son[i] >= T {
                self.freq[j] = self.freq[i].div_ceil(2);
                self.son[j] = self.son[i];
                j += 1;
            }
        }
        let mut i = 0;
        let mut j = N_CHAR;
        while j < T {
            let f = self.freq[i] + self.freq[i + 1];
            self.freq[j] = f;
            let mut k = j - 1;
            while f < self.freq[k] {
                k -= 1;
            }
            k += 1;
            self.freq.copy_within(k..j, k + 1);
            self.freq[k] = f;
            self.son.copy_within(k..j, k + 1);
            self.son[k] = i;
            i += 2;
            j += 1;
        }
        for i in 0..T {
            let k = self.son[i];
            self.parent[k] = i;
            if k < T {
                self.parent[k + 1] = i;
            }
        }
    }

    fn update(&mut self, symbol: usize) {
        if self.freq[R] == MAX_FREQ {
            self.rebuild();
        }
        let mut c = self.parent[symbol + T];
        loop {
            self.freq[c] += 1;
            let k = self.freq[c];
            let mut l = c + 1;
            if k > self.freq[l] {
                while k > self.freq[l + 1] {
                    l += 1;
                }
                self.freq[c] = self.freq[l];
                self.freq[l] = k;
                let i = self.son[c];
                self.parent[i] = l;
                if i < T {
                    self.parent[i + 1] = l;
                }
                let j = self.son[l];
                self.son[l] = i;
                self.parent[j] = c;
                if j < T {
                    self.parent[j + 1] = c;
                }
                self.son[c] = j;
                c = l;
            }
            c = self.parent[c];
            if c == 0 {
                break;
            }
        }
    }

    fn decode_char(&mut self, bits: &mut Bits) -> usize {
        let mut c = self.son[R];
        while c < T {
            c = self.son[c + bits.bit()];
        }
        let symbol = c - T;
        self.update(symbol);
        symbol
    }
}

/// The upper 6 bits of a match position have a fixed code: the first byte
/// says how many more bits follow.
fn decode_position(bits: &mut Bits) -> usize {
    let mut i = bits.byte();
    let (code, len) = match i {
        0x00..=0x1F => (0, 3),
        0x20..=0x2F => (1, 4),
        0x30..=0x3F => (2, 4),
        0x40..=0x4F => (3, 4),
        0x50..=0x8F => (4 + (i - 0x50) / 8, 5),
        0x90..=0xBF => (0x0C + (i - 0x90) / 4, 6),
        0xC0..=0xEF => (0x18 + (i - 0xC0) / 2, 7),
        _ => (0x30 + (i - 0xF0), 8),
    };
    for _ in 0..len - 2 {
        i = (i << 1) | bits.bit();
    }
    (code << 6) | (i & 0x3F)
}

fn lzh(data: &[u8], size: usize) -> Result<Vec<u8>, Error> {
    let mut bits = Bits {
        data,
        pos: 0,
        buf: 0,
        len: 0,
    };
    let mut tree = Tree::new();
    let mut window = [b' '; N];
    let mut r = N - F;
    let mut out = Vec::with_capacity(size);
    while out.len() < size {
        if bits.pos > data.len() + 2 {
            return Err(Error::Format("LZH data ends early"));
        }
        let c = tree.decode_char(&mut bits);
        if c < 256 {
            out.push(c as u8);
            window[r] = c as u8;
            r = (r + 1) % N;
        } else {
            let start = (r + N - decode_position(&mut bits) - 1) % N;
            for k in 0..c - 255 + THRESHOLD {
                let b = window[(start + k) % N];
                out.push(b);
                window[r] = b;
                r = (r + 1) % N;
            }
        }
    }
    out.truncate(size);
    Ok(out)
}

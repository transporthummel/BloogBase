//! The InterLeaved Bitmap (LBM) inside `KDREAMS.CMP`. Like the game, only
//! `BMHD` and `BODY` are read; the colors are the EGA ones.

use crate::{Error, Image};

pub fn decode(lbm: &[u8]) -> Result<Image, Error> {
    if lbm.len() < 12 || &lbm[..4] != b"FORM" || &lbm[8..12] != b"ILBM" {
        return Err(Error::Format("not an ILBM image"));
    }
    let mut header = None;
    let mut body = None;
    let mut at = 12;
    while at + 8 <= lbm.len() {
        let id = &lbm[at..at + 4];
        let len = u32::from_be_bytes([lbm[at + 4], lbm[at + 5], lbm[at + 6], lbm[at + 7]]) as usize;
        // Mod tools sometimes count the FORM header into the size, leaving
        // the last chunk short by 8 bytes on paper; the body check below
        // still catches pictures that really are cut off.
        let data = &lbm[at + 8..(at + 8 + len).min(lbm.len())];
        match id {
            b"BMHD" => header = Some(data),
            b"BODY" => body = Some(data),
            _ => {}
        }
        at += 8 + len + (len & 1);
    }
    let (header, body) = header
        .zip(body)
        .ok_or(Error::Format("ILBM without BMHD or BODY"))?;
    if header.len() < 11 {
        return Err(Error::Format("ILBM header too short"));
    }
    let width = usize::from(u16::from_be_bytes([header[0], header[1]]));
    let height = usize::from(u16::from_be_bytes([header[2], header[3]]));
    let planes = usize::from(header[8]);
    let compressed = header[10] == 1;
    if width == 0 || height == 0 || planes == 0 || planes > 8 || width > 1024 || height > 1024 {
        return Err(Error::Format("unsupported ILBM size"));
    }
    let row_bytes = width.div_ceil(16) * 2;
    let raw = if compressed {
        unpack_byte_run(body, row_bytes * planes * height)?
    } else {
        body.to_vec()
    };
    if raw.len() < row_bytes * planes * height {
        return Err(Error::Format("ILBM body too short"));
    }
    let mut pixels = vec![0u8; width * height];
    for y in 0..height {
        for p in 0..planes {
            let row = &raw[(y * planes + p) * row_bytes..][..row_bytes];
            for x in 0..width {
                let bit = (row[x / 8] >> (7 - x % 8)) & 1;
                pixels[y * width + x] |= bit << p;
            }
        }
    }
    Ok(Image {
        width,
        height,
        pixels,
    })
}

/// PackBits as used by ILBM ("ByteRun1").
fn unpack_byte_run(data: &[u8], len: usize) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(len);
    let mut at = 0;
    while out.len() < len {
        let n = *data.get(at).ok_or(Error::Format("ILBM body ends early"))? as i8;
        at += 1;
        match n {
            0..=127 => {
                let run = data
                    .get(at..at + n as usize + 1)
                    .ok_or(Error::Format("ILBM body ends early"))?;
                out.extend_from_slice(run);
                at += run.len();
            }
            -127..=-1 => {
                let b = *data.get(at).ok_or(Error::Format("ILBM body ends early"))?;
                at += 1;
                out.extend(std::iter::repeat_n(b, (1 - n as isize) as usize));
            }
            _ => {}
        }
    }
    Ok(out)
}

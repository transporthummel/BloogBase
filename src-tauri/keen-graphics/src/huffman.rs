//! id Software's Huffman compression, used for EGAGRAPH chunks. See
//! <https://moddingwiki.shikadi.net/wiki/Huffman_Compression>.

use crate::Error;

/// Bytes of a dictionary as stored in an executable: 255 nodes of two
/// 16-bit branches, plus four bytes of padding.
pub const DICT_LEN: usize = 1024;
const ROOT: usize = 254;

/// A decoding tree. Each branch is a byte (below 256) or a node (256 + index).
#[derive(Debug, Clone)]
pub struct Dictionary {
    nodes: [[u16; 2]; 255],
}

impl Dictionary {
    /// Reads a dictionary; fails if a branch points outside the tree.
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        if data.len() < 255 * 4 {
            return Err(Error::Format("Huffman dictionary too short"));
        }
        let mut nodes = [[0u16; 2]; 255];
        for (i, node) in nodes.iter_mut().enumerate() {
            for (b, branch) in node.iter_mut().enumerate() {
                let at = i * 4 + b * 2;
                *branch = u16::from_le_bytes([data[at], data[at + 1]]);
                if *branch >= 256 + 255 {
                    return Err(Error::Format("Huffman dictionary has a bad node"));
                }
            }
        }
        Ok(Dictionary { nodes })
    }

    /// Decodes until `len` bytes are out. Bits are read least significant
    /// first.
    pub fn decode(&self, data: &[u8], len: usize) -> Result<Vec<u8>, Error> {
        let mut out = Vec::with_capacity(len);
        let mut node = ROOT;
        if len == 0 {
            return Ok(out);
        }
        for &byte in data {
            for bit in 0..8 {
                let branch = self.nodes[node][usize::from((byte >> bit) & 1)];
                if branch < 256 {
                    out.push(branch as u8);
                    if out.len() == len {
                        return Ok(out);
                    }
                    node = ROOT;
                } else {
                    node = usize::from(branch - 256);
                }
            }
        }
        Err(Error::Format("Huffman data ends early"))
    }
}

/// Where dictionaries may start in `exe`: each one ends with the root node
/// (a leaf for byte 0 and node 253) followed by zero padding.
pub fn candidates(exe: &[u8]) -> impl Iterator<Item = usize> + '_ {
    const END: [u8; 6] = [0xFD, 0x01, 0x00, 0x00, 0x00, 0x00];
    exe.windows(END.len())
        .enumerate()
        .filter(|(_, w)| *w == END)
        .filter_map(|(at, _)| (at + END.len()).checked_sub(DICT_LEN))
}

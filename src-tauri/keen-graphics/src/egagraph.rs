//! EGAGRAPH, the graphics file of Keen 4-6 and Keen Dreams. See
//! <https://moddingwiki.shikadi.net/wiki/EGAGraph_Format>.
//!
//! The offsets (EGAHEAD) and the Huffman tree (EGADICT) are usually built
//! into the executable. Instead of keeping a table of where they sit in each
//! version, they are searched for: the header ends with the size of the
//! graphics file, and the right dictionary is the one that decodes the
//! picture table. If none does, the chunks may be stored as they are.

use crate::huffman::{self, Dictionary};
use crate::{Error, Image};

pub struct EgaGraph<'a> {
    data: &'a [u8],
    /// Start of each chunk; `None` for empty slots.
    offsets: Vec<Option<usize>>,
    /// `None` if a mod switched Huffman compression off (a patch to the
    /// executable).
    dict: Option<Dictionary>,
    /// CGAGRAPH: pictures with 2 bits per pixel instead of 4 planes.
    cga: bool,
}

impl<'a> EgaGraph<'a> {
    /// Opens `graph` with header and dictionary found in the unpacked `exe`.
    /// `offset_len` is 3 for Keen 4-6, 4 for Keen Dreams.
    pub fn from_exe(graph: &'a [u8], exe: &[u8], offset_len: usize) -> Result<Self, Error> {
        let header = find_header(exe, graph.len(), offset_len)
            .ok_or(Error::Format("EGAHEAD not found in the executable"))?;
        Self::with_dictionary_from(graph, &header, offset_len, exe)
    }

    /// Opens `graph` with a separate EGAHEAD file (as mods ship them) and the
    /// dictionary from a separate EGADICT file or the unpacked `exe`.
    pub fn from_files(
        graph: &'a [u8],
        head: &[u8],
        offset_len: usize,
        dict: Option<&[u8]>,
        exe: &[u8],
    ) -> Result<Self, Error> {
        match dict {
            Some(d) => {
                let offsets = parse_header(head, offset_len, graph.len());
                Ok(EgaGraph {
                    data: graph,
                    offsets,
                    dict: Some(Dictionary::parse(d)?),
                    cga: false,
                })
            }
            None => Self::with_dictionary_from(graph, head, offset_len, exe),
        }
    }

    fn with_dictionary_from(
        graph: &'a [u8],
        head: &[u8],
        offset_len: usize,
        exe: &[u8],
    ) -> Result<Self, Error> {
        let offsets = parse_header(head, offset_len, graph.len());
        let dicts = huffman::candidates(exe)
            .filter_map(|at| Dictionary::parse(&exe[at..]).ok())
            .map(Some)
            .chain([None]);
        for dict in dicts {
            let candidate = EgaGraph {
                data: graph,
                offsets: offsets.clone(),
                dict,
                cga: false,
            };
            if candidate.pictures().is_ok_and(|p| !p.is_empty()) {
                return Ok(candidate);
            }
        }
        Err(Error::Format("EGADICT not found in the executable"))
    }

    /// Reads the file as CGAGRAPH, the CGA releases' graphics.
    pub fn cga(self) -> Self {
        EgaGraph { cga: true, ..self }
    }

    /// The decompressed chunk `n`. Only for chunks with a size prefix (all
    /// but the 16x16 and 32x32 tiles).
    pub fn chunk(&self, n: usize) -> Result<Vec<u8>, Error> {
        let start = self
            .offsets
            .get(n)
            .copied()
            .flatten()
            .ok_or(Error::Format("empty or missing chunk"))?;
        // Without compression the game reads just the stated size, so the
        // next chunk may start anywhere, as in Skater Squirrel.
        let end = match self.dict {
            Some(_) => self.offsets[n + 1..].iter().flatten().next().copied(),
            None => None,
        }
        .unwrap_or(self.data.len());
        let raw = self
            .data
            .get(start..end)
            .filter(|r| r.len() >= 4)
            .ok_or(Error::Format("chunk outside the graphics file"))?;
        let len = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
        if len > 1 << 20 {
            return Err(Error::Format("chunk implausibly large"));
        }
        match &self.dict {
            // Uncompressed chunks that hold exactly the stated size pass with
            // any dictionary.
            Some(_) if raw.len() - 4 == len => Ok(raw[4..].to_vec()),
            Some(dict) => dict.decode(&raw[4..], len),
            None => raw
                .get(4..4 + len)
                .map(<[u8]>::to_vec)
                .ok_or(Error::Format("chunk outside the graphics file")),
        }
    }

    /// Width in bytes (8 pixels each) and height of every picture (chunk 0).
    pub fn pictures(&self) -> Result<Vec<(usize, usize)>, Error> {
        let table = self.chunk(0)?;
        if table.is_empty() || table.len() % 4 != 0 {
            return Err(Error::Format("bad picture table"));
        }
        let pics: Vec<(usize, usize)> = table
            .as_chunks::<4>()
            .0
            .iter()
            .map(|e| {
                let w = u16::from_le_bytes([e[0], e[1]]) as usize;
                let h = u16::from_le_bytes([e[2], e[3]]) as usize;
                (w, h)
            })
            .collect();
        if pics
            .iter()
            .any(|&(w, h)| w == 0 || w > 80 || h == 0 || h > 480)
        {
            return Err(Error::Format("bad picture table"));
        }
        Ok(pics)
    }

    /// Picture `index` of the picture table.
    pub fn picture(&self, index: usize) -> Result<Image, Error> {
        let pics = self.pictures()?;
        let (w, h) = *pics.get(index).ok_or(Error::Format("no such picture"))?;
        // Pictures follow the fonts, whose number differs per game: find the
        // first chunk that has the size of picture 0.
        let (w0, h0) = pics[0];
        let first = (3..16)
            .find(|&c| {
                self.chunk(c)
                    .is_ok_and(|d| d.len() == w0 * h0 * self.planes())
            })
            .ok_or(Error::Format("pictures not found"))?;
        let data = self.chunk(first + index)?;
        if self.cga {
            Image::from_cga(&data, w * 4, h)
        } else {
            Image::from_planar(&data, w * 8, h)
        }
    }

    /// Picture widths count bytes: 8 pixels in each of 4 EGA planes, or 4
    /// CGA pixels.
    fn planes(&self) -> usize {
        if self.cga { 1 } else { 4 }
    }

    /// The width of a 320-pixel picture.
    fn screen_width(&self) -> usize {
        if self.cga { 80 } else { 40 }
    }

    /// The title screen: the known title picture of Keen 4-6, else the
    /// full-screen (320x200) picture with the most colors. In the original
    /// games that's the only one using all 16 (in CGA all 4); the others are the menu frame,
    /// the story backdrop and, in Keen 5, the ending's galaxy.
    pub fn title(&self) -> Result<Image, Error> {
        // Mods can change pictures but not their numbers, so the original
        // title picture's number is the surest guide. Its game is told apart
        // by the number of pictures.
        let known = match (self.cga, self.pictures()?.len()) {
            (false, 107) => Some(95),  // Keen 4
            (false, 115) => Some(103), // Keen 4 shareware
            (false, 93) => Some(82),   // Keen 5
            (false, 37) => Some(28),   // Keen 6
            (true, 108) => Some(96),   // Keen 4 1.0-1.1
            (true, 116) => Some(104),  // Keen 4 1.4
            (true, 94) => Some(83),    // Keen 5
            (true, 34) => Some(25),    // Keen 6
            _ => None,
        };
        if let Some(index) = known
            && self.pictures()?[index] == (self.screen_width(), 200)
        {
            return self.picture(index);
        }
        let mut best: Option<(usize, Image)> = None;
        for index in self.full_screen()? {
            let image = self.picture(index)?;
            let colors = image.color_count();
            if best.as_ref().is_none_or(|(c, _)| colors >= *c) {
                best = Some((colors, image));
            }
        }
        best.map(|(_, image)| image)
            .ok_or(Error::Format("no full-screen picture"))
    }

    /// Indexes of all full-screen (320x200) pictures.
    pub fn full_screen(&self) -> Result<Vec<usize>, Error> {
        Ok(self
            .pictures()?
            .iter()
            .enumerate()
            .filter(|(_, p)| **p == (self.screen_width(), 200))
            .map(|(i, _)| i)
            .collect())
    }
}

fn read_offset(entry: &[u8]) -> usize {
    entry
        .iter()
        .rev()
        .fold(0, |acc, &b| (acc << 8) | usize::from(b))
}

fn parse_header(head: &[u8], offset_len: usize, graph_len: usize) -> Vec<Option<usize>> {
    let empty = (1usize << (8 * offset_len)) - 1;
    head.chunks_exact(offset_len)
        .map(read_offset)
        .map(|o| (o != empty && o < graph_len).then_some(o))
        .collect()
}

/// The EGAHEAD inside `exe`: it ends with the size of the graphics file and
/// starts with offset 0, with growing offsets (or empty slots) in between.
fn find_header(exe: &[u8], graph_len: usize, offset_len: usize) -> Option<Vec<u8>> {
    let needle = &(graph_len as u32).to_le_bytes()[..offset_len];
    let empty = (1usize << (8 * offset_len)) - 1;
    for end in exe
        .windows(offset_len)
        .enumerate()
        .filter(|(_, w)| *w == needle)
        .map(|(at, _)| at)
    {
        let mut at = end;
        let mut next = graph_len;
        while at >= offset_len {
            at -= offset_len;
            let o = read_offset(&exe[at..at + offset_len]);
            if o == empty {
                continue;
            }
            if o > next {
                break;
            }
            if o == 0 {
                return Some(exe[at..end + offset_len].to_vec());
            }
            next = o;
        }
    }
    None
}

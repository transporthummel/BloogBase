//! Extracts the title screen of a Commander Keen game from the player's own
//! game files, to use as its cover (PLAN.md §15). Nothing is shipped: the
//! pictures are made locally from files the player has.
//!
//! Defensive like `keen-saves`: files come from any version or mod, so a
//! file that doesn't match is an error, never a panic.
//!
//! - Keen 1-3: the `TITLE` bitmap over the title level ([`vorticons`]).
//! - Keen 4-6: the most colorful 320x200 picture in EGAGRAPH ([`egagraph`]).
//! - Keen Dreams 1.13: `TITLESCR` in the Softdisk library `KDREAMS.CMP`
//!   ([`slib`], [`ilbm`]).

use std::fmt;

pub mod egagraph;
pub mod huffman;
pub mod ilbm;
pub mod lzexe;
pub mod pklite;
pub mod slib;
pub mod vorticons;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Format(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Format(what) => f.write_str(what),
        }
    }
}

impl std::error::Error for Error {}

/// A 16-color EGA picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    /// One EGA color index (0-15) per pixel, row by row.
    pub pixels: Vec<u8>,
}

/// The default EGA palette.
pub const EGA_PALETTE: [[u8; 3]; 16] = [
    [0x00, 0x00, 0x00],
    [0x00, 0x00, 0xAA],
    [0x00, 0xAA, 0x00],
    [0x00, 0xAA, 0xAA],
    [0xAA, 0x00, 0x00],
    [0xAA, 0x00, 0xAA],
    [0xAA, 0x55, 0x00],
    [0xAA, 0xAA, 0xAA],
    [0x55, 0x55, 0x55],
    [0x55, 0x55, 0xFF],
    [0x55, 0xFF, 0x55],
    [0x55, 0xFF, 0xFF],
    [0xFF, 0x55, 0x55],
    [0xFF, 0x55, 0xFF],
    [0xFF, 0xFF, 0x55],
    [0xFF, 0xFF, 0xFF],
];

impl Image {
    /// From EGA planar data: all of plane 0 (blue), then green, red and
    /// intensity, each `width / 8 * height` bytes.
    pub fn from_planar(data: &[u8], width: usize, height: usize) -> Result<Self, Error> {
        let plane = width / 8 * height;
        if data.len() < plane * 4 {
            return Err(Error::Format("picture data too short"));
        }
        let mut pixels = vec![0u8; width * height];
        for (i, px) in pixels.iter_mut().enumerate() {
            let byte = i / 8;
            let shift = 7 - i % 8;
            for p in 0..4 {
                *px |= ((data[p * plane + byte] >> shift) & 1) << p;
            }
        }
        Ok(Image {
            width,
            height,
            pixels,
        })
    }

    /// From CGA data: 4 pixels per byte, leftmost in the top bits, in the
    /// palette the games use (black, light cyan, light magenta, white).
    pub fn from_cga(data: &[u8], width: usize, height: usize) -> Result<Self, Error> {
        const CGA: [u8; 4] = [0, 11, 13, 15];
        if data.len() < width / 4 * height {
            return Err(Error::Format("picture data too short"));
        }
        let pixels = (0..width * height)
            .map(|i| CGA[usize::from((data[i / 4] >> (6 - 2 * (i % 4))) & 3)])
            .collect();
        Ok(Image {
            width,
            height,
            pixels,
        })
    }

    /// How many of the 16 colors the picture uses.
    pub fn color_count(&self) -> usize {
        let mut used = [false; 16];
        for &c in &self.pixels {
            used[usize::from(c & 15)] = true;
        }
        used.iter().filter(|&&u| u).count()
    }

    /// 8-bit RGB, row by row.
    pub fn to_rgb(&self) -> Vec<u8> {
        self.to_rgb_with(&EGA_PALETTE)
    }

    /// 8-bit RGB in another palette, like one a mod sets.
    pub fn to_rgb_with(&self, palette: &[[u8; 3]; 16]) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|&c| palette[usize::from(c & 15)])
            .collect()
    }
}

/// The title screen of Keen 1-3 from `EGAHEAD`, `EGALATCH` and `LEVEL90`.
/// Mods with the tileset loader patch name a `tileset` file for the level
/// in `TILEHEAD` (see [`vorticons_tileset_name`]).
pub fn vorticons_title(
    egahead: &[u8],
    egalatch: &[u8],
    level90: &[u8],
    tileset: Option<&[u8]>,
) -> Result<Image, Error> {
    vorticons::title(egahead, egalatch, level90, tileset)
}

/// The tileset file a mod's `TILEHEAD` names for `level`.
pub fn vorticons_tileset_name(tilehead: &[u8], level: usize) -> Option<String> {
    vorticons::tileset_name(tilehead, level)
}

/// The title screen of Keen 4-6 from the game's executable and EGAGRAPH.
/// Mods often bring their own EGAHEAD and EGADICT files; whatever is
/// missing is taken from the executable.
pub fn galaxy_title(
    exe: &[u8],
    egagraph: &[u8],
    egahead: Option<&[u8]>,
    egadict: Option<&[u8]>,
) -> Result<Image, Error> {
    let image = load_image(exe)?;
    let graph = match egahead {
        Some(head) => egagraph::EgaGraph::from_files(egagraph, head, 3, egadict, &image)?,
        // The Keen 4 special demo still has 4-byte offsets, like Keen Dreams.
        None => egagraph::EgaGraph::from_exe(egagraph, &image, 3)
            .or_else(|_| egagraph::EgaGraph::from_exe(egagraph, &image, 4))?,
    };
    graph.title()
}

/// The title screen of a CGA release of Keen 4-6 from its executable and
/// CGAGRAPH.
pub fn galaxy_cga_title(exe: &[u8], cgagraph: &[u8]) -> Result<Image, Error> {
    egagraph::EgaGraph::from_exe(cgagraph, &load_image(exe)?, 3)?
        .cga()
        .title()
}

/// The title screen of Keen Dreams 1.13 from `KDREAMS.CMP`.
pub fn dreams_title(cmp: &[u8]) -> Result<Image, Error> {
    ilbm::decode(&slib::entry(cmp, "TITLESCR.LBM")?)
}

/// The load image of a DOS executable, unpacked if LZEXE or PKLITE packed it.
pub fn load_image(exe: &[u8]) -> Result<Vec<u8>, Error> {
    if pklite::is_pklite(exe) {
        pklite::load_image(exe)
    } else {
        lzexe::load_image(exe)
    }
}

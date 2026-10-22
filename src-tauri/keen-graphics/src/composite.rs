//! CGA pictures as a composite monitor shows them in 640x200 graphics with
//! the color burst on: four pixels make one cycle of the color carrier, so
//! each run of four becomes one of 16 artifact colors.

use crate::Image;

/// The artifact color of four pixels by where they fall in the carrier
/// cycle: bit 3 for the pixel at phase 0 (x divisible by 4) down to bit 0.
/// Measured from DOSBox's early-era composite emulation, in a screenshot of
/// Keen 4 Composite CGA Edition (KeenWiki); solid areas of each pattern
/// vary by about 2 of 255.
const COLORS: [[u8; 3]; 16] = [
    [0x00, 0x00, 0x00],
    [0x00, 0x61, 0x16],
    [0x24, 0x26, 0xB7],
    [0x0E, 0x97, 0xDA],
    [0x7D, 0x0B, 0x4C],
    [0x6F, 0x6F, 0x6E],
    [0xAC, 0x36, 0xFB],
    [0x97, 0xA7, 0xFB],
    [0x47, 0x48, 0x00],
    [0x3D, 0xB5, 0x00],
    [0x70, 0x70, 0x72],
    [0x5F, 0xEB, 0x81],
    [0xDB, 0x56, 0x12],
    [0xD0, 0xC5, 0x1B],
    [0xFB, 0x81, 0xE8],
    [0xFD, 0xFD, 0xFD],
];

/// The CGA colors [`Image::from_cga`] gives each two-bit value.
const CGA: [u8; 4] = [0, 11, 13, 15];

/// 8-bit RGB of a CGA picture, at its own size: its two-bit pixels are the
/// 640 one-bit pixels of each line. Each of those gets the mean color of the
/// five four-pixel runs around it (closest to DOSBox's filtering), and each
/// two of them make one pixel.
pub fn to_rgb(image: &Image) -> Vec<u8> {
    let wide = image.width * 2;
    let mut rgb = Vec::with_capacity(image.width * image.height * 3);
    let mut line = vec![[0u32; 3]; wide];
    for row in image.pixels.chunks(image.width) {
        let bit = |x: isize| -> usize {
            usize::try_from(x)
                .ok()
                .filter(|&x| x < wide)
                .map_or(0, |x| {
                    let value = CGA.iter().position(|&c| c == row[x / 2]).unwrap_or(0);
                    (value >> (1 - x % 2)) & 1
                })
        };
        let color = |start: isize| {
            let pattern = (start..start + 4).fold(0, |n, x| n | bit(x) << (3 - x.rem_euclid(4)));
            COLORS[pattern]
        };
        for (x, sum) in line.iter_mut().enumerate() {
            *sum = [0; 3];
            for start in x as isize - 3..=x as isize + 1 {
                for (s, c) in sum.iter_mut().zip(color(start)) {
                    *s += u32::from(c);
                }
            }
        }
        for pair in line.chunks(2) {
            // Five runs for each of the two.
            rgb.extend(
                pair[0]
                    .iter()
                    .zip(pair[1])
                    .map(|(a, b)| ((a + b + 5) / 10) as u8),
            );
        }
    }
    rgb
}

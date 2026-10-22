//! Reads what a Commander Keen saved game says about itself (PLAN.md §11).
//!
//! Defensive by design: saves come from any game version or mod, so every
//! read is bounds-checked and an unexpected file is an error, never a panic.
//!
//! Formats:
//! - Keen 1-3: a fixed 92-byte record, see
//!   <https://moddingwiki.shikadi.net/wiki/Keen_1-3_Saved_game_format>.
//! - Keen 4-6 and Keen Dreams: id's `SaveGame` header (signature, the name
//!   the player typed) followed by the episode's `gametype` struct, word
//!   aligned. Offsets checked against real saves of Keen 4 v1.4, Keen 6 v1.0
//!   and Keen Dreams 1.13; Keen 5 shares Keen 4's code with three
//!   episode-specific words.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Episode {
    Keen1,
    Keen2,
    Keen3,
    Keen4,
    Keen5,
    Keen6,
    Dreams,
}

impl Episode {
    /// From a BloogBase game id such as `keen4` or `kdreams`.
    pub fn from_game_id(id: &str) -> Option<Self> {
        Some(match id {
            "keen1" => Episode::Keen1,
            "keen2" => Episode::Keen2,
            "keen3" => Episode::Keen3,
            "keen4" => Episode::Keen4,
            "keen5" => Episode::Keen5,
            "keen6" => Episode::Keen6,
            "kdreams" => Episode::Dreams,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
}

/// Everything known about one save. Fields a format doesn't have are `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveInfo {
    /// What the player typed when saving (Keen 4-6, Keen Dreams).
    pub name: Option<String>,
    pub score: Option<i32>,
    pub lives: Option<i16>,
    /// Raygun or neural stunner shots (Keen Dreams: flower power seeds).
    pub ammo: Option<i16>,
    pub levels_done: Option<u16>,
    pub difficulty: Option<Difficulty>,
    /// Saved on the world map rather than inside a level.
    pub on_world_map: Option<bool>,
    /// Keen 1 only: how many of the four ship parts were found.
    pub ship_parts: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    TooShort { needed: usize, got: usize },
    BadSignature,
    WrongSize { expected: usize, got: usize },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::TooShort { needed, got } => {
                write!(f, "too short: {got} bytes, need {needed}")
            }
            ParseError::BadSignature => f.write_str("not a saved game of this episode"),
            ParseError::WrongSize { expected, got } => {
                write!(f, "unexpected size: {got} bytes, expected {expected}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse(episode: Episode, data: &[u8]) -> Result<SaveInfo, ParseError> {
    match episode {
        Episode::Keen1 | Episode::Keen2 | Episode::Keen3 => vorticons(episode, data),
        Episode::Keen4 => galaxy(b"CK4\0", 2, data),
        Episode::Keen5 => galaxy(b"CK5\0", 3, data),
        Episode::Keen6 => galaxy(b"CK6\0", 4, data),
        Episode::Dreams => dreams(data),
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn bytes(&self, at: usize, len: usize) -> Result<&[u8], ParseError> {
        self.0.get(at..at + len).ok_or(ParseError::TooShort {
            needed: at + len,
            got: self.0.len(),
        })
    }

    fn u16(&self, at: usize) -> Result<u16, ParseError> {
        let b = self.bytes(at, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn i16(&self, at: usize) -> Result<i16, ParseError> {
        Ok(self.u16(at)? as i16)
    }

    fn i32(&self, at: usize) -> Result<i32, ParseError> {
        let b = self.bytes(at, 4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Booleans stored as 16-bit words.
    fn count_words(&self, at: usize, n: usize) -> Result<u16, ParseError> {
        let mut done = 0;
        for i in 0..n {
            if self.u16(at + 2 * i)? != 0 {
                done += 1;
            }
        }
        Ok(done)
    }

    /// A NUL-terminated name in a fixed field; DOS code page bytes beyond
    /// ASCII are shown as replacement characters.
    fn name(&self, at: usize, len: usize) -> Result<Option<String>, ParseError> {
        let field = self.bytes(at, len)?;
        let end = field.iter().position(|&b| b == 0).unwrap_or(len);
        let name: String = field[..end]
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '\u{FFFD}'
                }
            })
            .collect();
        let name = name.trim();
        Ok((!name.is_empty()).then(|| name.to_string()))
    }
}

const VORTICONS_SIZE: usize = 92;

fn vorticons(episode: Episode, data: &[u8]) -> Result<SaveInfo, ParseError> {
    if data.len() != VORTICONS_SIZE {
        return Err(ParseError::WrongSize {
            expected: VORTICONS_SIZE,
            got: data.len(),
        });
    }
    let r = Reader(data);
    // 16 words, or 32 bytes with the 32-level patch: count non-zero bytes
    // works for both, since a finished level is stored as 1.
    let levels_done = r.bytes(18, 32)?.iter().filter(|&&b| b != 0).count() as u16;
    let ship_parts = (episode == Episode::Keen1).then(|| {
        // Battery, vacuum, joystick and whiskey; the pogo stick at 6 isn't a part.
        [0, 2, 4, 8]
            .iter()
            .filter(|&&at| r.u16(at).is_ok_and(|v| v != 0))
            .count() as u8
    });
    Ok(SaveInfo {
        lives: Some(r.i16(50)?),
        ammo: Some(r.i16(52)?),
        score: Some(r.i32(54)?),
        levels_done: Some(levels_done),
        // Keen 1-3 only save on the world map.
        on_world_map: Some(true),
        ship_parts,
        ..SaveInfo::default()
    })
}

const NAME_LEN: usize = 33;
const GALAXY_LEVELS: usize = 25;
const DREAMS_LEVELS: usize = 17;

fn difficulty(word: u16) -> Option<Difficulty> {
    match word {
        1 => Some(Difficulty::Easy),
        2 => Some(Difficulty::Normal),
        3 => Some(Difficulty::Hard),
        _ => None,
    }
}

/// `extra_words`: the episode's own items between `drops` and `keys`
/// (Keen 4: wetsuit, rescued; Keen 5: keycard, destroyed, fuses; Keen 6:
/// sandwich, hook, passcard, rocket).
fn galaxy(signature: &[u8; 4], extra_words: usize, data: &[u8]) -> Result<SaveInfo, ParseError> {
    let r = Reader(data);
    if r.bytes(0, 4)? != signature {
        return Err(ParseError::BadSignature);
    }
    // signature, oldtest pointer, present flag, then the name; padded to a word.
    let name = r.name(8, NAME_LEN)?;
    let state = (8 + NAME_LEN).next_multiple_of(2);
    let levels = state + 4; // after worldx, worldy
    let score = levels + 2 * GALAXY_LEVELS;
    let ammo = score + 8; // after score, nextextra
    let keys = ammo + 4 + 2 * extra_words; // after ammo, drops, extras
    let mapon = keys + 8;
    Ok(SaveInfo {
        name,
        score: Some(r.i32(score)?),
        ammo: Some(r.i16(ammo)?),
        levels_done: Some(r.count_words(levels, GALAXY_LEVELS)?),
        on_world_map: Some(r.u16(mapon)? == 0),
        lives: Some(r.i16(mapon + 2)?),
        difficulty: difficulty(r.u16(mapon + 4)?),
        ship_parts: None,
    })
}

fn dreams(data: &[u8]) -> Result<SaveInfo, ParseError> {
    let r = Reader(data);
    if r.bytes(0, 4)? != b"KDR\0" {
        return Err(ParseError::BadSignature);
    }
    // signature, present flag, then the name; padded to a word.
    let name = r.name(6, NAME_LEN)?;
    let state = (6 + NAME_LEN).next_multiple_of(2);
    let levels = state + 4;
    let score = levels + 2 * DREAMS_LEVELS;
    let flower_power = score + 8;
    // flowerpowers, boobusbombs, bombsthislevel, keys
    let mapon = flower_power + 8;
    Ok(SaveInfo {
        name,
        score: Some(r.i32(score)?),
        ammo: Some(r.i16(flower_power)?),
        // Slot 0 is the world map, which the game marks done on entering a level.
        levels_done: Some(r.count_words(levels + 2, DREAMS_LEVELS - 1)?),
        on_world_map: Some(r.u16(mapon)? == 0),
        lives: Some(r.i16(mapon + 2)?),
        // Keen Dreams saves Normal as 0.
        difficulty: match r.u16(mapon + 4)? {
            0 => Some(Difficulty::Normal),
            word => difficulty(word),
        },
        ship_parts: None,
    })
}

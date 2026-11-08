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
//! - Foray in the Forest 2.0, a Keen 4 source mod: the same header with its
//!   file extension as signature (`CKS`, `CKC` for the classic build) and a
//!   `gametype` of its own, from its source (`CK_DEF.H`). Checked against a
//!   real save of v2.0a.
//! - Atroxian Realm 1.4, a source mod on the Keen Dreams source played as
//!   Keen 4: the header with `CKD`, its file extension, and Keen 5's game
//!   state with 26 levels, from its source (`KD_DEF.H`). Checked against
//!   real saves of v1.4.
//! - Keen Dreams Gold Edition, a source mod on the Keen Dreams source played
//!   as Keen 4: the header with `KDG` and a byte-packed game state of its
//!   own, from its source (`KDG_DEF.H`). Checked against real saves of the
//!   2026-05-31 release.
//! - Return to the Shadowlands 1.8, a Keen 4 source mod: the header with
//!   `CK4`, but the game state packed without the padding byte and with
//!   three more item words. Checked against real saves of v1.8.

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
    /// Foray in the Forest's fourth level.
    Expert,
    /// Keen Dreams Gold Edition's fourth level.
    Keen,
    /// Keen Dreams Gold Edition's fifth, played as Mortimer.
    Grand,
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
    TooShort {
        needed: usize,
        got: usize,
    },
    BadSignature,
    WrongSize {
        expected: usize,
        got: usize,
    },
    /// Values the game can't have written, e.g. from another layout.
    Implausible(&'static str),
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
            ParseError::Implausible(what) => write!(f, "implausible {what}"),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse(episode: Episode, data: &[u8]) -> Result<SaveInfo, ParseError> {
    match episode {
        Episode::Keen1 | Episode::Keen2 | Episode::Keen3 => vorticons(episode, data),
        Episode::Keen4 => match data.get(..4) {
            Some(b"CKS\0" | b"CKC\0") => foray(data),
            Some(b"CKD\0") => atroxian(data),
            Some(b"KDG\0") => gold_edition(data),
            Some(b"CK4\0") if !galaxy_levels_plausible(data) => {
                shadowlands(data).or_else(|_| galaxy(b"CK4\0", 2, data))
            }
            _ => galaxy(b"CK4\0", 2, data),
        },
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

/// Keen 4-6 store each level as 0 or 1. A game state packed without the
/// padding byte after the name reads as words like 0x0100 here.
fn galaxy_levels_plausible(data: &[u8]) -> bool {
    let r = Reader(data);
    let levels = (8 + NAME_LEN).next_multiple_of(2) + 4;
    (0..GALAXY_LEVELS).all(|i| r.u16(levels + 2 * i).is_ok_and(|v| v <= 1))
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

const FORAY_LEVELS: usize = 27;

/// Foray in the Forest 2.0: after `score` come `nextextra` and `oldscore`,
/// then `ammo`, `drops`, three words of its own, `mapon`, `lives` and
/// `difficulty` (4 is Expert). Map 0 is the world map and map 1 the opening
/// rocket, which the game marks done before the first level, so neither
/// counts. Its saves have no version of their own; values the game can't
/// have written are refused rather than shown.
fn foray(data: &[u8]) -> Result<SaveInfo, ParseError> {
    let r = Reader(data);
    let name = r.name(8, NAME_LEN)?;
    let state = (8 + NAME_LEN).next_multiple_of(2);
    let levels = state + 4;
    let score = levels + 2 * FORAY_LEVELS;
    let ammo = score + 12;
    let mapon = ammo + 10;
    let (map, lives) = (r.u16(mapon)?, r.i16(mapon + 2)?);
    let difficulty = match r.u16(mapon + 4)? {
        4 => Difficulty::Expert,
        word => self::difficulty(word).ok_or(ParseError::Implausible("difficulty"))?,
    };
    if map as usize >= FORAY_LEVELS {
        return Err(ParseError::Implausible("map"));
    }
    if !(0..=99).contains(&lives) {
        return Err(ParseError::Implausible("lives"));
    }
    // Done and perfect are flags 1 and 2.
    for i in 0..FORAY_LEVELS {
        if r.u16(levels + 2 * i)? > 3 {
            return Err(ParseError::Implausible("levels"));
        }
    }
    Ok(SaveInfo {
        name,
        score: Some(r.i32(score)?),
        ammo: Some(r.i16(ammo)?),
        levels_done: Some(r.count_words(levels + 4, FORAY_LEVELS - 2)?),
        on_world_map: Some(map == 0),
        lives: Some(lives),
        difficulty: Some(difficulty),
        ship_parts: None,
    })
}

const GOLD_LEVELS: usize = 47;

/// Keen Dreams Gold Edition, built on the Keen Dreams source: the header
/// with `KDG`, then `gametype` from its `KDG_DEF.H`: world position, score,
/// `nextextra`, a done byte for each of 47 maps, then bytes for damage time,
/// flower powers, ammo, bombs, bombs this level, keys, keycard, keycard
/// text, `mapon`, `lives` and `difficulty` (Easy 1 to Hard 3, then its own
/// Keen 4 and Grand 5). Maps 21 and 22 are where a new game starts and don't
/// count, as in its own count. The score box shows the ammo on maps 23-46
/// and the flower powers elsewhere. Values the game can't have written are
/// refused. Checked against real saves of the 2026-05-31 release.
fn gold_edition(data: &[u8]) -> Result<SaveInfo, ParseError> {
    let r = Reader(data);
    let name = r.name(8, NAME_LEN)?;
    let state = (8 + NAME_LEN).next_multiple_of(2);
    let score = state + 4;
    let levels = score + 8;
    let items = r.bytes(levels + GOLD_LEVELS, 11)?;
    let (flower_powers, ammo, map) = (items[1], items[2], items[8]);
    let lives = items[9] as i8;
    let difficulty = match items[10] {
        word @ 1..=3 => difficulty(word.into()),
        4 => Some(Difficulty::Keen),
        5 => Some(Difficulty::Grand),
        _ => return Err(ParseError::Implausible("difficulty")),
    };
    if map as usize > GOLD_LEVELS {
        return Err(ParseError::Implausible("map"));
    }
    if !(0..=99).contains(&lives) {
        return Err(ParseError::Implausible("lives"));
    }
    let done = r.bytes(levels, GOLD_LEVELS)?;
    if done.iter().any(|&d| d > 1) {
        return Err(ParseError::Implausible("levels"));
    }
    let score = u32::from_le_bytes(r.bytes(score, 4)?.try_into().unwrap());
    Ok(SaveInfo {
        name,
        score: Some(score.min(i32::MAX as u32) as i32),
        lives: Some(lives.into()),
        ammo: Some(
            if (23..=46).contains(&map) {
                ammo
            } else {
                flower_powers
            }
            .into(),
        ),
        levels_done: Some(
            done.iter()
                .enumerate()
                .filter(|&(i, &d)| d == 1 && i != 21 && i != 22)
                .count() as u16,
        ),
        difficulty,
        on_world_map: None,
        ship_parts: None,
    })
}

const ATROXIAN_LEVELS: usize = 26;

/// Atroxian Realm 1.4, built on the Keen Dreams source with Keen 5's game
/// state (`CK_GameState` in its `KD_DEF.H`): a level word for each of its 26
/// maps, then `keenScore`, `nextKeenAt`, `numShots`, vitalin, security card,
/// vines, fuses, four key gems, `currentLevel`, `numLives` and `difficulty`.
/// A new game starts on map 25, the crash site, which is marked done after
/// it, so it doesn't count. Older versions saved in another layout; values
/// the game can't have written are refused.
fn atroxian(data: &[u8]) -> Result<SaveInfo, ParseError> {
    let r = Reader(data);
    let name = r.name(8, NAME_LEN)?;
    let state = (8 + NAME_LEN).next_multiple_of(2);
    let levels = state + 4;
    let score = levels + 2 * ATROXIAN_LEVELS;
    let ammo = score + 8;
    let mapon = ammo + 18;
    let (map, lives) = (r.u16(mapon)?, r.i16(mapon + 2)?);
    let difficulty = difficulty(r.u16(mapon + 4)?).ok_or(ParseError::Implausible("difficulty"))?;
    if map as usize >= ATROXIAN_LEVELS {
        return Err(ParseError::Implausible("map"));
    }
    if !(0..=99).contains(&lives) {
        return Err(ParseError::Implausible("lives"));
    }
    for i in 0..ATROXIAN_LEVELS {
        if r.u16(levels + 2 * i)? > 1 {
            return Err(ParseError::Implausible("levels"));
        }
    }
    Ok(SaveInfo {
        name,
        score: Some(r.i32(score)?),
        ammo: Some(r.i16(ammo)?),
        levels_done: Some(r.count_words(levels, ATROXIAN_LEVELS - 1)?),
        on_world_map: Some(map == 0),
        lives: Some(lives),
        difficulty: Some(difficulty),
        ship_parts: None,
    })
}

/// Return to the Shadowlands 1.8 saves as `CK4`, but its game state follows
/// the name without a padding byte: world position, 25 level words, score,
/// `nextKeenAt`, ten words (the third is the ammo, the rest its items), map,
/// lives and difficulty. Map 20, the rocket the game starts on, is marked
/// done from the start and map 19 holds the high scores, so only maps 0-18
/// count. Values the game can't have written are refused.
fn shadowlands(data: &[u8]) -> Result<SaveInfo, ParseError> {
    let r = Reader(data);
    let name = r.name(8, NAME_LEN)?;
    let state = 8 + NAME_LEN;
    let levels = state + 4;
    let score = levels + 2 * GALAXY_LEVELS;
    let ammo = score + 8 + 4;
    let mapon = score + 8 + 2 * 10;
    let (map, lives) = (r.u16(mapon)?, r.i16(mapon + 2)?);
    let difficulty = difficulty(r.u16(mapon + 4)?).ok_or(ParseError::Implausible("difficulty"))?;
    if map > 20 {
        return Err(ParseError::Implausible("map"));
    }
    if !(0..=99).contains(&lives) {
        return Err(ParseError::Implausible("lives"));
    }
    for i in 0..GALAXY_LEVELS {
        if r.u16(levels + 2 * i)? > 1 {
            return Err(ParseError::Implausible("levels"));
        }
    }
    Ok(SaveInfo {
        name,
        score: Some(r.i32(score)?),
        ammo: Some(r.i16(ammo)?),
        levels_done: Some(r.count_words(levels, 19)?),
        on_world_map: Some(map == 0),
        lives: Some(lives),
        difficulty: Some(difficulty),
        ship_parts: None,
    })
}

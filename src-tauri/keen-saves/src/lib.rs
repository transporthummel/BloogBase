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
//! - Source mods (Foray in the Forest, Atroxian Realm, Return to the
//!   Shadowlands, Keen Dreams Gold Edition): the same header, mostly with a
//!   signature of their own, and a `gametype` of their own from their
//!   source. Each is a [`Layout`] in [`SOURCE_MODS`], read by one reader;
//!   unlike the games' own saves, values the game can't have written are
//!   refused.

use std::fmt;

use serde::Serialize;
use specta::Type;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
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
        Episode::Keen4 => galaxy_or_source_mod(episode, b"CK4\0", 2, data),
        Episode::Keen5 => galaxy_or_source_mod(episode, b"CK5\0", 3, data),
        Episode::Keen6 => galaxy_or_source_mod(episode, b"CK6\0", 4, data),
        Episode::Dreams => dreams(data),
    }
}

/// A save of the episode itself, or of a source mod played as it. A source
/// mod may save under the episode's own signature (Return to the
/// Shadowlands); then the episode's layout comes first where its levels
/// read right, and is the last resort where no source mod's checks pass.
fn galaxy_or_source_mod(
    episode: Episode,
    signature: &[u8; 4],
    extra_words: usize,
    data: &[u8],
) -> Result<SaveInfo, ParseError> {
    let own = data.starts_with(signature);
    if !(own && galaxy_levels_plausible(data)) {
        match source_mod(episode, data) {
            Some(Ok(info)) => return Ok(info),
            Some(Err(e)) if !own => return Err(e),
            _ => {}
        }
    }
    galaxy(signature, extra_words, data)
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

/// How a field of a source mod's game state is stored.
#[derive(Clone, Copy)]
enum Width {
    Byte,
    Word,
}

impl Width {
    fn bytes(self) -> usize {
        match self {
            Width::Byte => 1,
            Width::Word => 2,
        }
    }
}

/// Where the score box takes the ammo from.
enum Ammo {
    At(usize),
    /// `at` on `maps`, `otherwise` elsewhere.
    ByMap {
        maps: std::ops::RangeInclusive<u16>,
        at: usize,
        otherwise: usize,
    },
}

/// Where a source mod keeps what a save says, from the `gametype` of its
/// source. Offsets count from the start of the game state, after the header
/// of id's `SaveGame` (signature, two words, the 33-byte name). Its saves
/// have no version of their own, so values the game can't have written are
/// refused rather than shown.
struct Layout {
    episode: Episode,
    signatures: &'static [&'static [u8; 4]],
    /// The game state starts on a word after the name, as in id's games;
    /// some mods pack it right after the name.
    padded: bool,
    levels_at: usize,
    levels: usize,
    level_width: Width,
    /// The highest flag a level can have: done is 1, Foray's perfect 2.
    level_max: u16,
    /// Maps that don't count as levels done: where a new game starts, the
    /// high scores.
    uncounted: &'static [usize],
    /// A 32-bit score.
    score_at: usize,
    ammo: Ammo,
    /// The map, lives and difficulty, one after another.
    map_at: usize,
    fields: Width,
    /// How many map numbers the game can save on.
    maps: u16,
    /// Map 0 is the world map.
    world_map: bool,
    /// Difficulty values beyond Easy 1 to Hard 3.
    difficulties: &'static [(u16, Difficulty)],
}

/// The source mods whose saves are known.
const SOURCE_MODS: &[Layout] = &[
    // Foray in the Forest 2.0 (`CK_DEF.H`): its file extension as signature,
    // `CKC` for the classic build. After the score come `nextextra` and
    // `oldscore`, then `ammo`, `drops`, three words of its own. Map 0 is the
    // world map and map 1 the opening rocket, which the game marks done
    // before the first level. Checked against a real save of v2.0a.
    Layout {
        episode: Episode::Keen4,
        signatures: &[b"CKS\0", b"CKC\0"],
        padded: true,
        levels_at: 4,
        levels: 27,
        level_width: Width::Word,
        level_max: 3,
        uncounted: &[0, 1],
        score_at: 4 + 2 * 27,
        ammo: Ammo::At(4 + 2 * 27 + 12),
        map_at: 4 + 2 * 27 + 22,
        fields: Width::Word,
        maps: 27,
        world_map: true,
        difficulties: &[(4, Difficulty::Expert)],
    },
    // Atroxian Realm 1.4, on the Keen Dreams source with Keen 5's game state
    // (`CK_GameState` in its `KD_DEF.H`): `keenScore`, `nextKeenAt`,
    // `numShots`, vitalin, security card, vines, fuses, four key gems. A new
    // game starts on map 25, the crash site, marked done after it. Older
    // versions saved in another layout. Checked against real saves of v1.4.
    Layout {
        episode: Episode::Keen4,
        signatures: &[b"CKD\0"],
        padded: true,
        levels_at: 4,
        levels: 26,
        level_width: Width::Word,
        level_max: 1,
        uncounted: &[25],
        score_at: 4 + 2 * 26,
        ammo: Ammo::At(4 + 2 * 26 + 8),
        map_at: 4 + 2 * 26 + 26,
        fields: Width::Word,
        maps: 26,
        world_map: true,
        difficulties: &[],
    },
    // Return to the Shadowlands 1.8 saves as `CK4`, but packed without the
    // padding byte, and with ten words after `nextKeenAt` (the third is the
    // ammo, the rest its items). Map 19 holds the high scores and map 20, the
    // rocket the game starts on, is marked done from the start, so only maps
    // 0-18 count. Checked against real saves of v1.8.
    Layout {
        episode: Episode::Keen4,
        signatures: &[b"CK4\0"],
        padded: false,
        levels_at: 4,
        levels: GALAXY_LEVELS,
        level_width: Width::Word,
        level_max: 1,
        uncounted: &[19, 20, 21, 22, 23, 24],
        score_at: 4 + 2 * 25,
        ammo: Ammo::At(4 + 2 * 25 + 12),
        map_at: 4 + 2 * 25 + 28,
        fields: Width::Word,
        maps: 21,
        world_map: true,
        difficulties: &[],
    },
    // Keen Dreams Gold Edition, on the Keen Dreams source (`KDG_DEF.H`),
    // packed in bytes: world position, score, `nextextra`, a done byte for
    // each of 47 maps, then damage time, flower powers, ammo, bombs, bombs
    // this level, keys, keycard, keycard text, `mapon`, `lives` and
    // `difficulty` (its own Keen 4 and Grand 5). Maps 21 and 22 are where a
    // new game starts, as in its own count. The score box shows the ammo on
    // maps 23-46 and the flower powers elsewhere. Checked against real saves
    // of the 2026-05-31 release.
    Layout {
        episode: Episode::Keen4,
        signatures: &[b"KDG\0"],
        padded: true,
        levels_at: 12,
        levels: 47,
        level_width: Width::Byte,
        level_max: 1,
        uncounted: &[21, 22],
        score_at: 4,
        ammo: Ammo::ByMap {
            maps: 23..=46,
            at: 12 + 47 + 2,
            otherwise: 12 + 47 + 1,
        },
        map_at: 12 + 47 + 8,
        fields: Width::Byte,
        maps: 48,
        world_map: false,
        difficulties: &[(4, Difficulty::Keen), (5, Difficulty::Grand)],
    },
];

impl Reader<'_> {
    fn field(&self, at: usize, width: Width) -> Result<u16, ParseError> {
        match width {
            Width::Byte => Ok(self.bytes(at, 1)?[0].into()),
            Width::Word => self.u16(at),
        }
    }

    fn signed(&self, at: usize, width: Width) -> Result<i16, ParseError> {
        match width {
            Width::Byte => Ok((self.bytes(at, 1)?[0] as i8).into()),
            Width::Word => self.i16(at),
        }
    }
}

impl Layout {
    fn read(&self, data: &[u8]) -> Result<SaveInfo, ParseError> {
        let r = Reader(data);
        let name = r.name(8, NAME_LEN)?;
        let state = if self.padded {
            (8 + NAME_LEN).next_multiple_of(2)
        } else {
            8 + NAME_LEN
        };
        let step = self.fields.bytes();
        let map_at = state + self.map_at;
        let map = r.field(map_at, self.fields)?;
        let lives = r.signed(map_at + step, self.fields)?;
        let difficulty = match r.field(map_at + 2 * step, self.fields)? {
            word @ 1..=3 => difficulty(word),
            word => self
                .difficulties
                .iter()
                .find(|(w, _)| *w == word)
                .map(|(_, d)| *d),
        }
        .ok_or(ParseError::Implausible("difficulty"))?;
        if map >= self.maps {
            return Err(ParseError::Implausible("map"));
        }
        if !(0..=99).contains(&lives) {
            return Err(ParseError::Implausible("lives"));
        }
        let mut levels_done = 0;
        for i in 0..self.levels {
            let at = state + self.levels_at + self.level_width.bytes() * i;
            let level = r.field(at, self.level_width)?;
            if level > self.level_max {
                return Err(ParseError::Implausible("levels"));
            }
            if level != 0 && !self.uncounted.contains(&i) {
                levels_done += 1;
            }
        }
        let ammo_at = match &self.ammo {
            Ammo::At(at) => *at,
            Ammo::ByMap { maps, at, .. } if maps.contains(&map) => *at,
            Ammo::ByMap { otherwise, .. } => *otherwise,
        };
        Ok(SaveInfo {
            name,
            score: Some(r.i32(state + self.score_at)?),
            ammo: Some(r.signed(state + ammo_at, self.fields)?),
            levels_done: Some(levels_done),
            difficulty: Some(difficulty),
            on_world_map: self.world_map.then_some(map == 0),
            lives: Some(lives),
            ship_parts: None,
        })
    }
}

/// The save as the first source mod layout of its signature reads it whose
/// checks pass, else the first one's error; `None` if no source mod played
/// as `episode` saves under this signature.
fn source_mod(episode: Episode, data: &[u8]) -> Option<Result<SaveInfo, ParseError>> {
    let signature = data.get(..4)?;
    let mut first = None;
    let layouts = SOURCE_MODS
        .iter()
        .filter(|l| l.episode == episode && l.signatures.iter().any(|s| *s == signature));
    for layout in layouts {
        match layout.read(data) {
            Ok(info) => return Some(Ok(info)),
            Err(e) => {
                first.get_or_insert(e);
            }
        }
    }
    first.map(Err)
}

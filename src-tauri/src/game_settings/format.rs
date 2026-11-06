//! The games' settings files: which bytes hold what. Read from the real files
//! of all versions; [`recognize`] leaves everything else alone.

use super::{GameKeys, GameSettings, SoundDevice};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Keen 1-3's `CTLPANEL`.
    Vorticons,
    /// Keen 4-6's `CONFIG`, version 2.
    GalaxyOld,
    /// Keen 4-6's `CONFIG`, version 4.
    GalaxyNew,
    /// Keen 5 v1.0: version 2 with the size of version 4.
    Keen5V10,
    /// Keen 4's special demo.
    GalaxyDemo,
    Dreams,
}

/// One value to write: a word, or else a byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit {
    pub offset: usize,
    pub wide: bool,
    pub value: u16,
}

/// Where a format keeps its settings; `None` where it has no such setting.
struct Layout {
    sound: usize,
    /// Off, PC Speaker, AdLib, rather than just on or off.
    device: bool,
    music: Option<usize>,
    score_box: Option<usize>,
    up: usize,
    down: usize,
    left: usize,
    right: usize,
    jump: usize,
    pogo: Option<usize>,
    fire: &'static [usize],
}

/// Keen 4-6, where the versions differ only in the fire key.
const fn galaxy(fire: &'static [usize]) -> Layout {
    Layout {
        sound: 0x206,
        device: true,
        music: Some(0x208),
        score_box: Some(0x216),
        up: 0x20F,
        down: 0x214,
        left: 0x211,
        right: 0x212,
        jump: 0x20C,
        pogo: Some(0x20D),
        fire,
    }
}

fn layout(format: Format) -> Layout {
    match format {
        Format::Vorticons => Layout {
            sound: 0,
            device: false,
            music: None,
            score_box: None,
            up: 0x22,
            down: 0x26,
            left: 0x28,
            right: 0x24,
            jump: 0x2A,
            pogo: Some(0x2B),
            fire: &[],
        },
        Format::GalaxyOld => galaxy(&[0x21E]),
        Format::GalaxyNew => galaxy(&[0x222]),
        // It holds Space at 0x222 too, but reads 0x21E (checked in its key menu).
        Format::Keen5V10 => galaxy(&[0x21E]),
        Format::GalaxyDemo => Layout {
            sound: 0x200,
            device: true,
            music: Some(0x202),
            score_box: Some(0x210),
            up: 0x209,
            down: 0x20E,
            left: 0x20B,
            right: 0x20C,
            jump: 0x206,
            pogo: Some(0x207),
            fire: &[],
        },
        Format::Dreams => Layout {
            sound: 0x280,
            device: true,
            music: None,
            score_box: None,
            up: 0x289,
            down: 0x28E,
            left: 0x28B,
            right: 0x28C,
            jump: 0x286,
            pogo: None,
            // The throw button.
            fire: &[0x287],
        },
    }
}

/// The format of settings file `data` of game `game_id`, if it's one we know
/// and its values make sense.
pub fn recognize(game_id: &str, data: &[u8]) -> Option<Format> {
    let format = match (game_id, data.len()) {
        ("keen1" | "keen2" | "keen3", 44) => Format::Vorticons,
        ("kdreams", 656) => Format::Dreams,
        ("keen4", 534) if data.starts_with(b"ID Software") => Format::GalaxyDemo,
        ("keen4" | "keen5" | "keen6", size) => {
            let episode = game_id.as_bytes()[4];
            if data.get(..4)? != [b'C', b'K', episode, 0] {
                return None;
            }
            match (read(data, 4, true)?, size, episode) {
                (2, 543, _) => Format::GalaxyOld,
                (4, 557, _) => Format::GalaxyNew,
                (2, 557, b'5') => Format::Keen5V10,
                _ => return None,
            }
        }
        _ => return None,
    };
    let l = layout(format);
    let keys_set = [l.up, l.down, l.left, l.right, l.jump]
        .into_iter()
        .chain(l.pogo)
        .chain(l.fire.iter().copied())
        .all(|at| data[at] != 0);
    // Keen 1-3's shipped files can hold anything in their sound word.
    let audio_fits = !l.device
        || (read(data, l.sound, true)? <= 2
            && l.music
                .is_none_or(|at| read(data, at, true).is_some_and(|m| m <= 1)));
    (keys_set && audio_fits).then_some(format)
}

/// What to write for `settings` into `data`, a file of `format`: only the
/// values that differ.
pub fn edits(format: Format, settings: &GameSettings, data: &[u8]) -> Vec<Edit> {
    let l = layout(format);
    let mut wanted: Vec<(usize, bool, u16)> = Vec::new();
    if let Some(current) = read(data, l.sound, true)
        && let Some(value) = sound_value(l.device, settings, current)
    {
        wanted.push((l.sound, true, value));
    }
    if let (Some(at), Some(on)) = (l.music, settings.music) {
        wanted.push((at, true, u16::from(on)));
    }
    if let (Some(at), Some(on)) = (l.score_box, settings.score_box) {
        wanted.push((at, true, u16::from(on)));
    }
    let GameKeys {
        up,
        down,
        left,
        right,
        jump,
        pogo,
        fire,
    } = settings.keys;
    let keys = [
        (Some(l.up), up),
        (Some(l.down), down),
        (Some(l.left), left),
        (Some(l.right), right),
        (Some(l.jump), jump),
        (l.pogo, pogo),
    ];
    let fires = l.fire.iter().map(|at| (Some(*at), fire));
    for (at, key) in keys.into_iter().chain(fires) {
        if let (Some(at), Some(key)) = (at, key) {
            wanted.push((at, false, u16::from(key)));
        }
    }
    wanted
        .into_iter()
        .filter(|(at, wide, value)| read(data, *at, *wide) != Some(*value))
        .map(|(offset, wide, value)| Edit {
            offset,
            wide,
            value,
        })
        .collect()
}

/// The sound value `settings` asks for, given the file's `current` one.
fn sound_value(device: bool, settings: &GameSettings, current: u16) -> Option<u16> {
    const OFF: u16 = 0;
    const PC_SPEAKER: u16 = 1;
    const ADLIB: u16 = 2;
    if !device {
        return settings.sound.map(u16::from);
    }
    let chosen = settings.device.map(|d| match d {
        SoundDevice::PcSpeaker => PC_SPEAKER,
        SoundDevice::SoundBlaster => ADLIB,
    });
    match settings.sound {
        Some(false) => Some(OFF),
        Some(true) => Some(chosen.unwrap_or(if current == OFF { ADLIB } else { current })),
        // A device alone doesn't turn sound on.
        None if current == OFF => None,
        None => chosen,
    }
}

pub fn read(data: &[u8], offset: usize, wide: bool) -> Option<u16> {
    if wide {
        let bytes = data.get(offset..offset.checked_add(2)?)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    } else {
        data.get(offset).map(|b| u16::from(*b))
    }
}

/// Writes `edit` into `data`; false if it's out of bounds.
pub fn write(data: &mut [u8], edit: &Edit) -> bool {
    let len = if edit.wide { 2 } else { 1 };
    let Some(end) = edit.offset.checked_add(len) else {
        return false;
    };
    match data.get_mut(edit.offset..end) {
        Some(bytes) => {
            bytes.copy_from_slice(&edit.value.to_le_bytes()[..len]);
            true
        }
        None => false,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A file of `format` with the games' defaults: AdLib effects and music,
    /// Ctrl and Alt, arrows, score box on.
    pub(crate) fn file(format: Format) -> Vec<u8> {
        let (size, head): (usize, &[u8]) = match format {
            Format::Vorticons => (44, &[1, 0]),
            Format::GalaxyOld => (543, b"CK4\0\x02\0"),
            Format::GalaxyNew => (557, b"CK4\0\x04\0"),
            Format::Keen5V10 => (557, b"CK5\0\x02\0"),
            Format::GalaxyDemo => (534, b"ID Software"),
            Format::Dreams => (656, b"Jordy"),
        };
        let mut data = vec![0; size];
        data[..head.len()].copy_from_slice(head);
        let directions = |data: &mut [u8], at: usize| {
            data[at..at + 8].copy_from_slice(&[0x47, 0x48, 0x49, 0x4B, 0x4D, 0x4F, 0x50, 0x51]);
        };
        let audio = |data: &mut [u8], at: usize| {
            data[at] = 2;
            data[at + 2] = 1;
        };
        match format {
            Format::Vorticons => {
                data[0x22..0x2C]
                    .copy_from_slice(&[0x48, 0x49, 0x4D, 0x51, 0x50, 0x4F, 0x4B, 0x47, 0x1D, 0x38]);
            }
            Format::GalaxyDemo => {
                audio(&mut data, 0x200);
                data[0x206] = 0x1D;
                data[0x207] = 0x38;
                directions(&mut data, 0x208);
                data[0x210] = 1;
            }
            Format::Dreams => {
                audio(&mut data, 0x280);
                data[0x286] = 0x1D;
                data[0x287] = 0x38;
                directions(&mut data, 0x288);
            }
            _ => {
                audio(&mut data, 0x206);
                data[0x20C] = 0x1D;
                data[0x20D] = 0x38;
                directions(&mut data, 0x20E);
                data[0x216] = 1;
                match format {
                    Format::GalaxyOld => data[0x21E] = 0x39,
                    Format::GalaxyNew => data[0x222] = 0x39,
                    _ => {
                        data[0x21E] = 0x39;
                        data[0x222] = 0x39;
                    }
                }
            }
        }
        data
    }

    fn game(format: Format) -> &'static str {
        match format {
            Format::Vorticons => "keen1",
            Format::Keen5V10 => "keen5",
            Format::Dreams => "kdreams",
            _ => "keen4",
        }
    }

    const ALL: [Format; 6] = [
        Format::Vorticons,
        Format::GalaxyOld,
        Format::GalaxyNew,
        Format::Keen5V10,
        Format::GalaxyDemo,
        Format::Dreams,
    ];

    fn offsets(edits: &[Edit]) -> Vec<(usize, u16)> {
        edits.iter().map(|e| (e.offset, e.value)).collect()
    }

    #[test]
    fn recognizes_each_format() {
        for format in ALL {
            assert_eq!(recognize(game(format), &file(format)), Some(format));
        }
    }

    #[test]
    fn rejects_what_it_does_not_know() {
        let keen4 = file(Format::GalaxyNew);
        assert_eq!(recognize("keen4", &keen4[..100]), None, "size");
        let mut other = keen4.clone();
        other[..4].copy_from_slice(b"CKS\0");
        assert_eq!(recognize("keen4", &other), None, "a source mod's own");
        let mut sound = keen4.clone();
        sound[0x206] = 7;
        assert_eq!(recognize("keen4", &sound), None, "sound");
        let mut key = keen4.clone();
        key[0x20C] = 0;
        assert_eq!(recognize("keen4", &key), None, "key");
        assert_eq!(recognize("keen5", &keen4), None, "another episode's");
        assert_eq!(recognize("keen1", &keen4), None, "another engine's");
    }

    #[test]
    fn keen6_special_demo_is_the_older_layout() {
        let mut data = file(Format::GalaxyOld);
        data[..4].copy_from_slice(b"CK6\0");
        assert_eq!(recognize("keen6", &data), Some(Format::GalaxyOld));
    }

    #[test]
    fn keen1_sound_is_the_first_word() {
        let mut data = file(Format::Vorticons);
        let off = GameSettings {
            sound: Some(false),
            ..GameSettings::default()
        };
        assert_eq!(
            edits(Format::Vorticons, &off, &data),
            [Edit {
                offset: 0,
                wide: true,
                value: 0
            }]
        );
        // Keen 2's shipped files hold leftovers ("SA") there.
        data[..2].copy_from_slice(b"SA");
        assert_eq!(recognize("keen2", &data), Some(Format::Vorticons));
        let on = GameSettings {
            sound: Some(true),
            ..GameSettings::default()
        };
        assert_eq!(offsets(&edits(Format::Vorticons, &on, &data)), [(0, 1)]);
    }

    #[test]
    fn keys_go_to_their_offsets() {
        let settings = GameSettings {
            keys: GameKeys {
                up: Some(0x11),
                down: Some(0x1F),
                left: Some(0x1E),
                right: Some(0x20),
                jump: Some(0x24),
                pogo: Some(0x25),
                fire: Some(0x26),
            },
            ..GameSettings::default()
        };
        let cases: [(Format, &[(usize, u16)]); 6] = [
            (
                Format::Vorticons,
                &[
                    (0x22, 0x11),
                    (0x24, 0x20),
                    (0x26, 0x1F),
                    (0x28, 0x1E),
                    (0x2A, 0x24),
                    (0x2B, 0x25),
                ],
            ),
            (
                Format::GalaxyOld,
                &[
                    (0x20C, 0x24),
                    (0x20D, 0x25),
                    (0x20F, 0x11),
                    (0x211, 0x1E),
                    (0x212, 0x20),
                    (0x214, 0x1F),
                    (0x21E, 0x26),
                ],
            ),
            (
                Format::GalaxyNew,
                &[
                    (0x20C, 0x24),
                    (0x20D, 0x25),
                    (0x20F, 0x11),
                    (0x211, 0x1E),
                    (0x212, 0x20),
                    (0x214, 0x1F),
                    (0x222, 0x26),
                ],
            ),
            (
                Format::Keen5V10,
                &[
                    (0x20C, 0x24),
                    (0x20D, 0x25),
                    (0x20F, 0x11),
                    (0x211, 0x1E),
                    (0x212, 0x20),
                    (0x214, 0x1F),
                    (0x21E, 0x26),
                ],
            ),
            (
                Format::GalaxyDemo,
                &[
                    (0x206, 0x24),
                    (0x207, 0x25),
                    (0x209, 0x11),
                    (0x20B, 0x1E),
                    (0x20C, 0x20),
                    (0x20E, 0x1F),
                ],
            ),
            (
                Format::Dreams,
                &[
                    (0x286, 0x24),
                    (0x287, 0x26),
                    (0x289, 0x11),
                    (0x28B, 0x1E),
                    (0x28C, 0x20),
                    (0x28E, 0x1F),
                ],
            ),
        ];
        for (format, expected) in cases {
            let mut got = offsets(&edits(format, &settings, &file(format)));
            got.sort_unstable();
            assert_eq!(got, expected, "{format:?}");
        }
    }

    #[test]
    fn dreams_has_no_music_or_score_box() {
        let settings = GameSettings {
            music: Some(false),
            score_box: Some(false),
            ..GameSettings::default()
        };
        assert!(edits(Format::Dreams, &settings, &file(Format::Dreams)).is_empty());
    }

    #[test]
    fn vorticons_has_no_music_device_or_score_box() {
        let settings = GameSettings {
            music: Some(false),
            device: Some(SoundDevice::PcSpeaker),
            score_box: Some(false),
            ..GameSettings::default()
        };
        assert!(edits(Format::Vorticons, &settings, &file(Format::Vorticons)).is_empty());
    }

    #[test]
    fn score_box_is_a_word() {
        let off = GameSettings {
            score_box: Some(false),
            ..GameSettings::default()
        };
        for (format, at) in [(Format::GalaxyNew, 0x216), (Format::GalaxyDemo, 0x210)] {
            assert_eq!(
                edits(format, &off, &file(format)),
                [Edit {
                    offset: at,
                    wide: true,
                    value: 0
                }]
            );
        }
    }

    #[test]
    fn nothing_set_means_no_edits() {
        for format in ALL {
            assert!(edits(format, &GameSettings::default(), &file(format)).is_empty());
        }
    }

    #[test]
    fn equal_values_mean_no_edits() {
        let settings = GameSettings {
            sound: Some(true),
            music: Some(true),
            score_box: Some(true),
            keys: GameKeys {
                up: Some(0x48),
                jump: Some(0x1D),
                fire: Some(0x39),
                ..GameKeys::default()
            },
            ..GameSettings::default()
        };
        let data = file(Format::GalaxyNew);
        assert!(edits(Format::GalaxyNew, &settings, &data).is_empty());
    }

    #[test]
    fn sound_and_device() {
        use SoundDevice::{PcSpeaker as Pc, SoundBlaster as Sb};
        let cases = [
            (Some(false), Some(Sb), 2, Some(0)),
            (Some(true), Some(Pc), 2, Some(1)),
            (Some(true), None, 1, None),
            (Some(true), None, 0, Some(2)),
            (None, Some(Sb), 1, Some(2)),
            (None, Some(Sb), 0, None),
            (None, Some(Pc), 2, Some(1)),
        ];
        for (sound, device, in_file, written) in cases {
            let mut data = file(Format::GalaxyNew);
            data[0x206] = in_file;
            let settings = GameSettings {
                sound,
                device,
                ..GameSettings::default()
            };
            let expected: Vec<(usize, u16)> = written.iter().map(|w| (0x206, *w)).collect();
            assert_eq!(
                offsets(&edits(Format::GalaxyNew, &settings, &data)),
                expected,
                "{sound:?} {device:?} {in_file}"
            );
        }
    }

    #[test]
    fn music() {
        let mut data = file(Format::GalaxyNew);
        let with = |music| GameSettings {
            music: Some(music),
            ..GameSettings::default()
        };
        assert_eq!(
            offsets(&edits(Format::GalaxyNew, &with(false), &data)),
            [(0x208, 0)]
        );
        data[0x208] = 0;
        assert_eq!(
            offsets(&edits(Format::GalaxyNew, &with(true), &data)),
            [(0x208, 1)]
        );
    }

    #[test]
    fn read_and_write_stay_in_bounds() {
        let mut data = vec![0x34, 0x12, 0x56];
        assert_eq!(read(&data, 0, true), Some(0x1234));
        assert_eq!(read(&data, 2, false), Some(0x56));
        assert_eq!(read(&data, 2, true), None);
        let edit = |offset, wide, value| Edit {
            offset,
            wide,
            value,
        };
        assert!(write(&mut data, &edit(0, true, 0xABCD)));
        assert_eq!(data, [0xCD, 0xAB, 0x56]);
        assert!(write(&mut data, &edit(2, false, 0x11)));
        assert_eq!(data[2], 0x11);
        assert!(!write(&mut data, &edit(2, true, 1)));
        assert!(!write(&mut data, &edit(9, false, 1)));
    }

    /// Every settings file of the test games in `.local/games`.
    #[test]
    #[ignore = "needs the owner's game files"]
    fn real_files_are_recognized() {
        fn walk(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, found);
                } else {
                    found.push(path);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.local/games");
        let mut files = Vec::new();
        walk(&root, &mut files);
        let mut checked = 0;
        let mut unknown = Vec::new();
        for path in files {
            let name = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_ascii_uppercase();
            let Some((stem, ext)) = name.split_once('.') else {
                continue;
            };
            let game = match (stem, ext) {
                ("CTLPANEL", "CK1") => "keen1",
                ("CTLPANEL", "CK2") => "keen2",
                ("CTLPANEL", "CK3") => "keen3",
                ("CONFIG", "CK4") => "keen4",
                ("CONFIG", "CK5") => "keen5",
                ("CONFIG", "CK6") => "keen6",
                ("CONFIG", "KDR") | ("KDREAMS", "CFG") => "kdreams",
                _ => continue,
            };
            checked += 1;
            if recognize(game, &std::fs::read(&path).unwrap()).is_none() {
                unknown.push(path.display().to_string());
            }
        }
        assert!(checked > 30, "only {checked} files");
        assert!(unknown.is_empty(), "{unknown:#?}");
    }
}

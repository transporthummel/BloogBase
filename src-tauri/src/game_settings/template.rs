//! A settings file for a game that hasn't written its own yet. High scores
//! come from the program as DOS loads it, so a mod keeps its own table.

use super::format::Format;

/// A high score: a 58-byte name, the score as a long, a level as a word.
const RECORD: usize = 64;

/// What each format holds after the table. All set as the games do on a
/// machine with an AdLib card: AdLib effects and music, Ctrl, Alt and arrows,
/// the score box on, Space to fire.
const AUDIO_AND_KEYS: [u8; 16] = [
    0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x1D, 0x38, 0x47, 0x48, 0x49, 0x4B, 0x4D, 0x4F, 0x50, 0x51,
];
const OLD_REST: [u8; 9] = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x39];
const NEW_REST: [u8; 23] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x39, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
/// Keen 5's files hold more after the fire key, whatever it is.
const KEEN5_END: [u8; 10] = [0x00, 0x00, 0x02, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00];
const KEEN5_V10_REST: [u8; 13] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x39, 0x00, 0x00, 0x00, 0x39,
];
const DEMO_REST: [u8; 6] = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
const VORTICONS: [u8; 44] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x14, 0x00, 0x14, 0x00, 0x00, 0x00,
    0x14, 0x00, 0x14, 0x00, 0x00, 0x00, 0x3C, 0x00, 0x3C, 0x00, 0x00, 0x00, 0x3C, 0x00, 0x3C, 0x00,
    0x05, 0x00, 0x48, 0x49, 0x4D, 0x51, 0x50, 0x4F, 0x4B, 0x47, 0x1D, 0x38,
];

/// The format a version of a game writes.
pub fn format_for_version(game_id: &str, version_id: Option<&str>) -> Option<Format> {
    // Keen 1-3 and Dreams have one format, whatever the version.
    Some(match game_id {
        "keen1" | "keen2" | "keen3" => Format::Vorticons,
        "kdreams" => Format::Dreams,
        "keen4" | "keen5" | "keen6" => {
            let version = version_id?;
            if version == "keen4-demo" {
                Format::GalaxyDemo
            } else if version == "keen5-1.0-ega" {
                Format::Keen5V10
            } else if version.contains("-1.4") || version.contains("-1.5") {
                Format::GalaxyNew
            } else {
                Format::GalaxyOld
            }
        }
        _ => return None,
    })
}

/// How many high scores a format holds.
pub fn table_records(format: Format) -> usize {
    match format {
        Format::Dreams => 10,
        _ => 8,
    }
}

/// The first `count` records in `image` that look like a high score table.
pub fn find_score_table(image: &[u8], count: usize) -> Option<Vec<u8>> {
    let size = count * RECORD;
    if image.len() < size {
        return None;
    }
    (0..=image.len() - size)
        .map(|at| &image[at..at + size])
        .find(|table| is_table(table, count))
        .map(<[u8]>::to_vec)
}

fn is_table(table: &[u8], count: usize) -> bool {
    let mut names = 0;
    for record in table.as_chunks::<RECORD>().0 {
        let Some(name) = record_name(record) else {
            return false;
        };
        names += usize::from(!name.is_empty());
    }
    // Some games leave a line empty, but no game leaves most of them.
    names * 2 >= count
}

/// The name in `record`, if the record makes sense.
fn record_name(record: &[u8]) -> Option<&[u8]> {
    let (name, rest) = record.split_at(58);
    let score = u32::from_le_bytes(rest[..4].try_into().ok()?);
    let level = u16::from_le_bytes(rest[4..6].try_into().ok()?);
    if score > 99_999_999 || level > 32 {
        return None;
    }
    let end = name.iter().position(|b| *b == 0)?;
    let (text, padding) = name.split_at(end);
    let fits = text.iter().all(|b| (0x20..0x7F).contains(b)) && padding.iter().all(|b| *b == 0);
    fits.then_some(text)
}

/// A new settings file of `format` for `game_id`, with the high scores
/// `scores` where the format holds any.
pub fn new_file(format: Format, game_id: &str, scores: Option<Vec<u8>>) -> Option<Vec<u8>> {
    if format == Format::Vorticons {
        return Some(VORTICONS.to_vec());
    }
    let scores = scores.filter(|s| s.len() == table_records(format) * RECORD)?;
    let keen5 = game_id == "keen5";
    let mut out = Vec::new();
    if matches!(
        format,
        Format::GalaxyOld | Format::GalaxyNew | Format::Keen5V10
    ) {
        let episode = *game_id.as_bytes().get(4)?;
        let version = if format == Format::GalaxyNew { 4 } else { 2 };
        out.extend([b'C', b'K', episode, 0, version, 0]);
    }
    out.extend(scores);
    out.extend(AUDIO_AND_KEYS);
    match format {
        Format::Vorticons | Format::Dreams => {}
        Format::GalaxyDemo => out.extend(DEMO_REST),
        Format::GalaxyOld => out.extend(OLD_REST),
        Format::GalaxyNew if keen5 => {
            out.extend(&NEW_REST[..13]);
            out.extend(KEEN5_END);
        }
        Format::GalaxyNew => out.extend(NEW_REST),
        Format::Keen5V10 => {
            out.extend(KEEN5_V10_REST);
            out.extend(KEEN5_END);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_settings::format::recognize;

    /// `count` records as a game holds them: names, 10000 points, level 0.
    fn table(names: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for name in names {
            let mut record = vec![0; RECORD];
            record[..name.len()].copy_from_slice(name.as_bytes());
            record[58..62].copy_from_slice(&10_000u32.to_le_bytes());
            out.extend(record);
        }
        out
    }

    const NAMES: [&str; 8] = [
        "Id Software - '91",
        "",
        "Jason Blochowiak",
        "Adrian Carmack",
        "John Carmack",
        "Tom Hall",
        "John Romero",
        "Jay Wilbur",
    ];

    #[test]
    fn finds_a_table_among_other_data() {
        let scores = table(&NAMES);
        let mut image = vec![0xCC; 1000];
        image.extend(&scores);
        image.extend([0xAA; 500]);
        assert_eq!(find_score_table(&image, 8), Some(scores));
    }

    #[test]
    fn no_table_in_noise() {
        let noise: Vec<u8> = (0..4000u32).map(|i| (i * 7 % 251) as u8).collect();
        assert_eq!(find_score_table(&noise, 8), None);
        assert_eq!(find_score_table(&[0; 2000], 8), None, "all empty");
        assert_eq!(find_score_table(&[], 8), None);
    }

    #[test]
    fn patched_names_are_found() {
        let scores = table(&["Norp", "the", "Yorp", "a", "b", "c", "d", "e"]);
        let mut image = vec![0xCC; 64];
        image.extend(&scores);
        assert_eq!(find_score_table(&image, 8), Some(scores));
    }

    #[test]
    fn vorticons_new_file_is_the_default() {
        assert_eq!(
            new_file(Format::Vorticons, "keen1", None).unwrap(),
            [
                0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x14, 0x00, 0x14, 0x00,
                0x00, 0x00, 0x14, 0x00, 0x14, 0x00, 0x00, 0x00, 0x3C, 0x00, 0x3C, 0x00, 0x00, 0x00,
                0x3C, 0x00, 0x3C, 0x00, 0x05, 0x00, 0x48, 0x49, 0x4D, 0x51, 0x50, 0x4F, 0x4B, 0x47,
                0x1D, 0x38,
            ]
        );
    }

    #[test]
    fn new_files_are_recognized() {
        let scores = |n: usize, name: &'static str| Some(table(&vec![name; n]));
        for (format, game, count) in [
            (Format::GalaxyOld, "keen4", 8),
            (Format::GalaxyOld, "keen6", 8),
            (Format::GalaxyNew, "keen4", 8),
            (Format::GalaxyNew, "keen5", 8),
            (Format::GalaxyNew, "keen6", 8),
            (Format::Keen5V10, "keen5", 8),
            (Format::GalaxyDemo, "keen4", 8),
            (Format::Dreams, "kdreams", 10),
        ] {
            let name = if format == Format::GalaxyDemo {
                "ID Software - '91"
            } else {
                "Name"
            };
            let data = new_file(format, game, scores(count, name)).unwrap();
            assert_eq!(recognize(game, &data), Some(format), "{format:?} {game}");
        }
    }

    #[test]
    fn galaxy_needs_scores() {
        assert_eq!(new_file(Format::GalaxyNew, "keen4", None), None);
        assert_eq!(
            new_file(Format::GalaxyNew, "keen4", Some(vec![0; 10])),
            None
        );
    }

    #[test]
    fn versions_have_their_formats() {
        let cases = [
            ("keen2", "keen2-1.31", Some(Format::Vorticons)),
            ("keen4", "keen4-demo", Some(Format::GalaxyDemo)),
            ("keen4", "keen4-1.2-ega", Some(Format::GalaxyOld)),
            ("keen4", "keen4-1.4-ega-gt", Some(Format::GalaxyNew)),
            ("keen5", "keen5-1.0-ega", Some(Format::Keen5V10)),
            ("keen6", "keen6-1.5-cga", Some(Format::GalaxyNew)),
            ("keen6", "keen6-promo", Some(Format::GalaxyOld)),
            ("kdreams", "kdreams-1.93", Some(Format::Dreams)),
        ];
        for (game, version, format) in cases {
            assert_eq!(format_for_version(game, Some(version)), format, "{version}");
        }
        assert_eq!(format_for_version("keen4", None), None);
    }

    /// Imports every game in `.local/games/versions`: its version's format
    /// must be the one of its own file, and its program must hold a table,
    /// except Keen Dreams' (its names are empty) and Keen 4's special demo.
    #[test]
    #[ignore = "needs the owner's game files"]
    fn real_versions_have_a_table_and_format() {
        use crate::db::Db;
        use crate::paths::AppPaths;
        use crate::{catalog, firing, library, profiles, recipe::Recipe};

        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.local/games/versions");
        let mut problems = Vec::new();
        let mut checked = 0;
        for folder in std::fs::read_dir(root).unwrap() {
            let folder = folder.unwrap().path();
            let tmp = tempfile::tempdir().unwrap();
            let paths = AppPaths::new(tmp.path().join("data"));
            paths.ensure().unwrap();
            let db = Db::open_in_memory().unwrap();
            let Ok(installs) = library::import(&paths, &db, &folder) else {
                continue;
            };
            for install in installs {
                checked += 1;
                let game = catalog::game(&install.game_id).unwrap();
                let version = install.version_id.as_deref();
                let label = format!("{} {version:?}", folder.display());
                let (_, layers) = profiles::get(&db, &install.id).unwrap();
                let format = format_for_version(&install.game_id, version);
                let own = std::fs::read_dir(&layers.upper)
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.ok())
                    .find(|e| {
                        let name = e.file_name();
                        let name = name.to_string_lossy();
                        game.state_files
                            .iter()
                            .any(|f| f.eq_ignore_ascii_case(&name))
                    })
                    .and_then(|e| std::fs::read(e.path()).ok());
                if let Some(own) = own
                    && recognize(&install.game_id, &own) != format
                {
                    problems.push(format!("{label}: format {format:?} isn't its file's"));
                }
                if format == Some(Format::Vorticons) {
                    continue;
                }
                let image = firing::image_as_dos_runs(game, &Recipe::Episode, &layers.sources());
                let found = format.and_then(|f| find_score_table(&image?, table_records(f)));
                match found {
                    Some(t) => {
                        let first = t.split(|b| *b == 0).next().unwrap();
                        eprintln!("{label}: {}", String::from_utf8_lossy(first));
                    }
                    None if matches!(format, Some(Format::Dreams | Format::GalaxyDemo)) => {}
                    None => problems.push(format!("{label}: no table")),
                }
            }
        }
        eprintln!("checked {checked}");
        assert!(checked > 30, "only {checked}");
        assert!(problems.is_empty(), "{problems:#?}");
    }
}

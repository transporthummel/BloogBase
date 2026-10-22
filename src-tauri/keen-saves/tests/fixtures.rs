//! Real saves made by playing. The Keen 4-6 and Keen Dreams fixtures are cut
//! after the game state: the rest of those files is level data.

use keen_saves::{Difficulty, Episode, ParseError, SaveInfo, parse};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn keen1() {
    let info = parse(Episode::Keen1, &fixture("SAVED1.CK1")).unwrap();
    assert_eq!(
        info,
        SaveInfo {
            score: Some(123_100),
            lives: Some(4),
            ammo: Some(64),
            levels_done: Some(13),
            on_world_map: Some(true),
            ship_parts: Some(3),
            ..SaveInfo::default()
        }
    );
}

#[test]
fn keen2_and_3_have_no_ship_parts() {
    let k2 = parse(Episode::Keen2, &fixture("SAVED1.CK2")).unwrap();
    assert_eq!(
        (k2.score, k2.lives, k2.ship_parts),
        (Some(0x0004_9f98), Some(1), None)
    );
    let k3 = parse(Episode::Keen3, &fixture("SAVED1.CK3")).unwrap();
    assert_eq!(
        (k3.score, k3.lives, k3.ammo),
        (Some(503_200), Some(9), Some(49))
    );
}

#[test]
fn keen4() {
    let info = parse(Episode::Keen4, &fixture("SAVEGAM0.CK4")).unwrap();
    assert_eq!(
        info,
        SaveInfo {
            name: Some("pabba".into()),
            score: Some(48_100),
            lives: Some(13),
            ammo: Some(45),
            levels_done: Some(2),
            difficulty: Some(Difficulty::Hard),
            on_world_map: Some(false),
            ship_parts: None,
        }
    );
}

#[test]
fn keen6() {
    let info = parse(Episode::Keen6, &fixture("SAVEGAM0.CK6")).unwrap();
    assert_eq!(info.name.as_deref(), Some("pabba"));
    assert_eq!(
        (info.score, info.lives, info.ammo, info.levels_done),
        (Some(655_700), Some(21), Some(102), Some(16))
    );
    assert_eq!(info.difficulty, Some(Difficulty::Hard));
}

#[test]
fn keen5_synthetic() {
    let mut data = vec![0u8; 0x80];
    data[..4].copy_from_slice(b"CK5\0");
    data[8..13].copy_from_slice(b"Fuse!");
    data[0x2e] = 1; // one level done
    data[0x60..0x64].copy_from_slice(&1234i32.to_le_bytes());
    data[0x68] = 7; // ammo
    // mapon at 0x7a (two words more than Keen 4), then lives, difficulty.
    data[0x7c] = 5;
    data[0x7e] = 1;
    let info = parse(Episode::Keen5, &data).unwrap();
    assert_eq!(info.name.as_deref(), Some("Fuse!"));
    assert_eq!(
        (info.score, info.ammo, info.lives, info.levels_done),
        (Some(1234), Some(7), Some(5), Some(1))
    );
    assert_eq!(
        (info.on_world_map, info.difficulty),
        (Some(true), Some(Difficulty::Easy))
    );
}

#[test]
fn dreams() {
    let info = parse(Episode::Dreams, &fixture("SAVEGM0.KDR")).unwrap();
    assert_eq!(
        info,
        SaveInfo {
            name: Some("papa".into()),
            score: Some(138_700),
            lives: Some(12),
            ammo: Some(71),
            levels_done: Some(10),
            difficulty: Some(Difficulty::Hard),
            on_world_map: Some(false),
            ship_parts: None,
        }
    );
}

#[test]
fn dreams_saves_normal_as_zero() {
    let mut data = fixture("SAVEGM0.KDR");
    data[0x62] = 0;
    let info = parse(Episode::Dreams, &data).unwrap();
    assert_eq!(info.difficulty, Some(Difficulty::Normal));
}

#[test]
fn wrong_files_are_errors_not_panics() {
    assert_eq!(
        parse(Episode::Keen4, &fixture("SAVEGAM0.CK6")),
        Err(ParseError::BadSignature)
    );
    assert!(matches!(
        parse(Episode::Keen4, b"CK4\0"),
        Err(ParseError::TooShort { .. })
    ));
    assert!(matches!(
        parse(Episode::Keen1, &[0; 10]),
        Err(ParseError::WrongSize { .. })
    ));
    assert!(parse(Episode::Dreams, &[]).is_err());
    for len in 0..140 {
        let data = vec![0xffu8; len];
        for ep in [
            Episode::Keen1,
            Episode::Keen4,
            Episode::Keen6,
            Episode::Dreams,
        ] {
            let _ = parse(ep, &data);
        }
    }
}

#[test]
fn names_are_trimmed_and_cleaned() {
    let mut data = fixture("SAVEGAM0.CK4");
    data[8..16].copy_from_slice(b" a\x82b    ");
    data[16] = 0;
    let info = parse(Episode::Keen4, &data).unwrap();
    assert_eq!(info.name.as_deref(), Some("a\u{FFFD}b"));
    data[8] = 0;
    assert_eq!(parse(Episode::Keen4, &data).unwrap().name, None);
}

#[test]
fn episodes_from_game_ids() {
    assert_eq!(Episode::from_game_id("kdreams"), Some(Episode::Dreams));
    assert_eq!(Episode::from_game_id("keen5"), Some(Episode::Keen5));
    assert_eq!(Episode::from_game_id("doom"), None);
}

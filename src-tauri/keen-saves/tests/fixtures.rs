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
fn keen5() {
    let info = parse(Episode::Keen5, &fixture("SAVEGAM0.CK5")).unwrap();
    assert_eq!(
        info,
        SaveInfo {
            name: Some("cheesecake".into()),
            score: Some(2_200),
            lives: Some(2),
            ammo: Some(9),
            levels_done: Some(0),
            difficulty: Some(Difficulty::Hard),
            on_world_map: Some(false),
            ship_parts: None,
        }
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

/// Foray in the Forest 2.0a, a new Expert game saved right on the world map
/// (`FITF-X.EXE /NOWAIT`, which marks the opening rocket done).
#[test]
fn foray_in_the_forest() {
    let data = fixture("SAVEGAM0.CKS");
    let info = parse(Episode::Keen4, &data).unwrap();
    assert_eq!(
        info,
        SaveInfo {
            name: Some("test".into()),
            score: Some(0),
            lives: Some(3),
            ammo: Some(3),
            levels_done: Some(0),
            difficulty: Some(Difficulty::Expert),
            on_world_map: Some(true),
            ship_parts: None,
        }
    );

    // The classic build writes the same layout under its own extension.
    let mut classic = data.clone();
    classic[..4].copy_from_slice(b"CKC\0");
    assert_eq!(parse(Episode::Keen4, &classic).unwrap(), info);

    // A level done counts; the world map and the opening rocket don't.
    let mut later = data.clone();
    let levels = 42 + 4;
    later[levels + 2 * 5] = 3; // done and perfect
    assert_eq!(parse(Episode::Keen4, &later).unwrap().levels_done, Some(1));

    // Anything the game can't have written is refused.
    let mut odd = data.clone();
    odd[126] = 9; // difficulty
    assert_eq!(
        parse(Episode::Keen4, &odd),
        Err(ParseError::Implausible("difficulty"))
    );
    let mut odd = data.clone();
    odd[levels] = 7;
    assert_eq!(
        parse(Episode::Keen4, &odd),
        Err(ParseError::Implausible("levels"))
    );
    // Only Keen 4 knows the signature.
    assert_eq!(parse(Episode::Keen5, &data), Err(ParseError::BadSignature));
}

/// Atroxian Realm 1.4: a Normal game saved on the world map after the crash
/// site, and a Hard one saved inside the crash site (map 25).
#[test]
fn atroxian_realm() {
    let data = fixture("SAVEGAM4.CKD");
    assert_eq!(
        parse(Episode::Keen4, &data).unwrap(),
        SaveInfo {
            name: Some("map".into()),
            score: Some(700),
            lives: Some(4),
            ammo: Some(5),
            levels_done: Some(0),
            difficulty: Some(Difficulty::Normal),
            on_world_map: Some(true),
            ship_parts: None,
        }
    );
    assert_eq!(
        parse(Episode::Keen4, &fixture("SAVEGAM1.CKD")).unwrap(),
        SaveInfo {
            name: Some("crash hard".into()),
            score: Some(0),
            lives: Some(5),
            ammo: Some(5),
            levels_done: Some(0),
            difficulty: Some(Difficulty::Hard),
            on_world_map: Some(false),
            ship_parts: None,
        }
    );

    // A level done counts; the crash site, marked done in this save, doesn't.
    let levels = 42 + 4;
    assert_eq!(data[levels + 2 * 25], 1);
    let mut later = data.clone();
    later[levels + 2 * 3] = 1;
    assert_eq!(parse(Episode::Keen4, &later).unwrap().levels_done, Some(1));

    // Anything the game can't have written is refused.
    let mut odd = data.clone();
    odd[0x80] = 9; // difficulty
    assert_eq!(
        parse(Episode::Keen4, &odd),
        Err(ParseError::Implausible("difficulty"))
    );
    let mut odd = data.clone();
    odd[levels] = 7;
    assert_eq!(
        parse(Episode::Keen4, &odd),
        Err(ParseError::Implausible("levels"))
    );
    let mut odd = data.clone();
    odd[0x7C] = 26; // map
    assert_eq!(
        parse(Episode::Keen4, &odd),
        Err(ParseError::Implausible("map"))
    );
    assert_eq!(parse(Episode::Keen5, &data), Err(ParseError::BadSignature));
}

/// Return to the Shadowlands 1.8: a Normal game saved inside Dunesville
/// (map 1), and one saved on the world map after finishing it.
#[test]
fn return_to_the_shadowlands() {
    let data = fixture("rtts/SAVEGAM1.CK4");
    assert_eq!(
        parse(Episode::Keen4, &data).unwrap(),
        SaveInfo {
            name: Some("test22".into()),
            score: Some(800),
            lives: Some(3),
            ammo: Some(0),
            levels_done: Some(1),
            difficulty: Some(Difficulty::Normal),
            on_world_map: Some(true),
            ship_parts: None,
        }
    );
    assert_eq!(
        parse(Episode::Keen4, &fixture("rtts/SAVEGAM0.CK4")).unwrap(),
        SaveInfo {
            name: Some("test1".into()),
            score: Some(600),
            lives: Some(3),
            ammo: Some(4),
            levels_done: Some(0),
            difficulty: Some(Difficulty::Normal),
            on_world_map: Some(false),
            ship_parts: None,
        }
    );

    // The rocket the game starts on, marked done, doesn't count.
    let levels = 41 + 4;
    assert_eq!(data[levels + 2 * 20], 1);

    // Anything the game can't have written falls back to Keen 4's layout.
    let mut odd = data.clone();
    odd[0x7F] = 9; // difficulty
    assert_ne!(parse(Episode::Keen4, &odd).unwrap().score, Some(800));
}

/// Keen Dreams Gold Edition: Easy games, one saved on the map a new game
/// starts on (22), one later with two flower powers.
#[test]
fn keen_dreams_gold_edition() {
    assert_eq!(
        parse(Episode::Keen4, &fixture("KDGOLD0.SAV")).unwrap(),
        SaveInfo {
            name: Some("save 1".into()),
            score: Some(0),
            lives: Some(3),
            ammo: Some(0),
            levels_done: Some(0),
            difficulty: Some(Difficulty::Easy),
            on_world_map: None,
            ship_parts: None,
        }
    );
    let data = fixture("KDGOLD2.SAV");
    assert_eq!(
        parse(Episode::Keen4, &data).unwrap(),
        SaveInfo {
            name: Some("ssave 3".into()),
            score: Some(200),
            lives: Some(3),
            ammo: Some(2),
            levels_done: Some(0),
            difficulty: Some(Difficulty::Easy),
            on_world_map: None,
            ship_parts: None,
        }
    );

    let (levels, map, difficulty) = (54, 109, 111);
    // A level done counts, the start maps don't.
    let mut later = data.clone();
    later[levels + 3] = 1;
    later[levels + 22] = 1;
    assert_eq!(parse(Episode::Keen4, &later).unwrap().levels_done, Some(1));
    // On the maps 23-46 the score box shows the ammo instead.
    let mut night = data.clone();
    night[map] = 30;
    night[103] = 7;
    assert_eq!(parse(Episode::Keen4, &night).unwrap().ammo, Some(7));
    // Its own Keen and Grand.
    let mut own = data.clone();
    own[difficulty] = 4;
    assert_eq!(
        parse(Episode::Keen4, &own).unwrap().difficulty,
        Some(Difficulty::Keen)
    );
    own[difficulty] = 5;
    assert_eq!(
        parse(Episode::Keen4, &own).unwrap().difficulty,
        Some(Difficulty::Grand)
    );

    // Anything the game can't have written is refused.
    let mut odd = data.clone();
    odd[difficulty] = 9;
    assert_eq!(
        parse(Episode::Keen4, &odd),
        Err(ParseError::Implausible("difficulty"))
    );
    let mut odd = data.clone();
    odd[map] = 48;
    assert_eq!(
        parse(Episode::Keen4, &odd),
        Err(ParseError::Implausible("map"))
    );
    let mut odd = data.clone();
    odd[levels] = 2;
    assert_eq!(
        parse(Episode::Keen4, &odd),
        Err(ParseError::Implausible("levels"))
    );
    assert_eq!(parse(Episode::Keen5, &data), Err(ParseError::BadSignature));
}

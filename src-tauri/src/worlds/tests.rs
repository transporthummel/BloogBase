use std::collections::BTreeMap;
use std::fs;

use super::*;
use crate::backups;
use crate::library;
use crate::saves::tests::keen4_save;
use crate::source::testutil::*;

const WORLDS: &str = r#"{
    "new": {"type": "batch", "file": "GEN.BAT"},
    "play": {"type": "patch", "patcher": "CK4PATCH.EXE", "patch": "WORLD.PAT", "args": []},
    "files": ["WLEVEL*.CK4", "WORLD.PAT"],
    "seed": {"arg": "/SEED", "max": 65535, "read": {"file": "WORLD.PAT", "after": "Seed: "}},
    "options": [
        {"id": "hard", "arg": "/HARD", "type": "flag", "label": {"en": "Hard"}},
        {"id": "lives", "arg": "/LIVES", "type": "number", "min": 1, "max": 9, "default": 3,
         "label": {"en": "Lives"}}
    ]
}"#;

struct T {
    _tmp: tempfile::TempDir,
    paths: AppPaths,
    db: Db,
    profile: String,
    state: PathBuf,
}

/// Keen 4 with a mod whose `GEN.BAT` generates worlds.
fn setup() -> T {
    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(tmp.path().join("data"));
    paths.ensure().unwrap();
    let db = Db::open_in_memory().unwrap();
    let src = tmp.path().join("k4");
    write_folder(&src, KEEN4);
    library::import(&paths, &db, &src).unwrap();
    let mod_src = tmp.path().join("gen");
    write_folder(
        &mod_src,
        &[("GEN.BAT", b"GEN %1 %2\r\n"), ("CK4PATCH.EXE", b"MZ")],
    );
    let m = crate::mods::install(
        &paths,
        &db,
        &crate::mods::InstallModRequest {
            path: mod_src.display().to_string(),
            name: "Generator".into(),
            recipe: Recipe::Batch {
                file: "GEN.BAT".into(),
            },
            install_id: Some("keen4-1".into()),
            keenwiki: None,
            extends: None,
            replace: None,
        },
    )
    .unwrap();
    db.conn()
        .execute(
            "UPDATE mods SET worlds_json = ?2 WHERE id = ?1",
            params![m.id, WORLDS],
        )
        .unwrap();
    let state = profiles::get(&db, &m.profile_id).unwrap().1.upper;
    fs::create_dir_all(&state).unwrap();
    T {
        _tmp: tmp,
        paths,
        db,
        profile: m.profile_id,
        state,
    }
}

impl T {
    fn prepare(&self, start: Option<WorldStart>) -> Result<Option<Run>> {
        prepare(&self.paths, &self.db, &self.profile, start)
    }

    /// What the generator writes, with `seed` in its patch file.
    fn generate(&self, seed: u32) {
        fs::write(self.state.join("WLEVEL01.CK4"), format!("level of {seed}")).unwrap();
        fs::write(
            self.state.join("WORLD.PAT"),
            format!("%patch $1 \"Seed: {seed}\\n\"\n"),
        )
        .unwrap();
    }

    /// Starts a new world that generates `seed`, plays it and saves.
    fn new_world(&self, seed: u32, save: &str) -> u32 {
        let run = self.prepare(Some(new(None))).unwrap().unwrap();
        self.generate(seed);
        fs::write(self.state.join("SAVEGAM0.CK4"), keen4_save(save)).unwrap();
        after_session(
            &self.paths,
            &self.db,
            &self.profile,
            run.new.as_ref().unwrap(),
        )
        .unwrap()
        .unwrap()
    }

    fn list(&self) -> ProfileWorlds {
        list(&self.paths, &self.db, &self.profile).unwrap().unwrap()
    }

    fn state_has(&self, name: &str) -> bool {
        self.state.join(name).exists()
    }

    fn read(&self, name: &str) -> String {
        String::from_utf8_lossy(&fs::read(self.state.join(name)).unwrap()).into_owned()
    }
}

fn new(seed: Option<u32>) -> WorldStart {
    WorldStart::New {
        seed,
        options: BTreeMap::new(),
    }
}

#[test]
fn profiles_without_worlds_launch_as_before() {
    let t = setup();
    assert!(prepare(&t.paths, &t.db, "keen4-1", None).unwrap().is_none());
    assert!(list(&t.paths, &t.db, "keen4-1").unwrap().is_none());
}

#[test]
fn a_new_world_runs_the_generator_with_seed_and_options() {
    let t = setup();
    let mut options = BTreeMap::new();
    options.insert("hard".to_string(), 1);
    let run = t
        .prepare(Some(WorldStart::New {
            seed: Some(12345),
            options,
        }))
        .unwrap()
        .unwrap();
    assert_eq!(run.command, "GEN.BAT");
    assert_eq!(run.args, ["/SEED", "12345", "/HARD", "/LIVES", "3"]);
    let new = run.new.unwrap();
    assert_eq!(new.seed, Some(12345));
    assert_eq!(new.options["lives"], 3);
    // Remembered for the next one.
    assert_eq!(t.list().last_options["hard"], 1);

    // Without a seed, none is passed; flags off are left out.
    let run = t.prepare(Some(new_world_plain())).unwrap().unwrap();
    assert_eq!(run.args, ["/LIVES", "3"]);
}

fn new_world_plain() -> WorldStart {
    new(None)
}

#[test]
fn seeds_and_options_out_of_range_are_refused() {
    let t = setup();
    assert!(matches!(
        t.prepare(Some(new(Some(65536)))),
        Err(Error::InvalidSetting(_))
    ));
    for (id, value) in [("lives", 10), ("lives", 0), ("hard", 2), ("other", 1)] {
        let mut options = BTreeMap::new();
        options.insert(id.to_string(), value);
        let got = t.prepare(Some(WorldStart::New {
            seed: None,
            options,
        }));
        assert!(matches!(got, Err(Error::InvalidSetting(_))), "{id}={value}");
    }
}

#[test]
fn the_world_a_session_generated_is_kept_with_the_seed_it_notes() {
    let t = setup();
    let id = t.new_world(4711, "first");
    assert_eq!(id, 1);
    let l = t.list();
    assert_eq!(l.active, Some(1));
    assert_eq!(l.worlds.len(), 1);
    let w = &l.worlds[0];
    assert_eq!((w.seed, w.active, w.saves), (Some(4711), true, 1));
    assert_eq!(w.options["lives"], 3);
    let p = profiles::get(&t.db, &t.profile).unwrap().0;
    assert!(p.worlds);
    assert_eq!(p.active_world, Some(1));
    assert_eq!(p.world_count, 1);

    // Without a play command's files, nothing is recorded.
    let run = t.prepare(Some(new(None))).unwrap().unwrap();
    fs::write(t.state.join("WLEVEL01.CK4"), "only a level").unwrap();
    let none = after_session(&t.paths, &t.db, &t.profile, run.new.as_ref().unwrap()).unwrap();
    assert_eq!(none, None);
    assert_eq!(t.list().active, None);
}

#[test]
fn worlds_switch_with_their_saves() {
    let t = setup();
    fs::write(t.state.join("CONFIG.CK4"), "settings").unwrap();
    let first = t.new_world(1, "first");
    let second = t.new_world(2, "second");
    assert_ne!(first, second);
    // The first one waits in its folder, saves and all.
    assert!(t.read("WORLD.PAT").contains("Seed: 2"));
    let parked = dir(&t.paths, &t.profile).join(first.to_string());
    assert!(parked.join("WORLD.PAT").is_file());
    assert!(parked.join("SAVEGAM0.CK4").is_file());

    let run = t
        .prepare(Some(WorldStart::Continue { id: first }))
        .unwrap()
        .unwrap();
    assert_eq!(run.command, "CK4PATCH.EXE WORLD.PAT");
    assert!(run.args.is_empty() && run.new.is_none());
    assert!(t.read("WORLD.PAT").contains("Seed: 1"));
    assert!(t.read("SAVEGAM0.CK4").contains("first"));
    assert!(!parked.exists());
    // Settings stay with the profile.
    assert_eq!(t.read("CONFIG.CK4"), "settings");
    let l = t.list();
    assert_eq!(l.active, Some(first));
    assert_eq!(l.worlds.iter().map(|w| w.saves).collect::<Vec<_>>(), [1, 1]);
    // The library counts the saves of all of them, e.g. for removing the mod.
    let p = profiles::list(&t.db, |_| false).unwrap();
    let p = p.iter().find(|p| p.id == t.profile).unwrap();
    assert_eq!(p.save_count, 2);

    // Without a choice, the world played last goes on.
    let run = t.prepare(None).unwrap().unwrap();
    assert!(run.new.is_none());
    assert!(t.read("WORLD.PAT").contains("Seed: 1"));
}

#[test]
fn continuing_a_world_whose_files_are_gone_fails() {
    let t = setup();
    let id = t.new_world(1, "first");
    fs::remove_file(t.state.join("WORLD.PAT")).unwrap();
    assert!(matches!(
        t.prepare(Some(WorldStart::Continue { id })),
        Err(Error::WorldIncomplete)
    ));
    assert!(matches!(
        t.prepare(Some(WorldStart::Continue { id: 99 })),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn saves_from_before_worlds_go_into_a_backup() {
    let t = setup();
    fs::write(t.state.join("SAVEGAM3.CK4"), keen4_save("old")).unwrap();
    fs::write(t.state.join("WORLD.PAT"), "stale").unwrap();
    fs::write(t.state.join("CONFIG.CK4"), "settings").unwrap();
    t.prepare(Some(new(None))).unwrap();
    assert!(!t.state_has("SAVEGAM3.CK4") && !t.state_has("WORLD.PAT"));
    assert!(t.state_has("CONFIG.CK4"));
    let b = backups::list(&t.db, &t.profile).unwrap();
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].reason, Some(Reason::BeforeWorlds));
    assert_eq!(b[0].files, ["SAVEGAM3.CK4"]);
    assert_eq!(b[0].world, None);
}

#[test]
fn a_switch_cut_short_is_finished_next_time() {
    let t = setup();
    let first = t.new_world(1, "first");
    let second = t.new_world(2, "second");
    // As a crash leaves it: the journal written, half of the second world
    // moved out.
    let to_dir = dir(&t.paths, &t.profile).join(second.to_string());
    fs::create_dir_all(&to_dir).unwrap();
    fs::rename(t.state.join("WORLD.PAT"), to_dir.join("WORLD.PAT")).unwrap();
    write_journal(
        &t.paths,
        &t.profile,
        &Swap {
            from: Some(second),
            to: Some(first),
            parked: false,
        },
    )
    .unwrap();

    // Listing only reads.
    assert_eq!(t.list().active, Some(second));
    t.prepare(Some(WorldStart::Continue { id: first })).unwrap();
    assert_eq!(t.list().active, Some(first));
    assert!(t.read("WORLD.PAT").contains("Seed: 1"));
    assert!(to_dir.join("WORLD.PAT").is_file() && to_dir.join("WLEVEL01.CK4").is_file());
    assert!(!dir(&t.paths, &t.profile).join(JOURNAL).exists());
}

#[test]
fn deleting_a_world_backs_it_up_first() {
    let t = setup();
    let first = t.new_world(1, "first");
    let second = t.new_world(2, "second");

    delete(&t.paths, &t.db, &t.profile, first).unwrap();
    assert!(!dir(&t.paths, &t.profile).join(first.to_string()).exists());
    let b = &backups::list(&t.db, &t.profile).unwrap()[0];
    assert_eq!(
        (b.reason, b.world),
        (Some(Reason::WorldDeleted), Some(first))
    );
    assert!(b.files.contains(&"SAVEGAM0.CK4".to_string()));

    // The one played leaves `state/`; none is played then.
    delete(&t.paths, &t.db, &t.profile, second).unwrap();
    assert!(!t.state_has("WORLD.PAT") && !t.state_has("SAVEGAM0.CK4"));
    let l = t.list();
    assert_eq!((l.active, l.worlds.len()), (None, 0));
    assert_eq!(
        backups::list(&t.db, &t.profile).unwrap()[0].world,
        Some(second)
    );
    // A new world doesn't take an id backups still name.
    assert_eq!(t.new_world(3, "third"), 3);
}

#[test]
fn rename_and_favourites() {
    let t = setup();
    let first = t.new_world(1, "first");
    let second = t.new_world(2, "second");
    assert_eq!(t.list().worlds[0].id, second);
    set_favourite(&t.db, &t.profile, first, true).unwrap();
    rename(&t.db, &t.profile, first, Some("  Mine ")).unwrap();
    let w = &t.list().worlds[0];
    assert_eq!(
        (w.id, w.name.as_deref(), w.favourite),
        (first, Some("Mine"), true)
    );
    rename(&t.db, &t.profile, first, Some(" ")).unwrap();
    assert_eq!(t.list().worlds[0].name, None);
    assert!(rename(&t.db, &t.profile, 9, None).is_err());
}

#[test]
fn restoring_a_backup_of_another_world_plays_that_world_again() {
    let t = setup();
    let first = t.new_world(1, "first");
    let made = backups::create(
        &t.paths,
        &t.db,
        &t.profile,
        Contents::State,
        backups::Origin::User(None),
    )
    .unwrap()
    .unwrap();
    assert_eq!(made.world, Some(first));
    t.new_world(2, "second");

    backups::restore(&t.paths, &t.db, made.id, None, None, true).unwrap();
    assert_eq!(t.list().active, Some(first));
    assert!(t.read("WORLD.PAT").contains("Seed: 1"));
    assert!(t.read("SAVEGAM0.CK4").contains("first"));

    // A deleted world's files never replace the ones played.
    delete(&t.paths, &t.db, &t.profile, first).unwrap();
    let third = t.new_world(3, "third");
    backups::restore(&t.paths, &t.db, made.id, None, None, true).unwrap();
    assert_eq!(t.list().active, Some(third));
    assert!(t.read("WORLD.PAT").contains("Seed: 3"));
}

#[test]
fn numbers_after_a_text() {
    assert_eq!(number_after("x \"Seed: 123\\n\"", "Seed: "), Some(123));
    assert_eq!(number_after("Seed: x", "Seed: "), None);
    assert_eq!(number_after("nothing", "Seed: "), None);
}

#[test]
fn removing_the_mod_with_its_saves_keeps_its_worlds() {
    let t = setup();
    let first = t.new_world(1, "first");
    let second = t.new_world(2, "second");
    set_favourite(&t.db, &t.profile, first, true).unwrap();
    let mod_id: String =
        t.db.conn()
            .query_row(
                "SELECT mod_id FROM profiles WHERE id = ?1",
                [&t.profile],
                |r| r.get(0),
            )
            .unwrap();
    let mod_dir = t.paths.mods.join(&mod_id).join("files");
    let src = t._tmp.path().join("again");
    fs::create_dir_all(&src).unwrap();
    for name in ["GEN.BAT", "CK4PATCH.EXE"] {
        fs::copy(mod_dir.join(name), src.join(name)).unwrap();
    }
    crate::mods::delete(&t.paths, &t.db, &mod_id, true).unwrap();

    let m = crate::mods::install(
        &t.paths,
        &t.db,
        &crate::mods::InstallModRequest {
            path: src.display().to_string(),
            name: "Generator".into(),
            recipe: Recipe::Batch {
                file: "GEN.BAT".into(),
            },
            install_id: Some("keen4-1".into()),
            keenwiki: None,
            extends: None,
            replace: None,
        },
    )
    .unwrap();
    assert_eq!(m.profile_id, t.profile);
    t.db.conn()
        .execute(
            "UPDATE mods SET worlds_json = ?2 WHERE id = ?1",
            params![m.id, WORLDS],
        )
        .unwrap();
    let offer = backups::earlier(&t.db, &t.profile).unwrap().unwrap();
    backups::answer_earlier(&t.paths, &t.db, &t.profile, offer.backup.id, true).unwrap();

    let l = t.list();
    assert_eq!(l.active, Some(second));
    assert_eq!(l.worlds.len(), 2);
    assert!(
        l.worlds
            .iter()
            .any(|w| w.id == first && w.favourite && w.saves == 1)
    );
    assert!(t.read("WORLD.PAT").contains("Seed: 2"));
    assert!(!t.state_has("bloogbase-worlds"));
    t.prepare(Some(WorldStart::Continue { id: first })).unwrap();
    assert!(t.read("SAVEGAM0.CK4").contains("first"));
}

#[test]
fn saves_from_before_worlds_outlive_pruning() {
    let t = setup();
    fs::write(t.state.join("SAVEGAM3.CK4"), keen4_save("old")).unwrap();
    t.prepare(Some(new(None))).unwrap();
    backups::set_settings(
        &t.db,
        &backups::BackupSettings {
            auto: true,
            keep: 1,
        },
    )
    .unwrap();
    for i in 0..3 {
        fs::write(t.state.join("SAVEGAM0.CK4"), keen4_save(&format!("s{i}"))).unwrap();
        backups::create(
            &t.paths,
            &t.db,
            &t.profile,
            Contents::State,
            Reason::BeforeRestore,
        )
        .unwrap();
    }
    let kept = backups::list(&t.db, &t.profile).unwrap();
    assert!(kept.iter().any(|b| b.reason == Some(Reason::BeforeWorlds)));
}

#[test]
fn a_world_made_current_brings_its_saves_without_a_start() {
    let t = setup();
    let first = t.new_world(1, "first");
    t.new_world(2, "second");
    let played = t
        .list()
        .worlds
        .iter()
        .find(|w| w.id == first)
        .unwrap()
        .last_played_at;
    activate(&t.paths, &t.db, &t.profile, first).unwrap();
    assert!(t.read("SAVEGAM0.CK4").contains("first"));
    let l = t.list();
    assert_eq!(l.active, Some(first));
    let w = l.worlds.iter().find(|w| w.id == first).unwrap();
    assert_eq!(w.last_played_at, played);
    assert!(matches!(
        activate(&t.paths, &t.db, &t.profile, 99),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn a_seed_and_options_already_played_continue_that_world() {
    let t = setup();
    let first = t.new_world(4711, "first");
    t.new_world(2, "second");
    // The default options, given or not, are the same world.
    let mut options = BTreeMap::new();
    options.insert("lives".to_string(), 3);
    let run = t
        .prepare(Some(WorldStart::New {
            seed: Some(4711),
            options,
        }))
        .unwrap()
        .unwrap();
    assert!(run.new.is_none());
    assert_eq!(run.command, "CK4PATCH.EXE WORLD.PAT");
    assert!(t.read("SAVEGAM0.CK4").contains("first"));
    assert_eq!(t.list().active, Some(first));

    // Other options make another world.
    let mut options = BTreeMap::new();
    options.insert("hard".to_string(), 1);
    let run = t
        .prepare(Some(WorldStart::New {
            seed: Some(4711),
            options,
        }))
        .unwrap()
        .unwrap();
    assert!(run.new.is_some());
}

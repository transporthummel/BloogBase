use std::fs;

use super::*;
use crate::backups::{self, Contents};
use crate::library;
use crate::mod_index::IndexEntry;
use crate::mods::{self, Curated};
use crate::profiles;
use crate::saves::tests::keen4_save;
use crate::slots::{JOURNAL, Swap, write_journal};
use crate::source::testutil::*;

struct T {
    tmp: tempfile::TempDir,
    paths: AppPaths,
    db: Db,
    profile: String,
    state: PathBuf,
}

const ENTRY: &str = r#"{
    "id": "levels", "name": "Levels", "game": "keen4", "requires": [], "authors": ["A"],
    "added": "2026-11-20", "type": "episode", "links": {},
    "downloads": [{"url": "https://example.com/levels.zip", "sha256": "00", "size": 1}],
    "recipe": {"type": "patch", "patcher": "CK4PATCH.EXE", "patch": "x.pat", "args": []},
    "variants": [
        {"id": "hard", "default": true, "name": "Hard", "root": "HARD"},
        {"id": "easy", "name": "Easy", "root": "EASY"}
    ]
}"#;

const ARCHIVE: &[(&str, &[u8])] = &[
    ("EASY/CK4PATCH.EXE", b"patcher"),
    ("EASY/x.pat", b"%ext ck4\n%version 1.4\n"),
    ("EASY/LEVEL01.CK4", b"easy"),
    ("HARD/CK4PATCH.EXE", b"patcher"),
    ("HARD/x.pat", b"%ext ck4\n%version 1.4\n"),
    ("HARD/LEVEL01.CK4", b"hard"),
];

/// Like [`ARCHIVE`], with a save that comes with Easy.
const SHIPPED: &[(&str, &[u8])] = &[
    ("EASY/CK4PATCH.EXE", b"patcher"),
    (
        "EASY/x.pat",
        b"%ext ck4
%version 1.4
",
    ),
    ("EASY/LEVEL01.CK4", b"easy"),
    ("EASY/SAVEGAM1.CK4", b"the author's save"),
    ("HARD/CK4PATCH.EXE", b"patcher"),
    (
        "HARD/x.pat",
        b"%ext ck4
%version 1.4
",
    ),
    ("HARD/LEVEL01.CK4", b"hard"),
];

/// Keen 4 with a mod in two variants, Hard (the default) and Easy.
fn setup() -> T {
    setup_with(ARCHIVE)
}

fn setup_with(archive: &[(&str, &[u8])]) -> T {
    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(tmp.path().join("data"));
    paths.ensure().unwrap();
    let db = Db::open_in_memory().unwrap();
    let src = tmp.path().join("k4");
    write_folder(&src, KEEN4);
    library::import(&paths, &db, &src).unwrap();
    let t = T {
        state: PathBuf::new(),
        profile: String::new(),
        tmp,
        paths,
        db,
    };
    let profile = t.install(archive);
    let state = profiles::get(&t.db, &profile).unwrap().1.upper;
    fs::create_dir_all(&state).unwrap();
    T {
        profile,
        state,
        ..t
    }
}

impl T {
    fn install(&self, archive: &[(&str, &[u8])]) -> String {
        let zip = self.tmp.path().join("levels.zip");
        write_zip(&zip, archive);
        let entry: IndexEntry = serde_json::from_str(ENTRY).unwrap();
        let m = mods::install_curated(
            &self.paths,
            &self.db,
            &zip,
            &Curated::new(&entry, 0),
            None,
            false,
        )
        .unwrap();
        m.profile_id
    }

    fn list(&self) -> ProfileVariants {
        list(&self.paths, &self.db, &self.profile).unwrap().unwrap()
    }

    fn save(&self, text: &str) {
        fs::write(self.state.join("SAVEGAM0.CK4"), keen4_save(text)).unwrap();
    }

    fn saved(&self) -> Option<String> {
        fs::read(self.state.join("SAVEGAM0.CK4"))
            .ok()
            .map(|d| String::from_utf8_lossy(&d).into_owned())
    }

    fn switch(&self, id: &str) {
        switch(&self.paths, &self.db, &self.profile, id).unwrap();
    }
}

#[test]
fn saves_belong_to_a_variant_and_settings_to_the_mod() {
    let t = setup();
    let l = t.list();
    assert_eq!(l.active.as_deref(), Some("hard"));
    assert!(!l.chosen);
    assert_eq!(l.variants.len(), 2);
    t.save("hard save");
    fs::write(t.state.join("CONFIG.CK4"), b"keys").unwrap();

    t.switch("easy");
    assert_eq!(t.saved(), None);
    assert!(t.state.join("CONFIG.CK4").is_file());
    let (profile, layers) = profiles::get(&t.db, &t.profile).unwrap();
    assert_eq!(profile.active_variant.unwrap().id, "easy");
    assert!(layers.mod_layer.unwrap().files.ends_with("variants/easy"));
    let l = t.list();
    assert_eq!(
        l.variants
            .iter()
            .map(|v| (v.active, v.saves))
            .collect::<Vec<_>>(),
        [(false, 1), (true, 0)]
    );
    // The library card counts the saves of all of them.
    let p = profiles::list(&t.db, |_| false, |_| false).unwrap();
    assert_eq!(p.iter().find(|p| p.id == t.profile).unwrap().save_count, 1);

    t.save("easy save");
    t.switch("hard");
    assert!(t.saved().unwrap().contains("hard save"));
    t.switch("easy");
    assert!(t.saved().unwrap().contains("easy save"));
    assert!(switch(&t.paths, &t.db, &t.profile, "medium").is_err());

    set_chosen(&t.db, &t.profile).unwrap();
    assert!(t.list().chosen);
}

#[test]
fn saves_a_variant_comes_with_count_as_its_own() {
    let t = setup_with(SHIPPED);
    let counts = |t: &T| {
        let card = profiles::list(&t.db, |_| false, |_| false)
            .unwrap()
            .into_iter()
            .find(|p| p.id == t.profile)
            .unwrap()
            .save_count;
        let rows = t
            .list()
            .variants
            .iter()
            .map(|v| (v.active, v.saves))
            .collect();
        (rows, card)
    };
    // Easy's save counts while Hard is played, and while Easy is.
    assert_eq!(counts(&t), (vec![(true, 0), (false, 1)], 1));
    t.switch("easy");
    assert_eq!(counts(&t), (vec![(false, 0), (true, 1)], 1));

    // Deleted, it stays gone when another variant is played.
    let marker = "DBOVERLAY_DEL_SAVEGAM1.CK4";
    fs::write(t.state.join(marker), b"").unwrap();
    assert_eq!(counts(&t), (vec![(false, 0), (true, 0)], 0));
    t.switch("hard");
    assert!(!t.state.join(marker).exists());
    assert_eq!(counts(&t), (vec![(true, 0), (false, 0)], 0));
    t.switch("easy");
    assert!(t.state.join(marker).is_file());
}

#[test]
fn a_switch_cut_short_is_finished_next_time() {
    let t = setup();
    t.save("hard save");
    // As a crash leaves it: the journal written, nothing moved yet.
    write_journal(
        &dir(&t.paths, &t.profile),
        &Swap {
            from: Some("hard".to_string()),
            to: Some("easy".to_string()),
            parked: false,
        },
    )
    .unwrap();
    recover(&t.paths, &t.db, &t.profile).unwrap();
    assert_eq!(t.list().active.as_deref(), Some("easy"));
    assert_eq!(t.saved(), None);
    assert!(!dir(&t.paths, &t.profile).join(JOURNAL).exists());
    t.switch("hard");
    assert!(t.saved().unwrap().contains("hard save"));
}

#[test]
fn restoring_a_backup_of_another_variant_plays_that_one_again() {
    let t = setup();
    t.save("hard save");
    let made = backups::create(
        &t.paths,
        &t.db,
        &t.profile,
        Contents::State,
        backups::Origin::User(None),
    )
    .unwrap()
    .unwrap();
    assert_eq!(made.variant.as_deref(), Some("hard"));
    t.switch("easy");
    t.save("easy save");

    backups::restore(&t.paths, &t.db, made.id, None, None, true).unwrap();
    assert_eq!(t.list().active.as_deref(), Some("hard"));
    assert!(t.saved().unwrap().contains("hard save"));
    t.switch("easy");
    assert!(t.saved().unwrap().contains("easy save"));
}

#[test]
fn removing_the_mod_with_its_saves_keeps_every_variant() {
    let t = setup();
    t.save("hard save");
    t.switch("easy");
    t.save("easy save");
    let mod_id = profiles::get(&t.db, &t.profile).unwrap().0.mod_id.unwrap();
    mods::delete(&t.paths, &t.db, &mod_id, true).unwrap();
    assert!(!dir(&t.paths, &t.profile).exists());

    assert_eq!(t.install(ARCHIVE), t.profile);
    assert_eq!(t.list().active.as_deref(), Some("hard"));
    let offer = backups::earlier(&t.db, &t.profile).unwrap().unwrap();
    backups::answer_earlier(&t.paths, &t.db, &t.profile, offer.backup.id, true).unwrap();
    assert_eq!(t.list().active.as_deref(), Some("easy"));
    assert!(t.saved().unwrap().contains("easy save"));
    assert!(!t.state.join(ARCHIVE_DIR).exists());
    t.switch("hard");
    assert!(t.saved().unwrap().contains("hard save"));
}

/// Like [`SHIPPED`], with Easy's program saving as `.EZ4`.
const OWN_EXTENSION: &[(&str, &[u8])] = &[
    ("EASY/CK4PATCH.EXE", b"patcher"),
    (
        "EASY/x.pat",
        b"%ext ck4
%version 1.4
",
    ),
    ("EASY/EASY.EXE", b"...SAVEGAM0.EZ4..."),
    ("EASY/LEVEL01.CK4", b"easy"),
    ("EASY/SAVEGAM1.EZ4", b"the author's save"),
    ("HARD/CK4PATCH.EXE", b"patcher"),
    (
        "HARD/x.pat",
        b"%ext ck4
%version 1.4
",
    ),
    ("HARD/LEVEL01.CK4", b"hard"),
];

#[test]
fn a_variant_naming_its_saves_its_own_way_counts_them() {
    let t = setup_with(OWN_EXTENSION);
    t.save("hard save");
    let rows = |t: &T| -> Vec<(bool, u32)> {
        t.list()
            .variants
            .iter()
            .map(|v| (v.active, v.saves))
            .collect()
    };
    let card = |t: &T| {
        profiles::list(&t.db, |_| false, |_| false)
            .unwrap()
            .into_iter()
            .find(|p| p.id == t.profile)
            .unwrap()
            .save_count
    };
    assert_eq!(rows(&t), vec![(true, 1), (false, 1)]);
    assert_eq!(card(&t), 2);
    t.switch("easy");
    assert_eq!(rows(&t), vec![(false, 1), (true, 1)]);
    assert_eq!(card(&t), 2);
}

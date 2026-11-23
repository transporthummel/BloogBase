use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::backups::{self, Contents};
use crate::catalog;
use crate::db::Db;
use crate::detect::sha256_hex;
use crate::error::Error;
use crate::keenwiki::WikiMod;
use crate::library::{self, Install};
use crate::mod_index::ModStatus;
use crate::paths::AppPaths;
use crate::recipe::{self, Recipe, VersionRecipes};
use crate::source::Source;

use super::analyze::{analyze, decide, find_root, version_ids};
use super::fit::fit_rank;
use super::install::{check_install, slug};
use super::metadata::{guess_metadata, title_case};
use super::*;
use crate::profiles;
use crate::source::testutil::*;

fn setup() -> (tempfile::TempDir, AppPaths, Db) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(tmp.path().join("data"));
    paths.ensure().unwrap();
    let db = Db::open_in_memory().unwrap();
    let src = tmp.path().join("k4");
    write_folder(&src, KEEN4);
    library::import(&paths, &db, &src).unwrap();
    (tmp, paths, db)
}

const AURORA: &[(&str, &[u8])] = &[
    (
        "Aurora/play.bat",
        b"@echo off\r\nCK4PATCH.EXE abiathar.pat\r\n",
    ),
    ("Aurora/CK4PATCH.EXE", b"patcher"),
    ("Aurora/abiathar.pat", b"%ext ck4\n%version 1.4\n"),
    ("Aurora/GAMEMAPS.CK4", b"new maps"),
    ("Aurora/READ ME.txt", b"hi"),
    ("Aurora/Thumbs.db", b""),
    ("__MACOSX/Aurora/._play.bat", b""),
    ("info.txt", b"outside the mod"),
];

fn request(path: &Path, recipe: Recipe) -> InstallModRequest {
    InstallModRequest {
        path: path.display().to_string(),
        name: String::new(),
        recipe,
        install_id: Some("keen4-1".into()),
        keenwiki: None,
        extends: None,
        replace: None,
    }
}

#[test]
fn inspects_zip_with_folder_and_metadata_from_name() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp
        .path()
        .join("m.dave216.aurora_bliss_v.1.2.k4.2023-08-23.zip");
    write_zip(&zip, AURORA);
    let info = inspect(&zip, None).unwrap();
    assert_eq!(info.name, "Aurora Bliss");
    assert_eq!(info.authors, ["dave216"]);
    assert_eq!(info.version.as_deref(), Some("1.2"));
    assert_eq!(info.released.as_deref(), Some("2023-08-23"));
    assert_eq!(info.game_id, "keen4");
    assert!(info.confident);
    assert_eq!(info.file_count, 5);
    assert_eq!(info.save_count, 0);
    assert_eq!(
        info.recipes[0],
        RecipeOption {
            recipe: Recipe::Patch {
                patcher: "CK4PATCH.EXE".into(),
                patch: "abiathar.pat".into(),
                args: vec![]
            },
            origin: "play.bat".into(),
            game_versions: vec!["1.4".into()],
            version_ids: vec!["keen4-1.4-ega".into()],
            missing_files: vec![],
            base_mod: None,
            made_for: None,
        }
    );
}

#[test]
fn installs_mod_with_profile_and_removes_it_keeping_saves() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("aurora.zip");
    write_zip(&zip, AURORA);
    let info = inspect(&zip, None).unwrap();
    assert_eq!(info.name, "Aurora");

    // The fake Keen 4 has an unknown version, which is allowed.
    let m = install(&paths, &db, &request(&zip, info.recipes[0].recipe.clone())).unwrap();
    assert_eq!((m.id.as_str(), m.profile_id.as_str()), ("aurora", "aurora"));
    let files = paths.mods.join("aurora/files");
    assert!(files.join("GAMEMAPS.CK4").is_file());
    assert!(files.join("READ ME.txt").is_file());
    assert!(!files.join("Thumbs.db").exists());
    assert!(!paths.mods.join("aurora/files/info.txt").exists());
    assert!(paths.mods.join("aurora/source/aurora.zip").is_file());
    assert!(paths.profiles.join("aurora/state").is_dir());
    assert_eq!(list(&db).unwrap().len(), 1);
    // Linked to the game it runs on; the game itself to nothing.
    assert_eq!(
        profiles::get(&db, "aurora")
            .unwrap()
            .0
            .game_profile_id
            .as_deref(),
        Some("keen4-1")
    );
    assert_eq!(
        profiles::get(&db, "keen4-1").unwrap().0.game_profile_id,
        None
    );
    assert_eq!(fs::read_dir(&paths.cache).unwrap().count(), 0);

    // Same name again gets its own id.
    let second = install(&paths, &db, &request(&zip, info.recipes[0].recipe.clone())).unwrap();
    assert_eq!(second.id, "aurora-2");

    fs::write(paths.profiles.join("aurora/state/SAVEGAM0.CK4"), b"save").unwrap();
    // An automatic backup goes with the profile, one the player made
    // stays with the saves; the one made for the removal is made even
    // though the automatic one holds the same.
    backups::before_launch(&paths, &db, "aurora").unwrap();
    let mine = backups::create(
        &paths,
        &db,
        "aurora",
        Contents::State,
        backups::Origin::User(Some("mine")),
    )
    .unwrap()
    .unwrap();
    delete(&paths, &db, "aurora", true).unwrap();
    assert!(!paths.mods.join("aurora").exists());
    assert!(!paths.profiles.join("aurora").exists());
    let labels: Vec<Option<String>> = db
        .conn()
        .prepare("SELECT label FROM save_backups WHERE profile_id = 'aurora' ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        labels,
        [
            Some("mine".to_string()),
            Some("bloogbase:mod-removed".to_string())
        ]
    );
    let (archive, files): (String, String) = db
        .conn()
        .query_row(
            "SELECT archive_path, files_json FROM save_backups
             WHERE profile_id = 'aurora' AND id != ?1",
            [mine.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(files, r#"["SAVEGAM0.CK4"]"#);
    let mut backup = zip::ZipArchive::new(fs::File::open(archive).unwrap()).unwrap();
    assert_eq!(backup.by_index(0).unwrap().name().unwrap(), "SAVEGAM0.CK4");
    assert_eq!(list(&db).unwrap().len(), 1);

    // Without keeping saves, the player's backups go too.
    fs::write(paths.profiles.join("aurora-2/state/SAVEGAM1.CK4"), b"save").unwrap();
    backups::create(
        &paths,
        &db,
        "aurora-2",
        Contents::State,
        backups::Origin::User(None),
    )
    .unwrap()
    .unwrap();
    delete(&paths, &db, "aurora-2", false).unwrap();
    assert!(list(&db).unwrap().is_empty());
    let left: u32 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM save_backups WHERE profile_id = 'aurora-2'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0);
    assert!(matches!(
        delete(&paths, &db, "aurora-2", false),
        Err(Error::NotFound(_))
    ));

    // Installed again, it gets its id back and is offered the saves.
    let again = install(&paths, &db, &request(&zip, info.recipes[0].recipe.clone())).unwrap();
    assert_eq!(again.profile_id, "aurora");
    let offer = backups::earlier(&db, "aurora").unwrap().unwrap();
    assert_eq!(offer.backup.reason, Some(backups::Reason::ModRemoved));
    assert!(backups::earlier(&db, "keen4-1").unwrap().is_none());
}

#[test]
fn keenwiki_install_keeps_the_page_and_can_be_suggested() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("aurora.zip");
    // A patch without `%ext` and no patcher: nothing names the episode.
    write_zip(
        &zip,
        &[(
            "Aurora/MOD.PAT",
            b"%version 1.4
",
        )],
    );
    assert!(matches!(inspect(&zip, None), Err(Error::NoModFound)));
    let info = inspect(&zip, Some("keen4")).unwrap();
    assert_eq!(info.game_id, "keen4");

    let page = "https://keenwiki.shikadi.net/wiki/Aurora_Bliss";
    let download = "https://files.shikadi.net/keenwiki/mods/Keen4/aurora.zip";
    let mut req = request(&zip, info.recipes[0].recipe.clone());
    req.name = "Aurora Bliss".into();
    req.keenwiki = Some(KeenwikiOrigin {
        page_url: page.into(),
        download_url: download.into(),
        game_id: Some("keen4".into()),
        authors: vec!["Dave216".into()],
        author_links: BTreeMap::from([(
            "Dave216".into(),
            "https://keenwiki.shikadi.net/wiki/User:Dave216".into(),
        )]),
        version: Some("1.2".into()),
        description: Some("Blissful.".into()),
        released: Some("2012-05-01".into()),
        homepage: Some("http://aurora.example/".into()),
        pckf: Some("https://pckf.com/viewtopic.php?t=1".into()),
        keenmodding: None,
        status: None,
    });
    let m = install(&paths, &db, &req).unwrap();
    assert_eq!(m.source, ModSource::Keenwiki);
    assert_eq!(m.authors, ["Dave216"]);
    assert_eq!(m.version.as_deref(), Some("1.2"));
    assert_eq!(m.keenwiki_url.as_deref(), Some(page));
    let listed = &list(&db).unwrap()[0];
    assert_eq!(listed.keenwiki_url.as_deref(), Some(page));
    assert_eq!(listed.description.as_deref(), Some("Blissful."));
    assert_eq!(listed.released.as_deref(), Some("2012-05-01"));
    assert_eq!(listed.homepage.as_deref(), Some("http://aurora.example/"));
    assert_eq!(
        listed.pckf.as_deref(),
        Some("https://pckf.com/viewtopic.php?t=1")
    );
    assert_eq!(
        listed.author_links["Dave216"],
        "https://keenwiki.shikadi.net/wiki/User:Dave216"
    );

    let url = reqwest::Url::parse(&index_suggestion(&db, &m.id).unwrap()).unwrap();
    assert_eq!(
        url.as_str().split('?').next(),
        Some("https://github.com/transporthummel/BloogBase/issues/new")
    );
    let query: BTreeMap<_, _> = url.query_pairs().collect();
    assert_eq!(query["title"], "Index suggestion: Aurora Bliss");
    let body = &query["body"];
    let sha = sha256_hex(&fs::read(&zip).unwrap());
    for line in [
        "id: aurora-bliss\n",
        "name: \"Aurora Bliss\"\n",
        "game: keen4\n",
        "authors: [\"Dave216\"]\n",
        "authorLinks:\n  \"Dave216\": https://keenwiki.shikadi.net/wiki/User:Dave216\n",
        &format!("  keenwiki: {page}\n  pckf: https://pckf.com/viewtopic.php?t=1\n"),
        "  homepage: http://aurora.example/\n",
        &format!("  - url: {download}\n    sha256: {sha}\n    version: \"1.2\"\n"),
        "recipe:\n  type: \"episode\"\n",
    ] {
        assert!(body.contains(line), "{line} missing in\n{body}");
    }
    assert!(matches!(
        index_suggestion(&db, "nope"),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn takes_over_what_the_keenwiki_page_now_says() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("aurora.zip");
    write_zip(&zip, AURORA);
    let recipe = inspect(&zip, None).unwrap().recipes[0].recipe.clone();
    let mut req = request(&zip, recipe);
    let page_url = "https://keenwiki.shikadi.net/wiki/Aurora_Bliss";
    req.keenwiki = Some(KeenwikiOrigin {
        page_url: page_url.into(),
        download_url: "https://files.shikadi.net/aurora.zip".into(),
        game_id: Some("keen4".into()),
        authors: vec!["Dave216".into()],
        author_links: BTreeMap::new(),
        version: None,
        description: Some("Blissful.".into()),
        released: Some("2012-05-01".into()),
        homepage: Some("http://aurora.example/".into()),
        pckf: None,
        keenmodding: None,
        status: Some("Beta".into()),
    });
    let m = install(&paths, &db, &req).unwrap();
    assert_eq!(list(&db).unwrap()[0].status, Some(ModStatus::Beta));
    // The game's profile comes first when the mod's was made a second later.
    let profiles = crate::profiles::list(&db, |_| false, |_| false).unwrap();
    let profile = profiles.iter().find(|p| p.id == m.profile_id).unwrap();
    assert_eq!(profile.mod_status, Some(ModStatus::Beta));

    let mut page = WikiMod {
        page_url: page_url.into(),
        name: "Aurora Bliss".into(),
        game_id: Some("keen4".into()),
        authors: vec!["Dave216".into()],
        author_links: BTreeMap::from([(
            "Dave216".into(),
            "https://keenwiki.shikadi.net/wiki/User:Dave216".into(),
        )]),
        status: Some("Abandoned".into()),
        released: Some("2012-05-01".into()),
        updated: None,
        levels: None,
        pckf: Some("https://pckf.com/viewtopic.php?t=1".into()),
        keenmodding: Some("https://keenmodding.org/viewtopic.php?t=2".into()),
        // The page no longer says.
        homepage: None,
        description: Some("Even more blissful.".into()),
        credits: vec![],
        downloads: vec![],
    };
    assert_eq!(
        refresh_details(&db, &m.id, &page).unwrap(),
        [
            PageDetail::Authors,
            PageDetail::Description,
            PageDetail::Pckf,
            PageDetail::Keenmodding,
            PageDetail::Status
        ]
    );
    let listed = &list(&db).unwrap()[0];
    assert_eq!(
        listed.author_links["Dave216"],
        "https://keenwiki.shikadi.net/wiki/User:Dave216"
    );
    assert_eq!(listed.description.as_deref(), Some("Even more blissful."));
    assert_eq!(listed.pckf.as_deref(), page.pckf.as_deref());
    assert_eq!(listed.keenmodding.as_deref(), page.keenmodding.as_deref());
    assert_eq!(listed.status, Some(ModStatus::Abandoned));
    assert_eq!(listed.homepage.as_deref(), Some("http://aurora.example/"));
    // Nothing new the second time.
    assert!(refresh_details(&db, &m.id, &page).unwrap().is_empty());
    page.released = Some("2012-05-02".into());
    assert_eq!(
        refresh_details(&db, &m.id, &page).unwrap(),
        [PageDetail::Released]
    );
}

#[test]
fn wont_replace_a_mod_from_the_index() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("aurora.zip");
    write_zip(&zip, AURORA);
    let recipe = inspect(&zip, None).unwrap().recipes[0].recipe.clone();
    let old = install(&paths, &db, &request(&zip, recipe.clone())).unwrap();
    db.conn()
        .execute(
            "UPDATE mods SET index_id = 'aurora' WHERE id = ?1",
            [&old.id],
        )
        .unwrap();
    let err = install(
        &paths,
        &db,
        &InstallModRequest {
            replace: Some(old.id.clone()),
            ..request(&zip, recipe)
        },
    )
    .unwrap_err();
    assert!(matches!(err, Error::IncompatibleInstall(_)), "{err:?}");
    assert_eq!(list(&db).unwrap().len(), 1);
}

#[test]
fn replaces_another_version_keeping_saves_and_page() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("aurora.zip");
    write_zip(&zip, AURORA);
    let recipe = inspect(&zip, None).unwrap().recipes[0].recipe.clone();
    let mut req = request(&zip, recipe.clone());
    req.keenwiki = Some(KeenwikiOrigin {
        page_url: "https://keenwiki.shikadi.net/wiki/Aurora_Bliss".into(),
        download_url: "https://files.shikadi.net/aurora.zip".into(),
        game_id: Some("keen4".into()),
        authors: vec!["Dave216".into()],
        author_links: BTreeMap::new(),
        version: Some("1.1".into()),
        description: Some("Blissful.".into()),
        released: None,
        homepage: Some("http://aurora.example/".into()),
        pckf: Some("https://pckf.com/viewtopic.php?t=1".into()),
        keenmodding: None,
        status: None,
    });
    let old = install(&paths, &db, &req).unwrap();
    fs::write(paths.profiles.join("aurora/state/SAVEGAM0.CK4"), b"save").unwrap();

    let newer = tmp.path().join("m.dave216.aurora_v.1.2.k4.2023-08-23.zip");
    let mut files = AURORA.to_vec();
    files[3] = ("Aurora/GAMEMAPS.CK4", b"newer maps");
    write_zip(&newer, &files);
    let m = install(
        &paths,
        &db,
        &InstallModRequest {
            replace: Some(old.id.clone()),
            ..request(&newer, recipe.clone())
        },
    )
    .unwrap();
    assert_eq!((m.id.as_str(), m.profile_id.as_str()), ("aurora", "aurora"));
    assert_eq!(m.source, ModSource::Manual);
    assert_eq!(m.version.as_deref(), Some("1.2"));
    assert_eq!(m.description.as_deref(), Some("Blissful."));
    assert_eq!(m.keenwiki_url, old.keenwiki_url);
    assert_eq!(m.homepage, old.homepage);
    assert_eq!(m.pckf, old.pckf);
    assert_eq!(list(&db).unwrap().len(), 1);
    let files = paths.mods.join("aurora/files");
    assert_eq!(fs::read(files.join("GAMEMAPS.CK4")).unwrap(), b"newer maps");
    assert!(
        paths
            .mods
            .join("aurora/source")
            .join(newer.file_name().unwrap())
            .is_file()
    );
    assert!(!paths.mods.join("aurora/source/aurora.zip").exists());
    assert!(paths.profiles.join("aurora/state/SAVEGAM0.CK4").is_file());
    assert_eq!(fs::read_dir(&paths.cache).unwrap().count(), 0);

    let k5 = tmp.path().join("k5mod");
    write_folder(&k5, &[("LEVEL01.CK5", b"x")]);
    let k5_recipe = inspect(&k5, None).unwrap().recipes[0].recipe.clone();
    let other_game = InstallModRequest {
        replace: Some("aurora".into()),
        install_id: None,
        ..request(&k5, k5_recipe)
    };
    assert!(matches!(
        install(&paths, &db, &other_game),
        Err(Error::IncompatibleInstall(_))
    ));
    let missing = InstallModRequest {
        replace: Some("nope".into()),
        ..request(&newer, recipe)
    };
    assert!(matches!(
        install(&paths, &db, &missing),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn refuses_other_games_and_versions() {
    let (tmp, paths, db) = setup();
    let src = tmp.path().join("k5mod");
    write_folder(&src, &[("LEVEL.CK5", b""), ("GAMEMAPS.CK5", b"")]);
    let info = inspect(&src, None).unwrap();
    assert_eq!(info.game_id, "keen5");
    let err = install(&paths, &db, &request(&src, Recipe::Episode)).unwrap_err();
    assert!(matches!(err, Error::IncompatibleInstall(_)));
    // Nothing left behind.
    assert_eq!(fs::read_dir(&paths.mods).unwrap().count(), 0);

    let option = RecipeOption {
        recipe: Recipe::Episode,
        origin: String::new(),
        game_versions: vec!["1.4".into()],
        version_ids: vec!["keen4-1.4-ega".into()],
        missing_files: vec![],
        base_mod: None,
        made_for: None,
    };
    let mut i = library::list(&db).unwrap().remove(0);
    check_install("keen4", &option, &i).unwrap();
    i.version_id = Some("keen4-1.0".into());
    assert!(check_install("keen4", &option, &i).is_err());
    i.version_id = Some("keen4-1.4-ega".into());
    check_install("keen4", &option, &i).unwrap();

    // A patch for a version the catalog doesn't know: the EGA one isn't it.
    let cga = RecipeOption {
        game_versions: vec!["cga-2.0".into()],
        version_ids: vec![],
        ..option
    };
    assert!(check_install("keen4", &cga, &i).is_err());
    i.version_id = None;
    check_install("keen4", &cga, &i).unwrap();
}

#[test]
fn identifies_versions_the_catalog_learned_for_installed_mods() {
    let (tmp, paths, db) = setup();
    let src = tmp.path().join("cga");
    write_folder(
        &src,
        &[
            (
                "play.bat",
                b"CK4PATCH.EXE KEEN4C16.PAT
",
            ),
            ("CK4PATCH.EXE", b"patcher"),
            (
                "KEEN4C16.PAT",
                b"%ext ck4
%version cga-1.4
",
            ),
        ],
    );
    let info = inspect(&src, None).unwrap();
    assert_eq!(info.recipes[0].version_ids, ["keen4-1.4-cga"]);
    let m = install(&paths, &db, &request(&src, info.recipes[0].recipe.clone())).unwrap();
    // As installed before the catalog knew the version, its game since
    // identified as that one.
    db.conn()
        .execute_batch(
            "UPDATE mods SET requires_json = '[]';
             UPDATE profiles SET install_id = NULL WHERE mod_id IS NOT NULL;
             UPDATE installs SET version_id = 'keen4-1.4-cga';",
        )
        .unwrap();
    assert!(fitting_installs(&db, &m.id).unwrap().is_empty());

    identify_requirements(&paths, &db).unwrap();
    assert_eq!(fitting_installs(&db, &m.id).unwrap()[0].id, "keen4-1");
    let install_id: Option<String> = db
        .conn()
        .query_row(
            "SELECT install_id FROM profiles WHERE mod_id = ?1",
            [&m.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(install_id.as_deref(), Some("keen4-1"));
}

#[test]
fn patches_for_all_versions_fit_every_version() {
    let (tmp, paths, db) = setup();
    let src = tmp.path().join("moon");
    write_folder(
        &src,
        &[
            ("CK4PATCH.EXE", b"patcher"),
            ("MOON.PAT", b"%ext ck4\n%version all\n%patch $1 $2\n"),
        ],
    );
    let info = inspect(&src, None).unwrap();
    assert!(info.recipes[0].game_versions.is_empty());
    let m = install(&paths, &db, &request(&src, info.recipes[0].recipe.clone())).unwrap();
    // As The Last Moon was installed, when "all" counted as a version.
    db.conn()
        .execute_batch(
            "UPDATE mods SET requires_json = '[]';
             UPDATE installs SET version_id = 'keen4-1.4-ega';",
        )
        .unwrap();
    assert!(fitting_installs(&db, &m.id).unwrap().is_empty());

    identify_requirements(&paths, &db).unwrap();
    let requires: Option<String> = db
        .conn()
        .query_row("SELECT requires_json FROM mods", [], |r| r.get(0))
        .unwrap();
    assert_eq!(requires, None);
    assert_eq!(fitting_installs(&db, &m.id).unwrap()[0].id, "keen4-1");
}

fn curated(recipe: Recipe) -> Curated {
    Curated {
        index_id: "aurora-bliss".into(),
        game_id: "keen4".into(),
        name: "Aurora Bliss".into(),
        authors: vec!["Dave216".into()],
        author_links: BTreeMap::new(),
        version: Some("1.2".into()),
        description: Some("A Keen 4 mod.".into()),
        released: None,
        recipe,
        version_recipes: VersionRecipes::new(),
        copy_to_root: Vec::new(),
        extends: None,
        cover: None,
        standalone: false,
        own_files: false,
        requires: vec!["keen4-1.4-ega".into()],
        keenwiki_url: Some("https://keenwiki.shikadi.net/wiki/Aurora_Bliss".into()),
        url: "https://example.com/m.dave216.aurora.zip".into(),
        sha256: "ab".repeat(32),
        worlds: None,
        root: None,
        save_dirs: Vec::new(),
        cover_from: None,
        variants: Vec::new(),
        parts: BTreeMap::new(),
        part_files: BTreeMap::new(),
    }
}

fn aurora_patch(args: &[&str]) -> Recipe {
    Recipe::Patch {
        patcher: "CK4PATCH.EXE".into(),
        patch: "ABIATHAR.PAT".into(),
        args: args.iter().map(|a| a.to_string()).collect(),
    }
}

#[test]
fn curated_install_uses_the_index_recipe_and_updates_in_place() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("index-aurora-bliss.zip");
    write_zip(&zip, AURORA);
    let c = curated(aurora_patch(&["/nocheck"]));
    let m = install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    assert_eq!(m.id, "aurora-bliss");
    assert_eq!(m.recipe, c.recipe);
    assert_eq!(m.source, ModSource::Index);
    assert_eq!(m.index_id.as_deref(), Some("aurora-bliss"));
    assert!(
        paths
            .mods
            .join("aurora-bliss/source/m.dave216.aurora.zip")
            .is_file()
    );
    let (url, wiki): (String, String) = db
        .conn()
        .query_row("SELECT source_url, keenwiki_url FROM mods", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(url, c.url);
    assert!(wiki.ends_with("Aurora_Bliss"));
    assert_eq!(cover_program(&db, "aurora-bliss").unwrap(), None);

    // A new version: same mod and profile, new files, saves kept.
    let state = paths.profiles.join("aurora-bliss/state/SAVEGAM0.CK4");
    fs::write(&state, b"save").unwrap();
    let mut v2: Vec<(&str, &[u8])> = AURORA.to_vec();
    v2[3] = ("Aurora/GAMEMAPS.CK4", b"better maps");
    write_zip(&zip, &v2);
    let c2 = Curated {
        sha256: sha256_hex(&fs::read(&zip).unwrap()),
        version: Some("1.3".into()),
        cover: Some("AURORA.EXE".into()),
        ..c.clone()
    };
    let updated = install_curated(&paths, &db, &zip, &c2, None, false).unwrap();
    assert_eq!(updated.id, "aurora-bliss");
    assert_eq!(
        cover_program(&db, "aurora-bliss").unwrap().as_deref(),
        Some("AURORA.EXE")
    );
    assert_eq!(updated.version.as_deref(), Some("1.3"));
    assert_eq!(list(&db).unwrap().len(), 1);
    assert_eq!(
        fs::read(paths.mods.join("aurora-bliss/files/GAMEMAPS.CK4")).unwrap(),
        b"better maps"
    );
    assert_eq!(fs::read(&state).unwrap(), b"save");
    let (sha, composed): (String, Option<String>) = db
        .conn()
        .query_row(
            "SELECT m.archive_sha256, p.composed_hash FROM mods m JOIN profiles p ON p.mod_id = m.id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(sha, c2.sha256);
    assert_eq!(composed, None);
    assert!(!paths.cache.join("mod-aurora-bliss-update").exists());
}

#[test]
fn standalone_mods_play_without_their_game_and_follow_their_entry() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("foray.zip");
    write_zip(
        &zip,
        &[
            ("FITF.EXE", b"menu"),
            ("FITF-X.EXE", b"game"),
            ("GAMEMAPS.CKS", b"maps"),
        ],
    );
    let c = Curated {
        index_id: "foray-in-the-forest".into(),
        name: "Foray in the Forest".into(),
        requires: Vec::new(),
        standalone: true,
        own_files: true,
        ..curated(Recipe::Program {
            program: "FITF.EXE".into(),
            args: vec!["adlib".into()],
        })
    };
    // Not on the Keen 4 copy there is, and no game is missing.
    let m = install_curated(&paths, &db, &zip, &c, Some("keen4-1"), false).unwrap();
    let (p, layers) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!(
        (p.install_id, p.missing_game, p.standalone),
        (None, None, true)
    );
    assert_eq!(p.game_id, "keen4");
    assert_eq!(layers.lower, None);
    assert!(fitting_installs(&db, &m.id).unwrap().is_empty());
    assert!(set_install(&db, &m.id, "keen4-1").is_err());
    assert!(rehome(&db, None).unwrap().is_empty());
    // An update stays without it.
    install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    assert_eq!(
        profiles::get(&db, &m.profile_id).unwrap().0.install_id,
        None
    );

    // An entry that drops it puts the mod on a copy, one that takes it up
    // takes the mod off again; the saves stay.
    let save = paths
        .profiles
        .join(&m.profile_id)
        .join("state/SAVEGAM0.CKS");
    fs::write(&save, b"save").unwrap();
    set_standalone(&db, "foray-in-the-forest", false, true).unwrap();
    let (p, _) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!(
        (p.install_id.as_deref(), p.standalone),
        (Some("keen4-1"), false)
    );
    set_standalone(&db, "foray-in-the-forest", true, true).unwrap();
    let (p, _) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!((p.install_id, p.standalone), (None, true));
    assert_eq!(fs::read(&save).unwrap(), b"save");
}

#[test]
fn a_fingerprint_is_the_mod_roots_files_but_documents_and_saves() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("aurora.zip");
    let mut files = AURORA.to_vec();
    files.extend([
        ("Aurora/SAVEGAM0.CK4", b"save" as &[u8]),
        ("Aurora/extras/LEVEL99.CK4", b"extra"),
        ("Aurora/EXTRAS.RAR", b"rar"),
    ]);
    write_zip(&zip, &files);
    let source = Source::open(&zip).unwrap();
    let analysis = analyze(&source, &zip, None).unwrap();
    let f = super::index_add::fingerprint(&source, &analysis)
        .unwrap()
        .unwrap();
    assert_eq!(
        f.files,
        ["abiathar.pat", "CK4PATCH.EXE", "GAMEMAPS.CK4", "play.bat"]
    );
    let hashes: Vec<(String, String)> = f
        .files
        .iter()
        .map(|n| {
            let data = AURORA
                .iter()
                .find(|(p, _)| p.ends_with(&format!("/{n}")))
                .unwrap()
                .1;
            (n.clone(), sha256_hex(data))
        })
        .collect();
    assert_eq!(f.sha256, crate::mod_index::fingerprint_sha256(&hashes));
}

#[test]
fn a_players_source_mod_keeps_its_files_and_saves() {
    let (tmp, paths, db) = setup();
    let folder = tmp.path().join("My Games/OSI");
    write_folder(
        &folder,
        &[
            ("osidos32.exe", b"old build"),
            ("GAMEMAPS.CK5", b"maps"),
            ("SAVEGAM0.CK5", b"my save"),
            ("CONFIG.CK5", b"my settings"),
        ],
    );
    let c = Curated {
        index_id: "operation-station-infiltration".into(),
        game_id: "keen5".into(),
        name: "Operation Station Infiltration".into(),
        version: Some("1.0".into()),
        requires: Vec::new(),
        standalone: true,
        own_files: true,
        ..curated(Recipe::Program {
            program: "osidos32.exe".into(),
            args: Vec::new(),
        })
    };
    let m = install_players_files(&paths, &db, &folder, &c, None, false).unwrap();
    assert_eq!(m.recipe, c.recipe);
    assert_eq!(m.version.as_deref(), Some("1.0"));
    assert_eq!(
        m.index_id.as_deref(),
        Some("operation-station-infiltration")
    );
    let files = paths.mods.join(&m.id).join("files");
    let state = paths.profiles.join(&m.profile_id).join("state");
    assert_eq!(fs::read(files.join("GAMEMAPS.CK5")).unwrap(), b"maps");
    assert!(!files.join("SAVEGAM0.CK5").exists());
    assert_eq!(fs::read(state.join("SAVEGAM0.CK5")).unwrap(), b"my save");
    assert_eq!(fs::read(state.join("CONFIG.CK5")).unwrap(), b"my settings");
    // The player's folder stays as it was.
    assert!(folder.join("SAVEGAM0.CK5").is_file());
    // A source mod if the index says so; profiles of other entries aren't.
    let source_mod = |entry: &'static str| {
        profiles::list(&db, |_| false, |id| id == entry)
            .unwrap()
            .into_iter()
            .any(|p| p.id == m.profile_id && p.source_mod)
    };
    assert!(source_mod("operation-station-infiltration"));
    assert!(!source_mod("other"));
    // It counts as that download, so the newest one is an update.
    let sha: String = db
        .conn()
        .query_row("SELECT archive_sha256 FROM mods", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sha, c.sha256);

    // Installed again over it: the saves played since win.
    fs::write(state.join("SAVEGAM0.CK5"), b"played on").unwrap();
    install_players_files(&paths, &db, &folder, &c, None, false).unwrap();
    assert_eq!(fs::read(state.join("SAVEGAM0.CK5")).unwrap(), b"played on");
    assert_eq!(list(&db).unwrap().len(), 1);
}

#[test]
fn source_mods_that_require_their_game_go_on_a_copy_of_it() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("pack.zip");
    write_zip(&zip, &[("RCK4.EXE", b"game"), ("GAMEMAPS.CK4", b"maps")]);
    let c = Curated {
        index_id: "pack".into(),
        name: "Pack".into(),
        requires: Vec::new(),
        standalone: false,
        own_files: true,
        ..curated(Recipe::Program {
            program: "RCK4.EXE".into(),
            args: vec![],
        })
    };
    let m = install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    let (p, layers) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!(
        (p.install_id.as_deref(), p.standalone),
        (Some("keen4-1"), false)
    );
    assert!(layers.lower.is_some());
    assert!(own_files(&db, &m.id).unwrap());
    // Without the game it waits for one.
    crate::library::remove(&paths, &db, "keen4-1", false, false).unwrap();
    let (p, _) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!(p.install_id, None);
    assert!(p.missing_game.is_some());
}

#[test]
fn the_archive_name_from_the_url_stays_in_the_source_folder() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("index-aurora-bliss.zip");
    write_zip(&zip, AURORA);
    let c = Curated {
        url: r"https://example.com/..\..\..\evil.zip".into(),
        ..curated(aurora_patch(&["/nocheck"]))
    };
    install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    let names: Vec<_> = fs::read_dir(paths.mods.join("aurora-bliss/source"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.len(), 1);
    assert!(names[0].ends_with("evil.zip"));
    assert!(!paths.mods.join("evil.zip").exists());
    assert!(!paths.mods.parent().unwrap().join("evil.zip").exists());
}

#[test]
fn archives_from_urls_with_a_query_keep_only_their_file_name() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("index-aurora-bliss.zip");
    write_zip(&zip, AURORA);
    let c = Curated {
        url: "https://www.dropbox.com/scl/fi/abc/AURORA.ZIP?rlkey=xyz&dl=1".into(),
        ..curated(aurora_patch(&["/nocheck"]))
    };
    install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    assert!(paths.mods.join("aurora-bliss/source/AURORA.ZIP").is_file());
}

#[test]
fn mods_played_over_another_need_it_and_go_with_it() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("index-pack.zip");
    // Levels only: the patcher and patch come from the mod below.
    write_zip(&zip, &[("Pack/GAMEMAPS.CK4", b"pack maps")]);
    let pack = Curated {
        index_id: "aurora-pack".into(),
        name: "Aurora Pack".into(),
        extends: Some("aurora-bliss".into()),
        url: "https://example.com/pack.zip".into(),
        ..curated(aurora_patch(&[]))
    };
    assert!(matches!(
        install_curated(&paths, &db, &zip, &pack, None, false),
        Err(Error::MissingGame(m)) if m == "aurora-bliss"
    ));

    let base = tmp.path().join("index-aurora-bliss.zip");
    write_zip(&base, AURORA);
    install_curated(&paths, &db, &base, &curated(aurora_patch(&[])), None, false).unwrap();
    let m = install_curated(&paths, &db, &zip, &pack, None, false).unwrap();
    assert_eq!(m.extends.as_deref(), Some("aurora-bliss"));
    let (profile, layers) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!(profile.missing_game, None);
    let layer = layers.mod_layer.as_ref().unwrap();
    assert_eq!(layer.base, Some(paths.mods.join("aurora-bliss/files")));
    assert_eq!(layers.sources().len(), 3);
    // A patch the mod below doesn't have either is still missing.
    let other = Curated {
        recipe: Recipe::Batch {
            file: "START.BAT".into(),
        },
        ..pack.clone()
    };
    assert!(matches!(
        install_curated(&paths, &db, &zip, &other, None, false),
        Err(Error::UnexpectedContent(_))
    ));

    assert_eq!(extensions(&db, "aurora-bliss").unwrap(), ["aurora-pack"]);
    delete(&paths, &db, "aurora-bliss", false).unwrap();
    assert!(list(&db).unwrap().is_empty());
    assert!(!paths.mods.join("aurora-pack").exists());
}

#[test]
fn mods_made_to_be_copied_over_another_are_found_by_its_files() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("legolevs.zip");
    // As Lego Levels: its patch loads Lego Keen's tiles.
    write_zip(
        &zip,
        &[
            (
                "lego.pat",
                b"%ext ck1\n%patchfile 0x130F8 lego.tli\n%patch $8041 $90\n%end\n",
            ),
            ("LEVEL01.CK1", b"l"),
            (
                "readme.txt",
                b"LEGO LEVELS\nTo install, first install Lego Keen.\n",
            ),
        ],
    );
    let info = inspect(&zip, None).unwrap();
    let missing = &info.recipes[0].missing_files;
    assert_eq!(missing, &["lego.tli"]);
    // From the index, by the files its entry says it has; its name in
    // the readme doesn't count.
    let index = |n: &str, provides: &[&str]| BaseCandidate {
        name: n.into(),
        mod_id: None,
        index_id: Some(slug(n)),
        files: None,
        provides: provides.iter().map(|p| p.to_string()).collect(),
        standalone: false,
    };
    let entries = [
        index("Lego Keen", &[]),
        index("Bricks", &["lego.pat", "LEGO.TLI"]),
    ];
    assert_eq!(
        needed_mod(missing, &entries),
        Some(BaseMod {
            name: "Bricks".into(),
            mod_id: None,
            index_id: Some("bricks".into()),
            standalone: false,
        })
    );
    assert_eq!(needed_mod(missing, &entries[..1]), None);

    // An installed mod with the files counts too.
    let has = tmp.path().join("has");
    let lacks = tmp.path().join("lacks");
    write_folder(&has, &[("LEGO.TLI", b"t")]);
    write_folder(&lacks, &[("lego.pat", b"p")]);
    let installed = |n: &str, files: &Path| BaseCandidate {
        name: n.into(),
        mod_id: Some(slug(n)),
        index_id: None,
        files: Some(files.to_path_buf()),
        provides: Vec::new(),
        standalone: false,
    };
    let installed = [installed("Lego Levels", &lacks), installed("Blocks", &has)];
    let found = needed_mod(missing, &installed).unwrap();
    assert_eq!(found.mod_id.as_deref(), Some("blocks"));
    assert_eq!(needed_mod(missing, &installed[..1]), None);
    // An entry that provides them goes first, with its installed mod;
    // another installed one may only bring the files along.
    let mut entry = entries[1].clone();
    entry.mod_id = Some("bricks".into());
    let both = [installed[1].clone(), entry];
    assert_eq!(
        needed_mod(missing, &both).unwrap().mod_id.as_deref(),
        Some("bricks")
    );

    // Nothing missing: nothing needed.
    assert_eq!(needed_mod(&[], &entries), None);
}

#[test]
fn levels_alone_are_for_the_mod_their_archive_names() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp
        .path()
        .join("l.keen_community.episode_smile.2009-08-30.zip");
    write_zip(
        &zip,
        &[("LEVEL01.CK2", b"l"), ("LIST.txt", b"Level 1: Shikadi")],
    );
    let info = inspect(&zip, Some("keen2")).unwrap();
    assert!(info.recipes[0].missing_files.is_empty());
    assert_eq!(info.made_for.as_deref(), Some("Episode Smile"));

    let candidate = |n: &str| BaseCandidate {
        name: n.into(),
        mod_id: Some(slug(n)),
        index_id: None,
        files: None,
        provides: Vec::new(),
        standalone: false,
    };
    let installed = [candidate("Lego Keen"), candidate("Episode: Smile")];
    assert_eq!(
        named_mod("Episode Smile", &installed)
            .unwrap()
            .mod_id
            .as_deref(),
        Some("episode-smile")
    );
    assert_eq!(named_mod("Episode Smile", &installed[..1]), None);
    assert_eq!(named_mod("", &[candidate("")]), None);
}

#[test]
fn levels_alone_lack_the_start_their_readme_names() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("xky1rooms.zip");
    // As xky1rooms, over Episode X. The game's own executable, the
    // patcher and web addresses don't count.
    let readme: &[u8] = b"To play, take a clean copy of XkyKeen1 and replace the \
        existing files. Then run XkyKeen1.bat.\nNot KEEN1.EXE, not ck1patch.exe, \
        see pckf.com. Again: xkykeen1.bat!\n";
    write_zip(&zip, &[("LEVEL01.CK1", b"l"), ("xky1rooms.txt", readme)]);
    let info = inspect(&zip, None).unwrap();
    assert_eq!(info.recipes.len(), 1);
    assert_eq!(info.recipes[0].recipe, Recipe::Episode);
    assert_eq!(info.recipes[0].missing_files, ["XkyKeen1.bat"]);

    // Shipped along, it's there.
    write_zip(
        &zip,
        &[
            ("LEVEL01.CK1", b"l"),
            ("xky1rooms.txt", readme),
            ("XKYKEEN1.BAT", b"keen1\r\n"),
        ],
    );
    let info = inspect(&zip, None).unwrap();
    assert!(info.recipes.iter().all(|o| o.missing_files.is_empty()));

    // A mod with a start of its own may name other ones.
    write_zip(
        &zip,
        &[
            ("LEVEL01.CK1", b"l"),
            ("readme.txt", b"Run gox.bat to play.\n"),
            ("go.bat", b"keen1\r\n"),
        ],
    );
    let info = inspect(&zip, None).unwrap();
    assert!(info.recipes.iter().all(|o| o.missing_files.is_empty()));
}

#[test]
fn only_files_the_starting_patch_loads_can_be_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("mod.zip");
    let patch: &[u8] = b"%ext ck1\n%patchfile $100 patch\\Loader.bin\n%end\n";
    // As Mura: a file in a folder, named with a backslash. As The
    // Electromagnet: another patch file it doesn't start with, loading a
    // file that isn't there.
    write_zip(
        &zip,
        &[
            ("Mod/play.bat", b"ck1patch mura.pat\r\n"),
            ("Mod/CK1PATCH.EXE", b"MZ"),
            ("Mod/mura.pat", patch),
            ("Mod/patch/loader.bin", b"b"),
            (
                "Mod/tileinfo.pat",
                b"%ext ck1\n%patchfile $200 tileinfo.tli\n%end\n",
            ),
            ("Mod/LEVEL01.CK1", b"l"),
        ],
    );
    let info = inspect(&zip, None).unwrap();
    assert_eq!(info.recipes[0].origin, "play.bat");
    assert_eq!(info.recipes[0].missing_files, Vec::<String>::new());
    // Picked instead, the leftover lacks its file.
    let leftover = info
        .recipes
        .iter()
        .find(|o| o.recipe.patch_file() == Some("tileinfo.pat"))
        .unwrap();
    assert_eq!(leftover.missing_files, ["tileinfo.tli"]);

    // Without the folder, the file it starts with is missing.
    write_zip(
        &zip,
        &[
            ("Mod/play.bat", b"ck1patch mura.pat\r\n"),
            ("Mod/CK1PATCH.EXE", b"MZ"),
            ("Mod/mura.pat", patch),
            ("Mod/LEVEL01.CK1", b"l"),
        ],
    );
    assert_eq!(
        inspect(&zip, None).unwrap().recipes[0].missing_files,
        ["patch\\Loader.bin"]
    );
}

#[test]
fn levels_added_by_hand_play_over_the_mod_they_name() {
    let (tmp, paths, db) = setup();
    let base = tmp.path().join("index-aurora-bliss.zip");
    write_zip(&base, AURORA);
    install_curated(&paths, &db, &base, &curated(aurora_patch(&[])), None, false).unwrap();

    // Levels only: they start the way the mod below does.
    let zip = tmp.path().join("pack.zip");
    write_zip(
        &zip,
        &[("Pack/LEVEL01.CK4", b"l"), ("Pack/GAMEMAPS.CK4", b"maps")],
    );
    assert_eq!(
        inspect(&zip, None).unwrap().recipes[0].recipe,
        Recipe::Episode
    );
    let req = InstallModRequest {
        name: "Pack".into(),
        extends: Some("aurora-bliss".into()),
        ..request(&zip, Recipe::Episode)
    };
    let m = install(&paths, &db, &req).unwrap();
    assert_eq!(m.extends.as_deref(), Some("aurora-bliss"));
    assert_eq!(m.recipe, aurora_patch(&[]));
    let requires: Option<String> = db
        .conn()
        .query_row(
            "SELECT requires_json FROM mods WHERE id = ?1",
            [&m.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(requires.as_deref(), Some(r#"["keen4-1.4-ega"]"#));
    let (profile, layers) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!(
        profile.played_over,
        Some(profiles::PlayedOver {
            profile_id: "aurora-bliss".into(),
            name: "Aurora Bliss".into(),
        })
    );
    assert_eq!(
        layers.mod_layer.unwrap().base,
        Some(paths.mods.join("aurora-bliss/files"))
    );

    // Not over a mod that's played over another one, nor another game's.
    let again = InstallModRequest {
        extends: Some(m.id.clone()),
        ..req.clone()
    };
    assert!(matches!(
        install(&paths, &db, &again),
        Err(Error::IncompatibleInstall(_))
    ));
    let k5 = tmp.path().join("k5.zip");
    write_zip(&k5, &[("LEVEL01.CK5", b"l")]);
    let other = InstallModRequest {
        install_id: None,
        ..request(&k5, Recipe::Episode)
    };
    let other = InstallModRequest {
        extends: Some(m.id.clone()),
        ..other
    };
    assert!(install(&paths, &db, &other).is_err());

    delete(&paths, &db, "aurora-bliss", false).unwrap();
    assert!(list(&db).unwrap().is_empty());
}

#[test]
fn levels_over_a_standalone_mod_need_no_game_either() {
    let (tmp, paths, db) = setup();
    let base = tmp.path().join("index-aurora-bliss.zip");
    // All of the game's files, as Episode X has Keen 1's.
    let mut files = AURORA.to_vec();
    files.extend([
        ("Aurora/KEEN4E.EXE", b"MZ".as_slice()),
        ("Aurora/EGAGRAPH.CK4", b"g"),
        ("Aurora/AUDIO.CK4", b"a"),
    ]);
    write_zip(&base, &files);
    let c = Curated {
        standalone: true,
        ..curated(aurora_patch(&[]))
    };
    install_curated(&paths, &db, &base, &c, None, false).unwrap();

    // xky1rooms over Episode X: not on the player's Keen 1, like the mod below.
    let zip = tmp.path().join("pack.zip");
    write_zip(&zip, &[("Pack/LEVEL01.CK4", b"l")]);
    let req = InstallModRequest {
        name: "Pack".into(),
        extends: Some("aurora-bliss".into()),
        ..request(&zip, Recipe::Episode)
    };
    let m = install(&paths, &db, &req).unwrap();
    assert!(standalone(&db, &m.id).unwrap());
    let (profile, layers) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!(profile.install_id, None);
    assert_eq!(profile.missing_game, None);
    assert!(profile.standalone);
    // A mod of its game, not a source mod.
    assert!(!profile.source_mod);
    assert_eq!(
        layers.mod_layer.unwrap().base,
        Some(paths.mods.join("aurora-bliss/files"))
    );
}

#[test]
fn mods_played_over_one_follow_it_when_it_turns_standalone() {
    let (tmp, paths, db) = setup();
    let base = tmp.path().join("index-aurora-bliss.zip");
    write_zip(&base, AURORA);
    install_curated(&paths, &db, &base, &curated(aurora_patch(&[])), None, false).unwrap();
    let zip = tmp.path().join("pack.zip");
    write_zip(&zip, &[("Pack/LEVEL01.CK4", b"l")]);
    let req = InstallModRequest {
        name: "Pack".into(),
        extends: Some("aurora-bliss".into()),
        ..request(&zip, Recipe::Episode)
    };
    let pack = install(&paths, &db, &req).unwrap();
    let install_of = |id: &str| profiles::get(&db, id).unwrap().0.install_id;
    assert_eq!(install_of(&pack.profile_id).as_deref(), Some("keen4-1"));

    // Lego Keen turns standalone: Lego Levels over it leaves the copy too.
    set_standalone(&db, "aurora-bliss", true, false).unwrap();
    assert!(standalone(&db, &pack.id).unwrap());
    assert_eq!(install_of(&pack.profile_id), None);
    assert_eq!(install_of("aurora-bliss"), None);

    // And back onto the copy with it.
    set_standalone(&db, "aurora-bliss", false, false).unwrap();
    assert!(!standalone(&db, &pack.id).unwrap());
    assert_eq!(install_of(&pack.profile_id).as_deref(), Some("keen4-1"));

    // One an earlier change left behind catches up.
    db.conn()
        .execute("UPDATE mods SET standalone = 1 WHERE id = ?1", [&pack.id])
        .unwrap();
    set_standalone(&db, "aurora-bliss", false, false).unwrap();
    assert!(!standalone(&db, &pack.id).unwrap());
}

#[test]
fn curated_install_checks_the_download_and_the_library() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("index-x.zip");
    write_zip(&zip, AURORA);
    let missing = curated(Recipe::Batch {
        file: "START.BAT".into(),
    });
    assert!(matches!(
        install_curated(&paths, &db, &zip, &missing, None, false),
        Err(Error::UnexpectedContent(_))
    ));
    let other_game = Curated {
        game_id: "keen5".into(),
        ..curated(aurora_patch(&[]))
    };
    assert!(matches!(
        install_curated(&paths, &db, &zip, &other_game, None, false),
        Err(Error::UnexpectedContent(_))
    ));

    let k5 = tmp.path().join("k5.zip");
    write_zip(&k5, &[("GAMEMAPS.CK5", b""), ("LEVEL.CK5", b"")]);
    let needs_k5 = Curated {
        game_id: "keen5".into(),
        requires: vec!["keen5-1.4-ega".into()],
        ..curated(Recipe::Episode)
    };
    match install_curated(&paths, &db, &k5, &needs_k5, None, false) {
        Err(Error::MissingGame(what)) => assert!(what.ends_with("(v1.4 EGA)"), "{what}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        install_curated(
            &paths,
            &db,
            &zip,
            &curated(aurora_patch(&[])),
            Some("nope"),
            false
        ),
        Err(Error::IncompatibleInstall(_))
    ));
    assert_eq!(fs::read_dir(&paths.mods).unwrap().count(), 0);
}

#[test]
fn mods_wait_for_their_game_and_move_when_it_goes() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = AppPaths::new(tmp.path().join("data"));
    paths.ensure().unwrap();
    let db = Db::open_in_memory().unwrap();
    let zip = tmp.path().join("aurora.zip");
    write_zip(&zip, AURORA);
    let info = inspect(&zip, None).unwrap();
    let req = InstallModRequest {
        install_id: None,
        ..request(&zip, info.recipes[0].recipe.clone())
    };
    let m = install(&paths, &db, &req).unwrap();
    let (p, layers) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!((p.install_id, p.game_id.as_str()), (None, "keen4"));
    assert_eq!(
        p.missing_game.as_deref(),
        Some("Commander Keen 4: Secret of the Oracle (v1.4 EGA)")
    );
    assert_eq!(layers.lower, None);
    let k5 = tmp.path().join("k5.zip");
    write_zip(&k5, &[("GAMEMAPS.CK5", b""), ("LEVEL.CK5", b"")]);
    let needs_k5 = Curated {
        game_id: "keen5".into(),
        requires: vec![],
        ..curated(Recipe::Episode)
    };
    let k5_mod = install_curated(&paths, &db, &k5, &needs_k5, None, true).unwrap();
    let install_of = |id: &str| profiles::get(&db, id).unwrap().0.install_id;
    assert_eq!(install_of(&k5_mod.profile_id), None);

    // The game arrives, of an unknown version, which may be the right one.
    let src = tmp.path().join("k4");
    write_folder(&src, KEEN4);
    library::import(&paths, &db, &src).unwrap();
    assert_eq!(install_of("aurora").as_deref(), Some("keen4-1"));
    assert_eq!(profiles::get(&db, "aurora").unwrap().0.missing_game, None);
    assert_eq!(install_of(&k5_mod.profile_id), None);

    // Removing it moves the mod to another release, else it waits again.
    let other = tmp.path().join("k4b");
    write_folder(&other, KEEN4);
    fs::write(other.join("KEEN4E.EXE"), b"other").unwrap();
    library::import(&paths, &db, &other).unwrap();
    library::remove(&paths, &db, "keen4-1", false, false).unwrap();
    assert_eq!(install_of("aurora").as_deref(), Some("keen4-2"));
    library::remove(&paths, &db, "keen4-2", false, false).unwrap();
    assert_eq!(install_of("aurora"), None);
    assert!(paths.mods.join("aurora/files/GAMEMAPS.CK4").is_file());
    assert_eq!(list(&db).unwrap().len(), 2);
}

#[test]
fn mods_move_to_another_fitting_copy() {
    let (tmp, paths, db) = setup();
    let other = tmp.path().join("k4b");
    write_folder(&other, KEEN4);
    fs::write(other.join("KEEN4E.EXE"), b"other").unwrap();
    library::import(&paths, &db, &other).unwrap();
    let zip = tmp.path().join("aurora.zip");
    write_zip(&zip, AURORA);
    let info = inspect(&zip, None).unwrap();
    let m = install(&paths, &db, &request(&zip, info.recipes[0].recipe.clone())).unwrap();
    let ids = |v: Vec<Install>| v.into_iter().map(|i| i.id).collect::<Vec<_>>();
    assert_eq!(
        ids(fitting_installs(&db, &m.id).unwrap()),
        ["keen4-1", "keen4-2"]
    );

    set_install(&db, &m.id, "keen4-2").unwrap();
    let p = profiles::get(&db, &m.profile_id).unwrap().0;
    assert_eq!(p.install_id.as_deref(), Some("keen4-2"));
    assert!(matches!(
        set_install(&db, &m.id, "keen5-1"),
        Err(Error::IncompatibleInstall(_))
    ));
    assert_eq!(
        profiles::get(&db, &m.profile_id)
            .unwrap()
            .0
            .install_id
            .as_deref(),
        Some("keen4-2")
    );
    assert!(set_install(&db, "nope", "keen4-1").is_err());
}

#[test]
fn a_known_version_the_mod_doesnt_name_isnt_used() {
    let k6 = catalog::game("keen6").unwrap();
    let install = Install {
        id: "keen6-1".into(),
        game_id: "keen6".into(),
        version_id: Some("keen6-1.4-ega".into()),
        label: String::new(),
        source: library::InstallSource::Folder,
        path: String::new(),
        added_at: 0,
    };
    let requires = parse_requires(Some(r#"["keen6-1.0"]"#));
    assert!(!fits(k6, requires.as_deref(), &install));
    assert!(fits(k6, parse_requires(None).as_deref(), &install));
    assert_eq!(
        needed_game(k6, requires.as_deref()),
        format!("{} (v1.0 EGA)", k6.title)
    );
}

#[test]
fn curated_versions_and_ranking() {
    let k6 = catalog::game("keen6").unwrap();
    let install = |v: Option<&str>| Install {
        id: "i".into(),
        game_id: "keen6".into(),
        version_id: v.map(str::to_string),
        label: String::new(),
        source: library::InstallSource::Folder,
        path: String::new(),
        added_at: 0,
    };
    let v10 = vec!["keen6-1.0".to_string()];
    let v14 = vec!["keen6-1.4-ega".to_string()];
    assert!(fits(k6, Some(&v10), &install(Some("keen6-1.0-nocp"))));
    assert!(!fits(k6, Some(&v14), &install(Some("keen6-1.0-nocp"))));
    assert!(fits(k6, Some(&v14), &install(None)));
    assert!(fits(k6, None, &install(Some("keen6-1.0"))));
    // A patch naming versions the catalog doesn't know.
    assert!(!fits(k6, Some(&[]), &install(Some("keen6-1.0"))));
    assert!(!fits(catalog::game("keen4").unwrap(), None, &install(None)));
    let rank = |v| fit_rank(k6, &install(v));
    assert!(rank(Some("keen6-1.0")) < rank(Some("keen6-1.0-nocp")));
    assert!(rank(Some("keen6-1.0-nocp")) < rank(None));
    // Releases by the game's preferred versions, builds after all of them.
    assert!(rank(Some("keen6-1.4-ega")) < rank(Some("keen6-1.5-ega")));
    assert!(rank(Some("keen6-1.5-ega")) < rank(Some("keen6-1.0")));
    assert!(rank(Some("keen6-1.4-cga")) < rank(Some("keen6-1.0-cga-anthology")));
    assert!(rank(Some("keen6-1.0-cga-anthology")) < rank(Some("keen6-1.5-cga")));
    assert!(rank(Some("keen6-demo")) < rank(Some("keen6-demo-apogee")));
    assert!(rank(Some("keen6-demo-apogee")) < rank(Some("keen6-1.5-ega-nocp")));
    assert!(rank(Some("keen6-1.5-ega-nocp")) < rank(Some("keen6-1.0-nocp")));
    let k1 = catalog::game("keen1").unwrap();
    let k1_rank = |v: &str| {
        let i = Install {
            game_id: "keen1".into(),
            ..install(Some(v))
        };
        fit_rank(k1, &i)
    };
    assert!(k1_rank("keen1-1.1") < k1_rank("keen1-beta-dec"));
    // Same game files as the original: as good as it.
    let kdreams = catalog::game("kdreams").unwrap();
    let v193 = vec!["kdreams-1.93".to_string()];
    let anthology = Install {
        game_id: "kdreams".into(),
        ..install(Some("kdreams-1.93-anthology"))
    };
    assert!(fits(kdreams, Some(&v193), &anthology));
    assert_eq!(fit_rank(kdreams, &anthology), (0, 0));
}

#[test]
fn unknown_recipe_is_rejected() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("aurora.zip");
    write_zip(&zip, AURORA);
    let bad = Recipe::Batch {
        file: "EVIL.BAT".into(),
    };
    assert!(install(&paths, &db, &request(&zip, bad)).is_err());
}

#[test]
fn nothing_moddy() {
    let tmp = tempfile::tempdir().unwrap();
    write_folder(tmp.path(), &[("README.TXT", b"")]);
    assert!(matches!(inspect(tmp.path(), None), Err(Error::NoModFound)));
}

#[test]
fn dropped_things_are_told_apart() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("aurora.zip");
    write_zip(&zip, AURORA);
    let Dropped::Mod { info: patched } = classify(&zip).unwrap() else {
        panic!("not a mod");
    };
    // A game of a version the catalog doesn't know is still a game…
    let game = tmp.path().join("k4");
    write_folder(&game, KEEN4);
    assert!(matches!(classify(&game).unwrap(), Dropped::Games));
    // …but the game's files with a patch are a mod.
    let modded = tmp.path().join("k4mod");
    write_folder(&modded, KEEN4);
    write_folder(
        &modded,
        &[
            ("CK4PATCH.EXE", b"patcher"),
            ("MOD.PAT", b"%ext ck4\n%version 1.4\n"),
        ],
    );
    assert!(matches!(classify(&modded).unwrap(), Dropped::Mod { .. }));
    let empty = tmp.path().join("empty");
    write_folder(&empty, &[("README.TXT", b"")]);
    assert!(matches!(classify(&empty).unwrap(), Dropped::Nothing));
    let text = tmp.path().join("notes.txt");
    fs::write(&text, "").unwrap();
    assert!(matches!(classify(&text), Err(Error::UnsupportedSource(_))));

    let with = |recipe| ModInfo {
        recipes: vec![RecipeOption {
            recipe,
            origin: "x".into(),
            game_versions: vec![],
            version_ids: vec![],
            missing_files: vec![],
            base_mod: None,
            made_for: None,
        }],
        ..(*patched).clone()
    };
    let batch = with(Recipe::Batch {
        file: "PLAY.BAT".into(),
    });
    assert!(matches!(decide(true, Some(batch)), Dropped::Mod { .. }));
    assert!(matches!(
        decide(true, Some(with(Recipe::Episode))),
        Dropped::Games
    ));
    assert!(matches!(decide(true, None), Dropped::Games));
    assert!(matches!(
        decide(false, Some(with(Recipe::Episode))),
        Dropped::Mod { .. }
    ));
    assert!(matches!(decide(false, None), Dropped::Nothing));
}

#[test]
fn leaves_out_keen_1_3_settings() {
    let tmp = tempfile::tempdir().unwrap();
    write_folder(
        tmp.path(),
        &[
            ("Ctlpanel.ck1", b"keys"),
            ("LEVEL01.CK1", b""),
            ("KEEN1.EXE", b"modded"),
            ("EXTRA/CTLPANEL.CK1", b"keys"),
        ],
    );
    let a = analyze(&Source::open(tmp.path()).unwrap(), tmp.path(), None).unwrap();
    assert_eq!(a.info.game_id, "keen1");
    assert!(
        !a.files
            .iter()
            .any(|f| f.eq_ignore_ascii_case("CTLPANEL.CK1"))
    );
    assert!(a.files.contains(&"LEVEL01.CK1".to_string()));
    assert!(a.files.contains(&"EXTRA/CTLPANEL.CK1".to_string()));
}

#[test]
fn a_source_mod_with_its_own_files_is_found_for_a_known_game() {
    // As Foray in the Forest: own executables, all data as *.CKS, nothing
    // that names Keen 4. The index entry says which game it's for.
    let tmp = tempfile::tempdir().unwrap();
    write_folder(
        tmp.path(),
        &[
            ("FITF/FITF.EXE", b"MZ"),
            ("FITF/FITF-X.EXE", b"MZ"),
            ("FITF/EGAGRAPH.CKS", b""),
            ("FITF/MAPTEMP.CKS", b""),
            ("FITF/HELPME.COM", b""),
            ("FITF/COPYING.TXT", b"GPL"),
        ],
    );
    let source = Source::open(tmp.path()).unwrap();
    let a = analyze(&source, tmp.path(), Some("keen4")).unwrap();
    assert_eq!(a.info.game_id, "keen4");
    assert_eq!(a.root, "FITF");
    assert!(a.files.contains(&"FITF/EGAGRAPH.CKS".to_string()));
    // Without knowing the game it's still nothing BloogBase recognises.
    assert!(matches!(
        analyze(&source, tmp.path(), None),
        Err(Error::NoModFound)
    ));
}

#[test]
fn patch_files_without_a_patcher_use_the_bundled_one() {
    // As Robot Rebellion: its menu runs `ck1patch ckrr.pat`, which the
    // player was to have already.
    let tmp = tempfile::tempdir().unwrap();
    write_folder(
        tmp.path(),
        &[
            ("ROBOKEEN.EXE", b"MZ"),
            (
                "ckrr.pat",
                b"%ext ck1\n%patchfile 0x12087 CKSCREEN.CK1\n%end\n",
            ),
            ("LEVEL01.CK1", b""),
        ],
    );
    let a = analyze(&Source::open(tmp.path()).unwrap(), tmp.path(), None).unwrap();
    assert_eq!(a.info.game_id, "keen1");
    let recipes: Vec<&Recipe> = a.info.recipes.iter().map(|o| &o.recipe).collect();
    let program = Recipe::Program {
        program: "ROBOKEEN.EXE".into(),
        args: Vec::new(),
    };
    let patch = Recipe::Patch {
        patcher: "CK1PATCH.EXE".into(),
        patch: "ckrr.pat".into(),
        args: Vec::new(),
    };
    assert_eq!(recipes, [&program, &patch]);
    // The patcher isn't one of the mod's files.
    assert!(
        !a.files
            .iter()
            .any(|f| f.eq_ignore_ascii_case("CK1PATCH.EXE"))
    );
}

#[test]
fn keeps_unknown_executables() {
    let tmp = tempfile::tempdir().unwrap();
    write_folder(
        tmp.path(),
        &[
            ("KDREAMS.EXE", b"modded"),
            ("KDREAMS.MAP", b""),
            ("KDRPATCH.EXE", b""),
        ],
    );
    let a = analyze(&Source::open(tmp.path()).unwrap(), tmp.path(), None).unwrap();
    assert!(a.files.contains(&"KDREAMS.EXE".to_string()));
    assert_eq!(a.info.game_id, "kdreams");
}

#[test]
fn renamed_patcher_and_patch_files_are_found() {
    // As Norp the Yorp 3: CK3PATCH as NORPER.EXE, one `.NY3` patch per
    // game version, each run by its own batch file.
    let tmp = tempfile::tempdir().unwrap();
    let norper = [b"MZ".as_slice(), &[0; 0x1E], b"Usage: CK3PATCH {patchfile}"].concat();
    let patch = b"#Text Files
%patchfile 0x17b00 DRAGON.NY3
%patch 0x1ba4c \"N3MAP\"
";
    write_folder(
        tmp.path(),
        &[
            ("NORPER.EXE", &norper),
            ("MAGMA.NY3", patch),
            ("PLASMA.NY3", patch),
            ("NORP3v131.bat", b"norper magma.ny3"),
            ("NORP3v10.bat", b"norper plasma.ny3"),
            ("N3MAP01.NY3", b""),
        ],
    );
    let a = analyze(&Source::open(tmp.path()).unwrap(), tmp.path(), None).unwrap();
    assert_eq!(a.info.game_id, "keen3");
    let recipes: Vec<&Recipe> = a.info.recipes.iter().map(|o| &o.recipe).collect();
    let patch = |p: &str| Recipe::Patch {
        patcher: "NORPER.EXE".into(),
        patch: p.into(),
        args: Vec::new(),
    };
    assert_eq!(recipes, [&patch("PLASMA.NY3"), &patch("MAGMA.NY3")]);
    assert!(!a.info.confident);
}

#[test]
fn curated_version_recipes_and_copies_to_root() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("aurora.zip");
    let mut files: Vec<(&str, &[u8])> = AURORA.to_vec();
    files.push(("Aurora/other.pat", b"%ext ck4\n"));
    files.push(("Aurora/1/CREATURE.CK4", b"big sprites"));
    write_zip(&zip, &files);
    let other = Recipe::Patch {
        patcher: "CK4PATCH.EXE".into(),
        patch: "other.pat".into(),
        args: Vec::new(),
    };
    let c = Curated {
        version_recipes: [("keen4-1.4-ega".to_string(), other.clone())].into(),
        copy_to_root: vec!["1".into()],
        ..curated(aurora_patch(&[]))
    };
    let m = install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    let files_dir = paths.mods.join(&m.id).join("files");
    assert_eq!(
        fs::read(files_dir.join("CREATURE.CK4")).unwrap(),
        b"big sprites"
    );
    assert!(files_dir.join("1/CREATURE.CK4").exists());
    // Played on 1.4 EGA, the mod starts with that version's recipe.
    assert_eq!(list(&db).unwrap()[0].recipe, aurora_patch(&[]));
    db.conn()
        .execute("UPDATE installs SET version_id = 'keen4-1.4-ega'", [])
        .unwrap();
    assert_eq!(list(&db).unwrap()[0].recipe, other);
    let (_, layers) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!(layers.mod_layer.unwrap().recipe, other);

    // Elsewhere the default one.
    let by_version = c.version_recipes.clone();
    let default = aurora_patch(&[]);
    assert_eq!(
        recipe::for_version(&default, &by_version, "keen4", Some("keen4-1.4-cga")),
        &default
    );
    assert_eq!(
        recipe::for_version(&default, &by_version, "keen4", None),
        &default
    );

    let missing = Curated {
        copy_to_root: vec!["2".into()],
        ..c
    };
    assert!(matches!(
        install_curated(&paths, &db, &zip, &missing, None, false),
        Err(Error::UnexpectedContent(_))
    ));
}

#[test]
fn root_is_the_folder_with_most_keen_files() {
    let files: Vec<String> = [
        "Mod/LEVEL01.CK1",
        "Mod/LEVEL02.CK1",
        "Mod/extras/LEVEL01.CK1",
        "readme.txt",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(find_root(&files, |_| false).as_deref(), Some("Mod"));
    assert_eq!(find_root(&["a.txt".to_string()], |_| false), None);

    // As Keen Rescue: more levels in a subfolder than files next to the
    // patcher.
    let mut files = vec![
        "krescue/CK3PATCH.EXE".to_string(),
        "krescue/martakr.pat".into(),
    ];
    files.extend((1..=5).map(|n| format!("krescue/levels/LEVEL0{n}.CK3")));
    assert_eq!(find_root(&files, |_| false).as_deref(), Some("krescue"));
}

#[test]
fn metadata_from_names() {
    let m = guess_metadata(
        Path::new("m.88vega.fireblaze_curse_of_the_dragonlords.k1.2018-11-18.zip"),
        "Fireblaze_Curse_of_the_Dragonlords",
    );
    assert_eq!(m.name, "Fireblaze Curse of the Dragonlords");
    assert_eq!(m.episode, Some("keen1"));
    assert_eq!(m.version, None);

    let m = guess_metadata(
        Path::new("l.alopex.back_to_fribbulus_xax_v1.0.k6.2024-10-03.zip"),
        "",
    );
    assert_eq!(
        (m.name.as_str(), m.version.as_deref()),
        ("Back to Fribbulus Xax", Some("1.0"))
    );

    let m = guess_metadata(
        Path::new("m.shikadi.simulator_keen.v.1.1.k3.2023-03-25.zip"),
        "",
    );
    assert_eq!(
        (m.name.as_str(), m.version.as_deref()),
        ("Simulator Keen", Some("1.1"))
    );
    let m = guess_metadata(Path::new("l.shikadi.k1.2008-02-14.zip"), "");
    assert_eq!(
        (m.name.as_str(), m.episode),
        ("Shikadi Level Pack", Some("keen1"))
    );
    // KeenWiki's packs for mods name no episode; a level pack's name is
    // the mod it's for, which it mustn't pass for.
    let m = guess_metadata(Path::new("l.keen_community.lego_keen.2012-02-10.zip"), "");
    assert_eq!(
        (m.name.as_str(), m.episode, m.released.as_deref()),
        ("Lego Keen Level Pack", None, Some("2012-02-10"))
    );
    assert_eq!(m.authors, ["keen_community"]);
    assert_eq!(m.made_for.as_deref(), Some("Lego Keen"));
    let m = guess_metadata(Path::new("m.someone.bricks_v2.1.2015-01-01.zip"), "");
    assert_eq!(
        (m.name.as_str(), m.version.as_deref()),
        ("Bricks", Some("2.1"))
    );

    assert_eq!(
        guess_metadata(Path::new("x/btower.zip"), "BTOWER").name,
        "BTOWER"
    );
    assert_eq!(guess_metadata(Path::new("x/my_mod.zip"), "").name, "My Mod");
    assert_eq!(
        guess_metadata(
            Path::new("m.keen_community.patchwork_mod_iii.k3.2008-10-29.zip"),
            ""
        )
        .name,
        "Patchwork Mod III"
    );
    assert_eq!(title_case("vi xiv mix ivy x"), "VI XIV Mix Ivy X");
    assert_eq!(
        guess_metadata(Path::new("x/magnet.zip"), "stuff/magnet").name,
        "Magnet"
    );
}

#[test]
fn third_party_builds_count_as_their_original() {
    let k6 = catalog::game("keen6").unwrap();
    assert_eq!(
        version_ids(k6, &["1.0".into()]),
        ["keen6-1.0", "keen6-1.0-nocp"]
    );
    // Not the CGA release of the same number.
    assert_eq!(version_ids(k6, &["1.4".into()]), ["keen6-1.4-ega"]);
    let kdreams = catalog::game("kdreams").unwrap();
    assert_eq!(
        version_ids(kdreams, &["1.93".into()]),
        ["kdreams-1.93-anthology", "kdreams-1.93"]
    );
}

#[test]
fn slugs() {
    assert_eq!(
        slug("The Grand Intelligence: Gold!"),
        "the-grand-intelligence-gold"
    );
    assert_eq!(slug("  ***  "), "mod");
    assert_eq!(slug("Ünïcode"), "n-code");
}

#[test]
fn counts_the_saves_in_the_mods_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let zip = tmp.path().join("aurora.zip");
    let mut files = AURORA.to_vec();
    files.push(("Aurora/SAVEGAM0.CK4", b"save"));
    files.push(("Aurora/SAVEGAM3.CK4", b"save"));
    files.push(("Aurora/old/SAVEGAM1.CK4", b"not where the game looks"));
    write_zip(&zip, &files);
    let info = inspect(&zip, None).unwrap();
    assert_eq!(info.file_count, 8);
    assert_eq!(info.save_count, 2);
}

#[test]
fn mods_with_all_of_the_games_files_need_no_copy_or_version() {
    let (tmp, paths, db) = setup();
    let k4: &[(&str, &[u8])] = &[
        ("KEEN4E.EXE", b"unpacked 1.4"),
        ("CK4PATCH.EXE", b"patcher"),
        ("MOD.PAT", b"%ext ck4\n%version 1.4\n"),
        ("EGAGRAPH.CK4", b"graphics"),
        ("GAMEMAPS.CK4", b"maps"),
        ("AUDIO.CK4", b"sounds"),
    ];
    let src = tmp.path().join("complete");
    write_folder(&src, k4);
    let info = inspect(&src, None).unwrap();
    assert!(info.complete && info.standalone);
    // The version the patch is for comes along.
    assert!(info.recipes[0].game_versions.is_empty());
    assert!(info.recipes[0].version_ids.is_empty());
    let mut req = request(&src, info.recipes[0].recipe.clone());
    req.install_id = None;
    let m = install(&paths, &db, &req).unwrap();
    let (p, _) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!((p.install_id, p.standalone), (None, true));
    assert!(p.missing_game.is_none());
    assert!(own_files(&db, &m.id).unwrap());

    // Without its sounds it reads the copy's, of the version it's made for.
    let src = tmp.path().join("partial");
    write_folder(&src, &k4[..5]);
    let info = inspect(&src, None).unwrap();
    assert!(!info.complete && !info.standalone);
    assert!(
        info.recipes[0]
            .version_ids
            .contains(&"keen4-1.4-ega".to_string())
    );

    // Keen 5 has no free shareware: any copy of it, but one.
    let src = tmp.path().join("k5");
    write_folder(
        &src,
        &[
            ("KEEN5E.EXE", b"unpacked 1.4"),
            ("CK5PATCH.EXE", b"patcher"),
            ("MOD.PAT", b"%ext ck5\n%version 1.4\n"),
            ("EGAGRAPH.CK5", b"graphics"),
            ("GAMEMAPS.CK5", b"maps"),
            ("AUDIO.CK5", b"sounds"),
        ],
    );
    let info = inspect(&src, None).unwrap();
    assert!(info.complete && !info.standalone);
    let mut req = request(&src, info.recipes[0].recipe.clone());
    req.install_id = None;
    let m = install(&paths, &db, &req).unwrap();
    let (p, _) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!((p.install_id, p.standalone), (None, false));
    assert!(p.missing_game.is_some());
    assert!(own_files(&db, &m.id).unwrap());
}

#[test]
fn installed_mods_without_their_executable_stay_on_their_copy() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("bounty.zip");
    write_zip(
        &zip,
        &[
            ("KEEN4E.EXE", b"the game's own"),
            ("CK4PATCH.EXE", b"patcher"),
            ("ABIATHAR.PAT", b"%ext ck4\n%version 1.4\n"),
            ("EGAGRAPH.CK4", b"graphics"),
            ("GAMEMAPS.CK4", b"maps"),
            ("AUDIO.CK4", b"sounds"),
        ],
    );
    // As installed before the entry turned standalone; older apps left out
    // the game's unchanged executable.
    let c = curated(aurora_patch(&[]));
    let m = install_curated(&paths, &db, &zip, &c, Some("keen4-1"), false).unwrap();
    let files = paths.mods.join(&m.id).join("files");
    fs::remove_file(files.join("KEEN4E.EXE")).unwrap();
    set_standalone(&db, "aurora-bliss", true, true).unwrap();
    let (p, _) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!(
        (p.install_id.as_deref(), p.standalone),
        (Some("keen4-1"), false)
    );
    assert!(!own_files(&db, &m.id).unwrap());

    // An update brings it, and then the mod needs no game.
    let c = Curated {
        standalone: true,
        own_files: true,
        requires: Vec::new(),
        ..c
    };
    install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    assert!(files.join("KEEN4E.EXE").is_file());
    set_standalone(&db, "aurora-bliss", true, true).unwrap();
    let (p, _) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!((p.install_id, p.standalone), (None, true));
}

/// A mod's levels copied over a real Keen 3 make the folder a mod, and its
/// game isn't offered for the library.
///
/// `cargo test --lib levels_copied_over -- --ignored`
#[test]
#[ignore = "needs the real games"]
fn levels_copied_over_the_game_are_a_mod() {
    let games = std::env::var_os("KEEN_GAMES")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.local/games"));
    let original = games.join("versions/Commander Keen 3 v1.31");
    let tmp = tempfile::tempdir().unwrap();
    let game = tmp.path().join("keen3");
    fs::create_dir(&game).unwrap();
    for entry in fs::read_dir(&original).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), game.join(entry.file_name())).unwrap();
    }
    assert!(matches!(classify(&game).unwrap(), Dropped::Games));

    let level = fs::read(game.join("LEVEL02.CK3")).unwrap();
    fs::write(game.join("LEVEL01.CK3"), level).unwrap();
    let Dropped::Mod { info } = classify(&game).unwrap() else {
        panic!("not a mod");
    };
    assert_eq!(info.game_id, "keen3");
    let (_dir, _paths, db) = setup();
    assert_eq!(
        crate::library::bundled_game(&db, &game, "keen3").unwrap(),
        None
    );
}

/// Revival of Bliss: a launcher in the archive's root starts one of three
/// folders, each a whole game.
const REVIVAL: &[(&str, &[u8])] = &[
    ("play.bat", b"@echo off\r\ncls\r\n_lfilea.bat %1\r\n"),
    (
        "_lfilea.bat",
        b"@echo off\r\nmklaunch /b _lfileb.bat _lfilea.bat\r\n",
    ),
    ("MKLAUNCH.EXE", b"launcher"),
    ("LAUNCHER.DAT", b"menu"),
    ("CRAIG/CK4PATCH.EXE", b"patcher"),
    ("CRAIG/abiathar.pat", b"%ext ck4\n%version 1.4\n"),
    ("CRAIG/file.bat", b"CK4PATCH abiathar.pat\r\n"),
    ("CRAIG/GAMEMAPS.CK4", b"craig maps"),
    ("KEEN/CK4PATCH.EXE", b"patcher"),
    ("KEEN/abiathar.pat", b"%ext ck4\n%version 1.4\n"),
    ("KEEN/file.bat", b"CK4PATCH abiathar.pat\r\n"),
    ("KEEN/GAMEMAPS.CK4", b"keen maps"),
    ("FUTURE/CK4PATCH.EXE", b"patcher"),
    ("FUTURE/WRAPPER.COM", b"wrapper"),
];

fn revival(zip: &Path, root: Option<&str>) -> Curated {
    let (recipe, root) = match root {
        Some(r) => (
            Recipe::Batch {
                file: "play.bat".into(),
            },
            Some(r.to_string()),
        ),
        None => (aurora_patch(&[]), None),
    };
    Curated {
        index_id: "revival-of-bliss".into(),
        name: "Revival of Bliss".into(),
        requires: Vec::new(),
        standalone: true,
        own_files: true,
        url: "https://example.com/m.dave216.revival.zip".into(),
        sha256: sha256_hex(&fs::read(zip).unwrap()),
        save_dirs: match root {
            Some(_) => revival_dirs(),
            None => Vec::new(),
        },
        root,
        ..curated(recipe)
    }
}

fn revival_dirs() -> Vec<crate::mod_index::SaveDir> {
    serde_json::from_str(
        r#"[{"dir": "KEEN", "name": "Keen"},
            {"dir": "CRAIG", "name": "Craig", "oldRoot": true},
            {"dir": "FUTURE", "name": "Epilogue", "translations": {"de": {"name": "Epilog"}}}]"#,
    )
    .unwrap()
}

fn mod_root(db: &Db, id: &str) -> Option<String> {
    db.conn()
        .query_row("SELECT root FROM mods WHERE id = ?1", [id], |r| r.get(0))
        .unwrap()
}

#[test]
fn a_named_root_installs_the_launcher_with_every_folder() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("revival.zip");
    write_zip(&zip, REVIVAL);
    // Left to itself, the app takes one of the folders.
    let m = install_curated(&paths, &db, &zip, &revival(&zip, None), None, false).unwrap();
    assert!(paths.mods.join(&m.id).join("files/abiathar.pat").is_file());
    assert_eq!(mod_root(&db, &m.id), None);
    delete(&paths, &db, &m.id, false).unwrap();

    let m = install_curated(&paths, &db, &zip, &revival(&zip, Some("")), None, false).unwrap();
    let files = paths.mods.join(&m.id).join("files");
    for f in [
        "play.bat",
        "MKLAUNCH.EXE",
        "CRAIG/abiathar.pat",
        "KEEN/file.bat",
        "FUTURE/WRAPPER.COM",
    ] {
        assert!(files.join(f).is_file(), "{f}");
    }
    assert_eq!(mod_root(&db, &m.id).as_deref(), Some(""));
    assert_eq!(
        m.recipe,
        Recipe::Batch {
            file: "play.bat".into()
        }
    );

    // A root the archive lacks.
    let c = Curated {
        root: Some("PAST".into()),
        ..revival(&zip, Some(""))
    };
    assert!(install_curated(&paths, &db, &zip, &c, None, false).is_err());
}

/// An install from before the entry named its root: Craig's files, with
/// saves, settings and a marker in its state.
fn old_revival() -> (tempfile::TempDir, AppPaths, Db, Mod, std::path::PathBuf) {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("revival.zip");
    write_zip(&zip, REVIVAL);
    let m = install_curated(&paths, &db, &zip, &revival(&zip, None), None, false).unwrap();
    let state = paths.profiles.join(&m.profile_id).join("state");
    fs::write(state.join("SAVEGAM0.CK4"), b"save").unwrap();
    fs::write(state.join("CONFIG.CK4"), b"config").unwrap();
    fs::write(state.join("DBOVERLAY_DEL_X.CK4"), b"empty").unwrap();
    db.conn()
        .execute("UPDATE profiles SET composed_hash = 'old'", [])
        .unwrap();
    (tmp, paths, db, m, zip)
}

#[test]
fn an_install_from_before_the_root_moves_there_with_its_saves() {
    let (_tmp, paths, db, m, zip) = old_revival();
    let running = crate::launcher::Running::default();
    let c = revival(&zip, Some(""));
    assert!(follow_root(&paths, &db, &running, &m.id, &c, Some("CRAIG")).unwrap());

    let files = paths.mods.join(&m.id).join("files");
    assert!(files.join("play.bat").is_file());
    assert!(files.join("CRAIG/abiathar.pat").is_file());
    assert!(!files.join("abiathar.pat").exists());
    let state = paths.profiles.join(&m.profile_id).join("state");
    assert_eq!(fs::read(state.join("CRAIG/SAVEGAM0.CK4")).unwrap(), b"save");
    assert_eq!(fs::read(state.join("CRAIG/CONFIG.CK4")).unwrap(), b"config");
    assert!(!state.join("SAVEGAM0.CK4").exists());
    assert!(state.join("DBOVERLAY_DEL_X.CK4").exists());
    assert_eq!(mod_root(&db, &m.id).as_deref(), Some(""));
    let (recipe, composed): (String, Option<String>) = db
        .conn()
        .query_row(
            "SELECT m.recipe_json, p.composed_hash FROM mods m JOIN profiles p ON p.mod_id = m.id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(recipe.contains("play.bat"), "{recipe}");
    assert_eq!(composed, None);
    assert!(
        paths
            .mods
            .join(&m.id)
            .join("source/m.dave216.revival.zip")
            .is_file()
    );
}

#[test]
fn a_running_mod_moves_to_its_root_later() {
    let (_tmp, paths, db, m, zip) = old_revival();
    let running = crate::launcher::Running::default();
    let claim = running.claim(&m.profile_id).unwrap();
    let c = revival(&zip, Some(""));
    assert!(!follow_root(&paths, &db, &running, &m.id, &c, Some("CRAIG")).unwrap());
    assert_eq!(mod_root(&db, &m.id), None);
    let state = paths.profiles.join(&m.profile_id).join("state");
    assert!(state.join("SAVEGAM0.CK4").is_file());
    drop(claim);
    assert!(follow_root(&paths, &db, &running, &m.id, &c, Some("CRAIG")).unwrap());
}

#[test]
fn a_failed_move_to_the_root_changes_nothing() {
    let (_tmp, paths, db, m, zip) = old_revival();
    let running = crate::launcher::Running::default();
    // The recipe isn't in the root.
    let c = Curated {
        recipe: Recipe::Batch {
            file: "START.BAT".into(),
        },
        ..revival(&zip, Some(""))
    };
    assert!(follow_root(&paths, &db, &running, &m.id, &c, Some("CRAIG")).is_err());
    let files = paths.mods.join(&m.id).join("files");
    assert!(files.join("abiathar.pat").is_file());
    assert!(!files.join("play.bat").exists());
    let state = paths.profiles.join(&m.profile_id).join("state");
    assert_eq!(fs::read(state.join("SAVEGAM0.CK4")).unwrap(), b"save");
    assert!(!state.join("CRAIG").exists());
    assert_eq!(mod_root(&db, &m.id), None);

    // Files that can't be swapped (Windows keeps open ones in place).
    #[cfg(windows)]
    {
        let c = revival(&zip, Some(""));
        use std::os::windows::fs::OpenOptionsExt;
        let open = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(files.join("abiathar.pat"))
            .unwrap();
        assert!(follow_root(&paths, &db, &running, &m.id, &c, Some("CRAIG")).is_err());
        drop(open);
        assert!(files.join("abiathar.pat").is_file());
        assert_eq!(fs::read(state.join("SAVEGAM0.CK4")).unwrap(), b"save");
        assert!(!state.join("CRAIG").exists());
        assert_eq!(mod_root(&db, &m.id), None);
    }
}

#[test]
fn saves_in_a_launchers_folders_count_and_come_back() {
    use crate::saves::tests::keen4_save;
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("revival.zip");
    write_zip(&zip, REVIVAL);
    let m = install_curated(&paths, &db, &zip, &revival(&zip, Some("")), None, false).unwrap();
    let state = paths.profiles.join(&m.profile_id).join("state");
    for dir in ["KEEN", "CRAIG", "OTHER"] {
        fs::create_dir_all(state.join(dir)).unwrap();
    }
    fs::write(state.join("KEEN/SAVEGAM0.CK4"), keen4_save("keen")).unwrap();
    fs::write(state.join("CRAIG/SAVEGAM4.CK4"), keen4_save("craig")).unwrap();
    fs::write(state.join("CRAIG/CONFIG.CK4"), b"config").unwrap();
    // Not a folder of the entry's.
    fs::write(state.join("OTHER/SAVEGAM1.CK4"), keen4_save("other")).unwrap();

    let saves = crate::saves::list(&db, &m.profile_id).unwrap();
    let got: Vec<(&str, Option<u32>)> = saves.iter().map(|s| (s.file.as_str(), s.slot)).collect();
    assert_eq!(
        got,
        [
            ("KEEN/SAVEGAM0.CK4", Some(1)),
            ("CRAIG/SAVEGAM4.CK4", Some(5))
        ]
    );
    let listed = profiles::list(&db, |_| false, |_| false).unwrap();
    assert_eq!(
        listed
            .iter()
            .find(|p| p.id == m.profile_id)
            .unwrap()
            .save_count,
        2
    );

    let b = backups::create(
        &paths,
        &db,
        &m.profile_id,
        Contents::State,
        backups::Reason::BeforeDelete,
    )
    .unwrap()
    .unwrap();
    assert_eq!((b.saves, b.settings, b.other), (2, 1, 1));
    fs::remove_file(state.join("KEEN/SAVEGAM0.CK4")).unwrap();
    backups::restore(&paths, &db, b.id, None, None, false).unwrap();
    assert_eq!(
        fs::read(state.join("KEEN/SAVEGAM0.CK4")).unwrap(),
        keen4_save("keen")
    );
}

#[test]
fn saves_from_a_folder_go_into_a_launchers_part() {
    use crate::saves::tests::keen4_save;
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("revival.zip");
    write_zip(&zip, REVIVAL);
    let m = install_curated(&paths, &db, &zip, &revival(&zip, Some("")), None, false).unwrap();
    let state = paths.profiles.join(&m.profile_id).join("state");
    let put = |dir: &Path, files: &[(&str, Vec<u8>)]| {
        for (name, data) in files {
            let path = dir.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, data).unwrap();
        }
    };

    // A whole copy: each folder's saves go into its part, the root's nowhere.
    let copy = tmp.path().join("Revival");
    put(
        &copy,
        &[
            ("play.bat", b"x".to_vec()),
            ("SAVEGAM5.CK4", keen4_save("root")),
            ("keen/SAVEGAM0.CK4", keen4_save("keen")),
            ("CRAIG/SAVEGAM0.CK4", keen4_save("craig")),
            ("CRAIG/CONFIG.CK4", b"config".to_vec()),
        ],
    );
    let found = backups::folder_saves(&db, &m.profile_id, &copy).unwrap();
    let names: Vec<_> = found.saves.iter().map(|s| s.file.as_str()).collect();
    assert_eq!(names, ["CRAIG/SAVEGAM0.CK4", "KEEN/SAVEGAM0.CK4"]);
    assert_eq!((found.settings, found.loose), (1, false));
    backups::restore_folder(&paths, &db, &m.profile_id, &copy, None, None, None, true).unwrap();
    assert_eq!(
        fs::read(state.join("KEEN/SAVEGAM0.CK4")).unwrap(),
        keen4_save("keen")
    );
    assert_eq!(fs::read(state.join("CRAIG/CONFIG.CK4")).unwrap(), b"config");
    assert!(!state.join("SAVEGAM5.CK4").exists());

    // One part's folder, by its name.
    let part = tmp.path().join("old/future");
    put(&part, &[("SAVEGAM1.CK4", keen4_save("future"))]);
    let found = backups::folder_saves(&db, &m.profile_id, &part).unwrap();
    assert_eq!(found.saves[0].file, "FUTURE/SAVEGAM1.CK4");
    assert!(!found.loose);

    // Any other folder: the player says which part, into a slot of it.
    let loose = tmp.path().join("forum");
    put(&loose, &[("SAVEGAM2.CK4", keen4_save("loose"))]);
    let found = backups::folder_saves(&db, &m.profile_id, &loose).unwrap();
    assert_eq!(found.saves[0].file, "SAVEGAM2.CK4");
    assert!(found.loose);
    assert!(
        backups::restore_folder(&paths, &db, &m.profile_id, &loose, None, None, None, false)
            .is_err()
    );
    assert!(
        backups::restore_folder(
            &paths,
            &db,
            &m.profile_id,
            &loose,
            Some("OTHER"),
            None,
            None,
            false
        )
        .is_err()
    );
    backups::restore_folder(
        &paths,
        &db,
        &m.profile_id,
        &loose,
        Some("craig"),
        Some("CRAIG/SAVEGAM2.CK4"),
        Some("CRAIG/SAVEGAM4.CK4"),
        false,
    )
    .unwrap();
    assert_eq!(
        fs::read(state.join("CRAIG/SAVEGAM4.CK4")).unwrap(),
        keen4_save("loose")
    );
    assert!(!state.join("SAVEGAM2.CK4").exists());
}

#[test]
fn saves_of_backups_from_before_the_root_go_to_its_old_folder() {
    let (_tmp, paths, db, m, zip) = old_revival();
    let old = backups::create(
        &paths,
        &db,
        &m.profile_id,
        Contents::State,
        backups::Reason::BeforeDelete,
    )
    .unwrap()
    .unwrap();
    let running = crate::launcher::Running::default();
    assert!(
        follow_root(
            &paths,
            &db,
            &running,
            &m.id,
            &revival(&zip, Some("")),
            Some("CRAIG")
        )
        .unwrap()
    );
    let state = paths.profiles.join(&m.profile_id).join("state");
    fs::remove_file(state.join("CRAIG/SAVEGAM0.CK4")).unwrap();

    let names: Vec<String> = backups::saves_in(&db, old.id)
        .unwrap()
        .into_iter()
        .map(|s| s.file)
        .collect();
    assert_eq!(names, ["CRAIG/SAVEGAM0.CK4"]);
    backups::restore(&paths, &db, old.id, Some("CRAIG/SAVEGAM0.CK4"), None, false).unwrap();
    assert_eq!(fs::read(state.join("CRAIG/SAVEGAM0.CK4")).unwrap(), b"save");
    assert!(!state.join("SAVEGAM0.CK4").exists());
}

#[test]
fn deleting_a_save_a_launchers_folder_ships_hides_it_there() {
    use crate::saves::tests::keen4_save;
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("revival.zip");
    let shipped = keen4_save("shipped");
    let mut files = REVIVAL.to_vec();
    files.push(("KEEN/SAVEGAM9.CK4", &shipped));
    write_zip(&zip, &files);
    let m = install_curated(&paths, &db, &zip, &revival(&zip, Some("")), None, false).unwrap();
    crate::saves::delete(&paths, &db, &m.profile_id, "KEEN/SAVEGAM9.CK4").unwrap();
    let state = paths.profiles.join(&m.profile_id).join("state");
    assert!(state.join("KEEN/DBOVERLAY_DEL_SAVEGAM9.CK4").is_file());
    assert!(crate::saves::list(&db, &m.profile_id).unwrap().is_empty());
}

fn stored(
    db: &Db,
    id: &str,
) -> (
    String,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
) {
    db.conn()
        .query_row(
            "SELECT m.name, m.recipe_json, m.version, m.archive_sha256, m.cover, p.composed_hash
             FROM mods m JOIN profiles p ON p.mod_id = m.id WHERE m.id = ?1",
            [id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap()
}

#[test]
fn installed_index_mods_follow_their_entrys_recipe_and_details() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("aurora.zip");
    write_zip(&zip, AURORA);
    let c = curated(aurora_patch(&["/nocheck"]));
    let m = install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    db.conn()
        .execute("UPDATE profiles SET composed_hash = 'old'", [])
        .unwrap();
    let running = crate::launcher::Running::default();

    // The entry now names it otherwise and starts it with its batch file.
    let newer = Curated {
        name: "Aurora Bliss Deluxe".into(),
        version: Some("9.9".into()),
        sha256: "cd".repeat(32),
        cover: Some("KEEN4E.EXE".into()),
        recipe: Recipe::Batch {
            file: "play.bat".into(),
        },
        ..c.clone()
    };
    assert!(follow_entry(&paths, &db, &running, &m.id, &newer).unwrap());
    let (name, recipe, version, sha, cover, composed) = stored(&db, &m.id);
    assert_eq!(name, "Aurora Bliss Deluxe");
    assert!(recipe.contains("play.bat"), "{recipe}");
    assert_eq!(cover.as_deref(), Some("KEEN4E.EXE"));
    // The download and its version stay those installed.
    assert_eq!((version.as_deref(), sha), (Some("1.2"), c.sha256.clone()));
    assert_eq!(composed, None);
    // Nothing new the next time.
    assert!(!follow_entry(&paths, &db, &running, &m.id, &newer).unwrap());

    // A recipe whose files the mod lacks stays out; the details come.
    db.conn()
        .execute("UPDATE profiles SET composed_hash = 'old'", [])
        .unwrap();
    let missing = Curated {
        name: "Aurora".into(),
        recipe: Recipe::Batch {
            file: "START.BAT".into(),
        },
        ..newer.clone()
    };
    assert!(follow_entry(&paths, &db, &running, &m.id, &missing).unwrap());
    let (name, recipe, _, _, _, composed) = stored(&db, &m.id);
    assert_eq!(name, "Aurora");
    assert!(recipe.contains("play.bat"), "{recipe}");
    assert_eq!(composed.as_deref(), Some("old"));

    // Not while it runs: then only the details.
    let claim = running.claim(&m.profile_id).unwrap();
    let back = Curated {
        name: "Aurora Bliss".into(),
        ..c.clone()
    };
    follow_entry(&paths, &db, &running, &m.id, &back).unwrap();
    let (name, recipe, ..) = stored(&db, &m.id);
    assert_eq!(name, "Aurora Bliss");
    assert!(recipe.contains("play.bat"), "{recipe}");
    drop(claim);
    follow_entry(&paths, &db, &running, &m.id, &back).unwrap();
    assert!(stored(&db, &m.id).1.contains("ABIATHAR.PAT"));
}

const LEVELS: &[(&str, &[u8])] = &[
    ("EASY/CK4PATCH.EXE", b"patcher"),
    ("EASY/x.pat", b"%ext ck4\n%version 1.4\n"),
    ("EASY/LEVEL01.CK4", b"easy"),
    ("EASY/README.TXT", b"Difficulty is now implemented"),
    ("HARD/CK4PATCH.EXE", b"patcher"),
    ("HARD/x.pat", b"%ext ck4\n%version 1.4\n"),
    ("HARD/LEVEL01.CK4", b"hard"),
    ("HARD/LE.COG", b"swapped"),
    ("sf/SHARED.CK4", b"shared"),
];

const GREEN: &[(&str, &[u8])] = &[
    ("CK4PATCH.EXE", b"patcher"),
    ("x.pat", b"%ext ck4\n%version 1.4\n"),
    ("LEVEL01.CK4", b"green"),
];

fn variant(json: &str) -> crate::mod_index::Variant {
    serde_json::from_str(json).unwrap()
}

/// A mod with variants Hard (default), Easy, Green (a part) and Swap.
fn with_variants(tmp: &Path) -> (std::path::PathBuf, Curated) {
    let zip = tmp.join("index-clouds.zip");
    write_zip(&zip, LEVELS);
    let part = tmp.join("index-clouds-green.zip");
    write_zip(&part, GREEN);
    let mut c = curated(Recipe::Patch {
        patcher: "CK4PATCH.EXE".into(),
        patch: "x.pat".into(),
        args: Vec::new(),
    });
    c.index_id = "clouds".into();
    c.name = "Clouds".into();
    c.url = "https://example.com/clouds.zip".into();
    c.variants = vec![
        variant(r#"{"id": "hard", "default": true, "name": "Hard", "root": "HARD"}"#),
        variant(r#"{"id": "easy", "name": "Easy", "root": "EASY"}"#),
        variant(r#"{"id": "green", "name": "Green", "archive": "green"}"#),
        variant(
            r#"{"id": "swap", "name": "Swap", "root": "", "copyToRoot": ["sf", "HARD"],
                "files": {"LEVEL80.CK4": "HARD/LE.COG"}}"#,
        ),
    ];
    c.parts.insert(
        "green".into(),
        crate::mod_index::Part {
            url: "https://example.com/clouds-green.zip".into(),
            sha256: sha256_hex(&fs::read(&part).unwrap()),
            size: 1,
        },
    );
    c.part_files.insert("green".into(), part);
    (zip, c)
}

fn active_variant(db: &Db) -> (Option<String>, bool) {
    db.conn()
        .query_row(
            "SELECT active_variant, variant_chosen FROM profiles WHERE mod_id IS NOT NULL",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
}

#[test]
fn variants_are_installed_each_in_its_folder() {
    let (tmp, paths, db) = setup();
    let (zip, c) = with_variants(tmp.path());
    let m = install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    let dir = paths.mods.join(&m.id);
    let read = |p: &str| fs::read(dir.join(p)).unwrap();
    assert_eq!(read("variants/hard/LEVEL01.CK4"), b"hard");
    assert_eq!(read("variants/easy/LEVEL01.CK4"), b"easy");
    assert_eq!(read("variants/green/LEVEL01.CK4"), b"green");
    // A shared folder and a renamed file.
    assert_eq!(read("variants/swap/SHARED.CK4"), b"shared");
    assert_eq!(read("variants/swap/LEVEL01.CK4"), b"hard");
    assert_eq!(read("variants/swap/LEVEL80.CK4"), b"swapped");
    assert!(!dir.join("files").exists());
    assert!(dir.join("source/clouds.zip").is_file());
    assert!(dir.join("source/clouds-green.zip").is_file());
    assert_eq!(active_variant(&db), (Some("hard".into()), false));
    let (profile, layers) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!(profile.variant_count, 4);
    assert_eq!(profile.active_variant.unwrap().name, "Hard");
    assert!(!profile.variant_chosen);
    assert_eq!(
        layers.mod_layer.unwrap().files,
        paths.mod_files(&m.id, Some("hard"))
    );
    assert_eq!(
        active_files(&paths, &db, &m.id).unwrap(),
        dir.join("variants/hard")
    );

    // A part that's missing, or a variant whose recipe isn't there.
    let (tmp2, paths2, db2) = setup();
    let (zip2, mut bad) = with_variants(tmp2.path());
    bad.part_files.clear();
    assert!(install_curated(&paths2, &db2, &zip2, &bad, None, false).is_err());
    let (_, mut bad) = with_variants(tmp2.path());
    bad.variants[1].recipe = Some(Recipe::Batch {
        file: "EASY.BAT".into(),
    });
    let err = install_curated(&paths2, &db2, &zip2, &bad, None, false).unwrap_err();
    assert!(err.to_string().contains("EASY.BAT"), "{err}");
    assert!(list(&db2).unwrap().is_empty());
}

#[test]
fn an_update_keeps_the_variant_played_or_finds_it_by_its_files() {
    let (tmp, paths, db) = setup();
    let (zip, c) = with_variants(tmp.path());
    // Installed before the entry had variants: the folder found, EASY.
    let old = install_curated(&paths, &db, &zip, &c.without_variants(), None, false).unwrap();
    assert_eq!(
        fs::read(paths.mods.join(&old.id).join("files/LEVEL01.CK4")).unwrap(),
        b"easy"
    );
    assert_eq!(active_variant(&db), (None, false));
    let state = paths.profiles.join(&old.id).join("state/SAVEGAM0.CK4");
    fs::write(&state, b"easy save").unwrap();

    let mut c = c.clone();
    let files = paths.mods.join(&old.id).join("files");
    c.variants[1].fingerprint = variant_files::fingerprint_of(&files, "keen4").unwrap();
    assert!(c.variants[1].fingerprint.is_some());
    install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    assert_eq!(active_variant(&db), (Some("easy".into()), false));
    assert!(!files.exists());
    assert_eq!(fs::read(&state).unwrap(), b"easy save");

    // The one played stays; one the entry dropped gives way to the default.
    db.conn()
        .execute(
            "UPDATE profiles SET active_variant = 'green' WHERE mod_id IS NOT NULL",
            [],
        )
        .unwrap();
    install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    assert_eq!(active_variant(&db).0.as_deref(), Some("green"));
    c.variants.retain(|v| v.id != "green");
    install_curated(&paths, &db, &zip, &c, None, false).unwrap();
    assert_eq!(active_variant(&db).0.as_deref(), Some("hard"));
    assert!(!paths.mods.join(&old.id).join("variants/green").exists());
}

#[test]
fn installed_mods_get_their_entrys_variants_and_keep_playing_theirs() {
    let (tmp, paths, db) = setup();
    let (zip, c) = with_variants(tmp.path());
    let old = install_curated(&paths, &db, &zip, &c.without_variants(), None, false).unwrap();
    let state = paths.profiles.join(&old.id).join("state");
    fs::write(state.join("SAVEGAM0.CK4"), b"easy save").unwrap();
    let files = paths.mod_files(&old.id, None);
    let running = crate::launcher::Running::default();

    // Without the part (no network), nothing changes.
    let mut offline = c.clone();
    offline.part_files.clear();
    assert!(convert_to_variants(&paths, &db, &running, &old.id, &offline, &zip).is_err());
    assert!(files.is_dir());
    assert_eq!(active_variant(&db), (None, false));
    // Nor while it runs.
    let claim = running.claim(&old.profile_id).unwrap();
    assert!(!convert_to_variants(&paths, &db, &running, &old.id, &c, &zip).unwrap());
    drop(claim);
    assert!(files.is_dir());

    // No fingerprints: the folder found then, EASY.
    assert!(convert_to_variants(&paths, &db, &running, &old.id, &c, &zip).unwrap());
    assert_eq!(active_variant(&db), (Some("easy".into()), false));
    assert!(!files.exists());
    assert_eq!(
        fs::read(paths.mod_files(&old.id, Some("green")).join("LEVEL01.CK4")).unwrap(),
        b"green"
    );
    assert_eq!(fs::read(state.join("SAVEGAM0.CK4")).unwrap(), b"easy save");

    // An entry's new variant comes along; the one played stays.
    let mut more = c.clone();
    more.variants.push(variant(
        r#"{"id": "hard2", "name": "Hard 2", "root": "HARD"}"#,
    ));
    assert!(convert_to_variants(&paths, &db, &running, &old.id, &more, &zip).unwrap());
    assert!(paths.mod_files(&old.id, Some("hard2")).is_dir());
    assert_eq!(active_variant(&db).0.as_deref(), Some("easy"));
}

#[test]
fn the_variant_played_so_far_is_known_by_its_files() {
    let (tmp, paths, db) = setup();
    let (zip, mut c) = with_variants(tmp.path());
    let old = install_curated(&paths, &db, &zip, &c.without_variants(), None, false).unwrap();
    // Its files are Hard's now, e.g. from the player's own folder.
    let files = paths.mod_files(&old.id, None);
    fs::write(files.join("LEVEL01.CK4"), b"hard").unwrap();
    let staged = tmp.path().join("hard");
    write_folder(
        &staged,
        &[
            ("CK4PATCH.EXE", b"patcher"),
            ("x.pat", b"%ext ck4\n%version 1.4\n"),
            ("LEVEL01.CK4", b"hard"),
        ],
    );
    c.variants[0].fingerprint = variant_files::fingerprint_of(&staged, "keen4").unwrap();
    let running = crate::launcher::Running::default();
    assert!(convert_to_variants(&paths, &db, &running, &old.id, &c, &zip).unwrap());
    assert_eq!(active_variant(&db).0.as_deref(), Some("hard"));
}

#[test]
fn variants_of_the_same_files_are_told_apart_by_the_recipe_played() {
    let (tmp, paths, db) = setup();
    let zip = tmp.path().join("dash.zip");
    let files: &[(&str, &[u8])] = &[
        ("CK4PATCH.EXE", b"patcher"),
        ("NORMAL.PAT", b"%ext ck4\n%version 1.4\n"),
        ("EASY.PAT", b"%ext ck4\n%version 1.4\n#easy"),
        ("LEVEL01.CK4", b"levels"),
    ];
    write_zip(&zip, files);
    let staged = tmp.path().join("staged");
    write_folder(&staged, files);
    let fp = variant_files::fingerprint_of(&staged, "keen4").unwrap();
    let patch = |p: &str| Recipe::Patch {
        patcher: "CK4PATCH.EXE".into(),
        patch: p.into(),
        args: Vec::new(),
    };
    let mut c = curated(patch("NORMAL.PAT"));
    c.variants = vec![
        variant(r#"{"id": "normal", "default": true, "name": "Normal"}"#),
        variant(r#"{"id": "easy", "name": "Easy"}"#),
    ];
    c.variants[1].recipe = Some(patch("EASY.PAT"));
    for v in &mut c.variants {
        v.fingerprint = fp.clone();
    }
    // Installed before, with the entry's recipe of then: Easy's.
    let old = Curated {
        recipe: patch("EASY.PAT"),
        ..c.without_variants()
    };
    let m = install_curated(&paths, &db, &zip, &old, None, false).unwrap();
    let running = crate::launcher::Running::default();
    assert!(convert_to_variants(&paths, &db, &running, &m.id, &c, &zip).unwrap());
    assert_eq!(active_variant(&db).0.as_deref(), Some("easy"));
    let (_, layers) = profiles::get(&db, &m.profile_id).unwrap();
    assert_eq!(layers.mod_layer.unwrap().recipe, patch("EASY.PAT"));
}

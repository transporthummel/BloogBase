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
    });
    let m = install(&paths, &db, &req).unwrap();

    let mut page = WikiMod {
        page_url: page_url.into(),
        name: "Aurora Bliss".into(),
        game_id: Some("keen4".into()),
        authors: vec!["Dave216".into()],
        author_links: BTreeMap::from([(
            "Dave216".into(),
            "https://keenwiki.shikadi.net/wiki/User:Dave216".into(),
        )]),
        status: None,
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
            PageDetail::Keenmodding
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
        requires: vec!["keen4-1.4-ega".into()],
        keenwiki_url: Some("https://keenwiki.shikadi.net/wiki/Aurora_Bliss".into()),
        url: "https://example.com/m.dave216.aurora.zip".into(),
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

    // A new version: same mod and profile, new files, saves kept.
    let state = paths.profiles.join("aurora-bliss/state/SAVEGAM0.CK4");
    fs::write(&state, b"save").unwrap();
    let mut v2: Vec<(&str, &[u8])> = AURORA.to_vec();
    v2[3] = ("Aurora/GAMEMAPS.CK4", b"better maps");
    write_zip(&zip, &v2);
    let c2 = Curated {
        version: Some("1.3".into()),
        ..c.clone()
    };
    let updated = install_curated(&paths, &db, &zip, &c2, None, false).unwrap();
    assert_eq!(updated.id, "aurora-bliss");
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
    assert_eq!(sha, sha256_hex(&fs::read(&zip).unwrap()));
    assert_eq!(composed, None);
    assert!(!paths.cache.join("mod-aurora-bliss-update").exists());
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
    assert!(rank(Some("keen6-demo")) < rank(Some("keen6-1.5-ega-nocp")));
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

//! `pnpm index:add <KeenWiki link>`: drafts `index/mods/<id>.yaml` from a mod's
//! KeenWiki page and archive (PLAN.md §10.3), with the parser and the detection
//! the app uses, and says what's left to do by hand. The page and the one
//! archive are fetched as the app's link import does (docs/keenwiki.md).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::catalog;
use crate::detect::sha256_hex;
use crate::keenwiki::{self, WikiDownload, WikiMod};
use crate::readme;
use crate::recipe::{self, Recipe, has_ext};
use crate::source::{Source, split_dir};

use super::ModInfo;
use super::analyze::{analyze, is_keen_file, version_ids};
use super::install::slug;
use super::metadata::is_date;

const USAGE: &str =
    "usage: pnpm index:add <KeenWiki link> [--id ID] [--game GAME] [--download N] [--mods DIR]

  --id ID       the entry's id, when the one from the mod's name doesn't fit
  --game GAME   keen1 ... keen6, kdreams, for a mod neither page nor files tell
                (source mods: the page names none as their episode)
  --download N  the page's Nth download instead of its first, the newest
  --mods DIR    write there instead of index/mods";

/// The first app that takes a mod without any file it knows as Keen's, where
/// the entry names the game (Foray in the Forest).
const SOURCE_MOD_APP_VERSION: &str = "1.1.0";

struct Options {
    link: String,
    id: Option<String>,
    game: Option<String>,
    download: usize,
    mods: PathBuf,
}

fn repo() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap_or(manifest_dir).to_path_buf()
}

/// Run by `examples/index_add.rs`.
pub fn index_add(args: Vec<String>) -> ExitCode {
    let result = options(args).and_then(|o| {
        let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        runtime.block_on(run(&o))
    });
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn options(args: Vec<String>) -> Result<Options, String> {
    let mut o = Options {
        link: String::new(),
        id: None,
        game: None,
        download: 1,
        mods: repo().join("index/mods"),
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--id" => o.id = Some(value()?),
            "--game" => {
                let game = value()?;
                if catalog::game(&game).is_none() {
                    return Err(format!("unknown game {game}\n\n{USAGE}"));
                }
                o.game = Some(game);
            }
            "--download" => {
                o.download = value()?
                    .parse()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or("--download needs a number from 1")?;
            }
            "--mods" => o.mods = PathBuf::from(value()?),
            _ if arg.starts_with("--") || !o.link.is_empty() => {
                return Err(format!("unexpected argument {arg}\n\n{USAGE}"));
            }
            _ => o.link = arg,
        }
    }
    if o.link.is_empty() {
        return Err(USAGE.to_string());
    }
    Ok(o)
}

/// Whether the check of the entry passed.
async fn run(o: &Options) -> Result<bool, String> {
    let text = |e: crate::error::Error| e.to_string();
    let page = keenwiki::page_url(&o.link).map_err(text)?;
    let entries = entries(&o.mods)?;
    if let Some((id, _)) = entries.iter().find(|(_, yaml)| links_page(yaml, &page)) {
        return Err(format!("{page} is in the index already, as `{id}`"));
    }

    let tmp = repo().join(".local/tmp/index-add");
    eprintln!("reading {page}");
    let html = keenwiki::fetch(&tmp.join("pages"), &page)
        .await
        .map_err(text)?;
    let wiki = keenwiki::parse(&html, &page).map_err(text)?;
    let download = wiki
        .downloads
        .get(o.download - 1)
        .ok_or(format!(
            "the page lists {} download(s), see {page}",
            wiki.downloads.len()
        ))?
        .clone();

    let id = o.id.clone().unwrap_or_else(|| slug(&wiki.name));
    let target = o.mods.join(format!("{id}.yaml"));
    if target.exists() {
        return Err(format!(
            "{} exists; name the new entry with --id",
            target.display()
        ));
    }

    eprintln!("downloading {}", download.url);
    let dir = tmp.join("download");
    let archive = keenwiki::download(&dir, &download.url, &|_, _| {})
        .await
        .map_err(text)?;
    let data = fs::read(&archive).map_err(|e| e.to_string())?;
    let sha256 = sha256_hex(&data);
    if let Some((id, _)) = entries.iter().find(|(_, yaml)| yaml.contains(&sha256)) {
        return Err(format!(
            "this download is in the index already, as `{id}` (its page: {page})"
        ));
    }

    let draft = draft(
        &Facts {
            id: id.clone(),
            game: o.game.as_deref().or(wiki.game_id.as_deref()),
            today: today(),
            wiki: &wiki,
            download: &download,
            sha256: sha256.clone(),
            size: data.len() as u64,
        },
        &archive,
    )
    .map_err(|e| {
        let hint = match (&e, wiki.game_id.is_none() && o.game.is_none()) {
            (crate::error::Error::NoModFound, true) => {
                "; the page names no game, try --game keen1 ... keen6 or kdreams. "
            }
            _ => ". ",
        };
        format!("{e}{hint}The archive is at {}", archive.display())
    })?;
    fs::write(&target, &draft.yaml).map_err(|e| e.to_string())?;

    // Where `pnpm index:check` looks, so it doesn't download the file again.
    let cache = repo().join(".local/mods/index-cache");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let cached = cache.join(format!("{sha256}.zip"));
    fs::rename(&archive, &cached).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&dir);
    let readmes = save_readmes(&tmp.join(&id), &draft.readmes)?;

    let passed = check(&id, &o.mods);
    report(&target, &cached, &readmes, &draft, passed);
    Ok(passed)
}

/// The id and text of each entry.
fn entries(mods: &Path) -> Result<Vec<(String, String)>, String> {
    let dir = match fs::read_dir(mods) {
        Ok(dir) => dir,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", mods.display())),
    };
    let mut out = Vec::new();
    for path in dir.filter_map(|e| e.ok().map(|e| e.path())) {
        if path.extension().is_some_and(|e| e == "yaml") {
            let id = path.file_stem().unwrap_or_default().to_string_lossy();
            let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            out.push((id.into_owned(), text));
        }
    }
    Ok(out)
}

fn links_page(yaml: &str, page: &str) -> bool {
    yaml.lines().any(|l| {
        l.trim()
            .strip_prefix("keenwiki:")
            .is_some_and(|url| keenwiki::same_page(url.trim().trim_matches('"'), page))
    })
}

/// `YYYY-MM-DD`, in UTC.
fn today() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    date_of(secs / 86_400)
}

/// The date `days` after 1970-01-01 (Hinnant's `civil_from_days`).
fn date_of(days: u64) -> String {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

struct Facts<'a> {
    id: String,
    /// What `--game` or the page says.
    game: Option<&'a str>,
    today: String,
    wiki: &'a WikiMod,
    download: &'a WikiDownload,
    sha256: String,
    size: u64,
}

struct Draft {
    yaml: String,
    /// What the entry lacks.
    todo: Vec<String>,
    /// What the draft guessed, or what may need a field it doesn't set.
    check: Vec<String>,
    root: String,
    /// Bare names in the mod's root.
    names: Vec<String>,
    readmes: Vec<readme::Readme>,
}

/// The entry for the mod `facts` tell about, whose archive is at `path`.
fn draft(facts: &Facts, path: &Path) -> crate::error::Result<Draft> {
    let source = Source::open(path)?;
    let analysis = analyze(&source, path, facts.game)?;
    let info = &analysis.info;
    let root = analysis.root.as_str();
    let names: Vec<String> = analysis
        .files
        .iter()
        .filter(|f| split_dir(f).0 == root)
        .map(|f| split_dir(f).1.to_string())
        .collect();
    let read = |name: &str| {
        let file = if root.is_empty() {
            name.to_string()
        } else {
            format!("{root}/{name}")
        };
        let bytes = source.read(&file).ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    };
    let start = info
        .recipes
        .first()
        .ok_or(crate::error::Error::NoModFound)?;

    // What the patch that starts the mod names; a batch file's as well.
    let patches: Vec<String> = match &start.recipe {
        Recipe::Patch { patch, .. } => vec![patch.clone()],
        Recipe::Batch { file } => read(file)
            .map(|t| recipe::batch_commands(&t))
            .unwrap_or_default()
            .into_iter()
            .filter(|cmd| cmd.len() > 1 && recipe::is_patcher(&format!("{}.EXE", cmd[0])))
            .map(|cmd| cmd[1].clone())
            .collect(),
        Recipe::Program { .. } | Recipe::Episode => Vec::new(),
    };
    let mut named: Vec<String> = patches
        .iter()
        .filter_map(|p| read(p))
        .flat_map(|t| recipe::patch_versions(&t))
        .collect();
    named.sort();
    named.dedup();
    let game = catalog::game(&info.game_id);
    // Third-party builds fit through the version they're made from.
    let requires: Vec<String> = game.map_or(Vec::new(), |g| {
        let ids = version_ids(g, &named);
        g.versions
            .iter()
            .filter(|v| v.original.is_none() && ids.contains(&v.id))
            .map(|v| v.id.clone())
            .collect()
    });

    let folders = keen_folders(&analysis.files, root);
    let is_game =
        |n: &str| game.is_some_and(|g| g.executables.iter().any(|e| e.eq_ignore_ascii_case(n)));
    let programs: Vec<String> = names
        .iter()
        .filter(|n| {
            (has_ext(n, "EXE") || has_ext(n, "COM")) && !recipe::is_patcher(n) && !is_game(n)
        })
        .cloned()
        .collect();
    // It has nothing of the game's, so only the page's game makes it a mod.
    let source_mod = analyze(&source, path, None).is_err();
    let readmes = readme::find(&source).unwrap_or_default();

    let found = Found {
        info,
        named,
        requires,
        folders,
        programs,
        names: names.clone(),
        source_mod,
    };
    let (todo, check) = notes(facts, &found);
    Ok(Draft {
        yaml: yaml(facts, &found),
        todo,
        check,
        root: analysis.root.clone(),
        names,
        readmes,
    })
}

/// What the archive says.
struct Found<'a> {
    info: &'a ModInfo,
    /// Versions the patch that starts the mod names.
    named: Vec<String>,
    /// The catalog's ids for them.
    requires: Vec<String>,
    /// Folders right in the mod's root with Keen files in them.
    folders: Vec<String>,
    /// Programs in the root, patchers and the game's aside.
    programs: Vec<String>,
    names: Vec<String>,
    source_mod: bool,
}

impl Found<'_> {
    /// A source mod that runs a program of its own plays without its game.
    fn standalone(&self) -> bool {
        self.source_mod
            && matches!(
                self.info.recipes[0].recipe,
                Recipe::Program { .. } | Recipe::Batch { .. }
            )
    }
}

fn keen_folders(files: &[String], root: &str) -> Vec<String> {
    let mut folders: Vec<String> = files
        .iter()
        .filter_map(|f| {
            let (dir, name) = split_dir(f);
            let inner = if root.is_empty() {
                dir
            } else {
                dir.strip_prefix(root)?.strip_prefix('/')?
            };
            let folder = inner.split('/').next().filter(|s| !s.is_empty())?;
            is_keen_file(name).then(|| folder.to_string())
        })
        .collect();
    folders.sort();
    folders.dedup();
    folders
}

fn kind(facts: &Facts) -> &'static str {
    let file = crate::downloads::url_file_name(&facts.download.url);
    if file.to_ascii_lowercase().starts_with("l.") {
        "levelpack"
    } else {
        "episode"
    }
}

fn version<'a>(facts: &'a Facts, found: &'a Found) -> Option<&'a str> {
    facts
        .download
        .version
        .as_deref()
        .or(found.info.version.as_deref())
}

/// The page's date when it's a full one.
fn date(page: Option<&str>) -> Option<&str> {
    page.filter(|d| is_date(d))
}

fn released<'a>(facts: &'a Facts, found: &'a Found) -> Option<&'a str> {
    date(facts.wiki.released.as_deref()).or(found.info.released.as_deref())
}

fn authors<'a>(facts: &'a Facts, found: &'a Found) -> &'a [String] {
    if facts.wiki.authors.is_empty() {
        &found.info.authors
    } else {
        &facts.wiki.authors
    }
}

/// A YAML string; JSON's are valid YAML.
fn quoted(s: &str) -> String {
    serde_json::to_string(s).expect("strings serialize")
}

/// Bare where YAML takes it as the same string: URLs and file names.
fn scalar(s: &str) -> String {
    let plain = s.starts_with(|c: char| c.is_ascii_alphabetic())
        && !s.contains(": ")
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:%?=&+~".contains(c))
        && !["true", "false", "null", "yes", "no", "on", "off"]
            .iter()
            .any(|w| w.eq_ignore_ascii_case(s));
    if plain { s.to_string() } else { quoted(s) }
}

fn list(items: &[String]) -> String {
    let items: Vec<String> = items.iter().map(|s| quoted(s)).collect();
    format!("[{}]", items.join(", "))
}

fn recipe_yaml(recipe: &Recipe) -> String {
    let args = |args: &[String]| {
        if args.is_empty() {
            String::new()
        } else {
            format!("  args: {}\n", list(args))
        }
    };
    match recipe {
        Recipe::Patch {
            patcher,
            patch,
            args: a,
        } => format!(
            "recipe:\n  type: patch\n  patcher: {}\n  patch: {}\n{}",
            scalar(patcher),
            scalar(patch),
            args(a)
        ),
        Recipe::Program { program, args: a } => format!(
            "recipe:\n  type: program\n  program: {}\n{}",
            scalar(program),
            args(a)
        ),
        Recipe::Batch { file } => format!("recipe:\n  type: batch\n  file: {}\n", scalar(file)),
        Recipe::Episode => "recipe:\n  type: episode\n".to_string(),
    }
}

/// `text` as comment lines of about 78 columns.
fn comment(text: &str) -> String {
    let mut out = String::new();
    let mut line = String::from("#  ");
    for word in text.split_whitespace() {
        if line.len() + word.len() > 77 && line.len() > 3 {
            out += line.trim_end();
            out.push('\n');
            line = String::from("#  ");
        }
        line.push(' ');
        line += word;
    }
    out += &line;
    out.push('\n');
    out
}

fn yaml(facts: &Facts, found: &Found) -> String {
    let wiki = facts.wiki;
    let info = found.info;
    let mut y = format!(
        "id: {}\nname: {}\ngame: {}\n",
        facts.id,
        quoted(&wiki.name),
        info.game_id
    );
    if !found.requires.is_empty() {
        y += &format!("requires: [{}]\n", found.requires.join(", "));
    }
    let authors = authors(facts, found);
    y += &format!("authors: {}\n", list(authors));
    let author_links: Vec<_> = wiki
        .author_links
        .iter()
        .filter(|(name, url)| authors.contains(name) && url.starts_with("https://"))
        .collect();
    if !author_links.is_empty() {
        y += "authorLinks:\n";
        for (name, url) in author_links {
            y += &format!("  {}: {}\n", quoted(name), scalar(url));
        }
    }
    if let Some(v) = version(facts, found) {
        y += &format!("version: {}\n", quoted(v));
    }
    let released = released(facts, found);
    if let Some(d) = released {
        y += &format!("released: {d}\n");
    }
    if let Some(d) = date(wiki.updated.as_deref()).filter(|d| Some(*d) != released) {
        y += &format!("updated: {d}\n");
    }
    y += &format!("added: {}\ntype: {}\n", facts.today, kind(facts));
    y += "# TODO: description (your own words, in English) and translations.de.description.\n";
    if let Some(d) = &wiki.description {
        y += "# KeenWiki says:\n";
        y += &comment(d);
    }
    y += &format!("links:\n  keenwiki: {}\n", scalar(&wiki.page_url));
    for (field, url) in [
        ("pckf", &wiki.pckf),
        ("keenmodding", &wiki.keenmodding),
        ("homepage", &wiki.homepage),
    ] {
        match url {
            Some(url) if url.starts_with("https://") => {
                y += &format!("  {field}: {}\n", scalar(url));
            }
            // The check takes only https.
            Some(url) => y += &format!("  # {field}: {url}\n"),
            None => {}
        }
    }
    y += "downloads:\n";
    let mut first = "  - ";
    if let Some(v) = version(facts, found) {
        y += &format!("{first}version: {}\n", quoted(v));
        first = "    ";
    }
    y += &format!(
        "{first}url: {}\n    sha256: {}\n    size: {}\n",
        scalar(&facts.download.url),
        facts.sha256,
        facts.size
    );
    if found.standalone() {
        y += "# A source mod: it ships its whole game and plays without a copy of it.\n";
        y += "standalone: true\n";
    }
    y += &recipe_yaml(&info.recipes[0].recipe);
    if found.source_mod {
        y += "# Older apps find no mod in it, as it has no Keen files.\n";
        y += &format!("minAppVersion: {SOURCE_MOD_APP_VERSION}\n");
    }
    y
}

/// What's left to fill in, and what to look at.
fn notes(facts: &Facts, found: &Found) -> (Vec<String>, Vec<String>) {
    let wiki = facts.wiki;
    let info = found.info;
    let start = &info.recipes[0];
    let mut todo = vec![
        "description and translations.de.description: not written, they have to be your own \
         words (the file has the page's text as a comment; remove the TODO)"
            .to_string(),
    ];
    let mut check = Vec::new();

    if authors(facts, found).is_empty() {
        todo.push("authors: neither the page nor the archive name them".into());
    }
    if version(facts, found).is_none() {
        todo.push(
            "version: neither the page nor the archive name say; add it to the entry and its \
             download if the readme does"
                .into(),
        );
    }
    if let Some(v) = version(facts, found) {
        let file = crate::downloads::url_file_name(&facts.download.url);
        if longer_in_name(file, v) {
            check.push(format!(
                "version: {v} from the page, but the archive is called {file}"
            ));
        }
    }
    match (wiki.released.as_deref(), released(facts, found)) {
        (Some(page), Some(d)) if page != d => check.push(format!(
            "released: the page says \"{page}\", which isn't a full date; took the archive \
             name's {d}"
        )),
        (Some(page), None) => todo.push(format!(
            "released: the page says \"{page}\", which isn't a full date"
        )),
        (None, Some(d)) => check.push(format!(
            "released: the page has no date; took the archive name's {d}, which may be the \
             latest release's instead"
        )),
        (None, None) => todo.push("released: neither the page nor the archive name say".into()),
        _ => {}
    }
    if let Some(page) = &wiki.updated
        && !is_date(page)
    {
        todo.push(format!(
            "updated: the page says \"{page}\", which isn't a full date"
        ));
    }

    let command = start
        .recipe
        .command()
        .unwrap_or_else(|| "the game's own program".to_string());
    let origin = if start.origin.is_empty() || start.origin.eq_ignore_ascii_case(&command) {
        String::new()
    } else {
        format!(" (from {})", start.origin)
    };
    check.push(format!(
        "recipe: `{command}`{origin} is a guess; start the mod in the app (`pnpm index:build`, \
         then a debug build) and compare with the readme"
    ));
    if !info.confident || info.recipes.len() > 1 {
        let others: Vec<String> = info.recipes[1..]
            .iter()
            .map(|o| {
                let command = o.recipe.command().unwrap_or_else(|| "episode".into());
                match o.game_versions.as_slice() {
                    [] => format!("`{command}`"),
                    v => format!("`{command}` (for {})", v.join(", ")),
                }
            })
            .collect();
        let also = if others.is_empty() {
            String::new()
        } else {
            format!("; also found: {}", others.join(", "))
        };
        let certainty = if info.confident {
            "several ways to start it"
        } else {
            "the detection isn't sure"
        };
        check.push(format!(
            "recipe: {certainty}{also}. Patches for different game versions go into \
             versionRecipes (needs minAppVersion)"
        ));
    }
    if let Recipe::Patch { patcher, .. } = &start.recipe
        && !found.names.iter().any(|n| n.eq_ignore_ascii_case(patcher))
    {
        check.push(format!(
            "recipe: the archive has no {patcher}, the app would use its own; the index check \
             wants the recipe's files in the archive"
        ));
    }
    if !start.missing_files.is_empty() {
        check.push(format!(
            "extends: the mod lacks {}, so it may be meant to go over another mod; such packs \
             usually need no entry, otherwise set extends (needs minAppVersion) and the other \
             entry's provides",
            start.missing_files.join(", ")
        ));
    }
    if let Some(made_for) = &info.made_for {
        check.push(format!(
            "extends: the archive name says these levels are for {made_for}"
        ));
    }

    match (found.named.is_empty(), found.requires.is_empty()) {
        (true, _) => check.push(
            "requires: left out (any version), as no patch names one; add it if the readme \
             names one"
                .to_string(),
        ),
        (false, true) => check.push(format!(
            "requires: left out (any version), as the catalog doesn't know {}, which the patch \
             names",
            found.named.join(", ")
        )),
        (false, false) => {}
    }
    if wiki.game_id.as_deref() != Some(info.game_id.as_str()) {
        check.push(format!(
            "game: {} by the files; the page says {}",
            info.game_id,
            wiki.game_id.as_deref().unwrap_or("nothing")
        ));
    }

    check.push(format!(
        "type: {} is a guess; levelpack for new levels only, conversion if it plays unlike Keen",
        kind(facts)
    ));
    if !found.folders.is_empty() {
        check.push(format!(
            "copyToRoot: {} in the mod's root hold Keen files; set it (needs minAppVersion) if \
             the readme says to copy them into the root",
            found.folders.join(", ")
        ));
    }
    if !matches!(start.recipe, Recipe::Patch { .. }) && found.programs.len() > 1 {
        check.push(format!(
            "cover: the root has {}; name the one with the title screen if the recipe runs a \
             menu",
            found.programs.join(", ")
        ));
    }
    if found.source_mod {
        check.push(format!(
            "minAppVersion: set to {SOURCE_MOD_APP_VERSION}, as the archive has no Keen files \
             (a source mod); also see if it plays on any copy"
        ));
    }
    if found.standalone() {
        check.push(format!(
            "standalone: set, as the archive has nothing of {}'s; take it out if the mod still \
             loads the game's own files (a lone executable)",
            found.info.game_id
        ));
    }

    let others: Vec<String> = wiki
        .downloads
        .iter()
        .filter(|d| d.url != facts.download.url)
        .map(|d| match &d.note {
            Some(note) => format!("{} ({note})", d.url),
            None => d.url.clone(),
        })
        .collect();
    if !others.is_empty() {
        check.push(format!(
            "downloads: the page has others too, not in the entry: {}",
            others.join(", ")
        ));
    }
    check.push("featured: not set; a handful per game are, for the setup wizard".to_string());
    if let Some(status) = &wiki.status
        && !status.eq_ignore_ascii_case("complete")
    {
        check.push(format!("status: the page calls the mod \"{status}\""));
    }
    for (field, url) in [
        ("pckf", &wiki.pckf),
        ("keenmodding", &wiki.keenmodding),
        ("homepage", &wiki.homepage),
    ] {
        if let Some(url) = url.as_deref().filter(|u| !u.starts_with("https://")) {
            check.push(format!(
                "links.{field}: commented out, the entry takes only https: {url}"
            ));
        }
    }
    for (name, url) in &wiki.author_links {
        if !url.starts_with("https://") {
            check.push(format!(
                "authorLinks: left out {name}'s, the entry takes only https: {url}"
            ));
        }
    }
    (todo, check)
}

/// Whether the archive name goes on after `version`: the page's `2.0` for
/// `..._v2.0a.sc.2025-04-30.zip`.
fn longer_in_name(file: &str, version: &str) -> bool {
    file.match_indices(version).any(|(i, _)| {
        file[i + version.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    })
}

/// Writes the readable documents into `dir`, for looking things up.
fn save_readmes(dir: &Path, docs: &[readme::Readme]) -> Result<Vec<PathBuf>, String> {
    let _ = fs::remove_dir_all(dir);
    let mut out = Vec::new();
    for doc in docs.iter().filter(|d| !d.text.is_empty()) {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let name: String = doc
            .file
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "._-".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let name = if has_ext(&name, "TXT") {
            name
        } else {
            format!("{name}.txt")
        };
        let path = dir.join(name);
        fs::write(&path, &doc.text).map_err(|e| e.to_string())?;
        out.push(path);
    }
    Ok(out)
}

/// Runs the index's own check on the new entry, its download included.
fn check(id: &str, mods: &Path) -> bool {
    let status = Command::new("cargo")
        .args(["run", "--quiet", "--manifest-path"])
        .arg(repo().join("index/tool/Cargo.toml"))
        .args(["--", "check", "--download", "--only", id, "--mods"])
        .arg(mods)
        .status();
    match status {
        Ok(status) => status.success(),
        Err(e) => {
            eprintln!("couldn't run the check: {e}");
            false
        }
    }
}

fn report(target: &Path, archive: &Path, readmes: &[PathBuf], draft: &Draft, passed: bool) {
    let shown = |p: &Path| {
        let repo = repo();
        p.strip_prefix(&repo)
            .unwrap_or(p)
            .display()
            .to_string()
            .replace('\\', "/")
    };
    println!("\nWrote {}", shown(target));
    println!("Archive: {}", shown(archive));
    let root = if draft.root.is_empty() {
        "the archive's root"
    } else {
        draft.root.as_str()
    };
    println!("Mod's root: {root}: {}", draft.names.join(", "));
    for path in readmes {
        println!("Readme: {}", shown(path));
    }
    for doc in draft.readmes.iter().filter(|d| d.text.is_empty()) {
        println!("Readme, only in the archive: {}", doc.file);
    }
    println!(
        "Index check: {}",
        if passed {
            "passed"
        } else {
            "FAILED, see above"
        }
    );
    println!("\nStill to fill in:");
    for note in &draft.todo {
        println!("  - {note}");
    }
    println!("\nTo check by hand:");
    for note in &draft.check {
        println!("  - {note}");
    }
    println!(
        "\nThen: pnpm exec prettier --write {}, pnpm index:check, commit (index/README.md).",
        shown(target)
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::source::testutil::write_zip;

    const URL: &str =
        "https://files.shikadi.net/keenwiki/mods/Keen4/m.a.aurora_v1.2.k4.2023-08-23.zip";

    fn page() -> WikiMod {
        WikiMod {
            page_url: "https://keenwiki.shikadi.net/wiki/Aurora_Bliss".into(),
            name: "Aurora Bliss".into(),
            game_id: Some("keen4".into()),
            authors: vec!["Dave216".into()],
            author_links: BTreeMap::from([(
                "Dave216".to_string(),
                "https://keenwiki.shikadi.net/wiki/User:Dave216".to_string(),
            )]),
            status: Some("Complete".into()),
            released: Some("2022-08-01".into()),
            updated: Some("2023-08-23".into()),
            levels: None,
            pckf: Some("https://pckf.com/viewtopic.php?t=13985".into()),
            keenmodding: None,
            homepage: None,
            description: Some("Keen finds spare parts.".into()),
            credits: vec![],
            downloads: vec![WikiDownload {
                url: URL.into(),
                label: "newest".into(),
                note: None,
                version: Some("1.2".into()),
            }],
        }
    }

    fn draft_of(wiki: &WikiMod, archive: &str, files: &[(&str, &[u8])]) -> Draft {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(archive);
        write_zip(&path, files);
        let facts = Facts {
            id: "aurora-bliss".into(),
            game: wiki.game_id.as_deref(),
            today: "2026-11-06".into(),
            wiki,
            download: &wiki.downloads[0],
            sha256: "ab".repeat(32),
            size: 10,
        };
        draft(&facts, &path).unwrap()
    }

    const PATCH_MOD: &[(&str, &[u8])] = &[
        ("Aurora/CK4PATCH.EXE", b"patcher"),
        ("Aurora/abiathar.pat", b"%ext ck4\n%version 1.4\n"),
        ("Aurora/GAMEMAPS.CK4", b"maps"),
    ];

    #[test]
    fn writes_the_entry_from_the_page_and_the_archive() {
        let d = draft_of(&page(), "m.a.aurora_v1.2.k4.2023-08-23.zip", PATCH_MOD);
        assert_eq!(
            d.yaml,
            format!(
                r#"id: aurora-bliss
name: "Aurora Bliss"
game: keen4
requires: [keen4-1.4-ega]
authors: ["Dave216"]
authorLinks:
  "Dave216": https://keenwiki.shikadi.net/wiki/User:Dave216
version: "1.2"
released: 2022-08-01
updated: 2023-08-23
added: 2026-11-06
type: episode
# TODO: description (your own words, in English) and translations.de.description.
# KeenWiki says:
#   Keen finds spare parts.
links:
  keenwiki: https://keenwiki.shikadi.net/wiki/Aurora_Bliss
  pckf: https://pckf.com/viewtopic.php?t=13985
downloads:
  - version: "1.2"
    url: {URL}
    sha256: {}
    size: 10
recipe:
  type: patch
  patcher: CK4PATCH.EXE
  patch: abiathar.pat
"#,
                "ab".repeat(32)
            )
        );
        assert_eq!(d.root, "Aurora");
        assert_eq!(d.todo.len(), 1, "{:?}", d.todo);
        assert!(d.check.iter().any(|n| n.starts_with("recipe: `")));
        assert!(!d.check.iter().any(|n| n.starts_with("requires")));
    }

    #[test]
    fn requires_comes_from_the_patch_a_batch_file_runs() {
        let files: &[(&str, &[u8])] = &[
            ("PLAY.BAT", b"@echo off\r\nCK4PATCH mod.pat\r\n"),
            ("CK4PATCH.EXE", b"patcher"),
            ("mod.pat", b"%ext ck4\n%version 1.4\n"),
        ];
        let d = draft_of(&page(), "m.a.aurora_v1.2.k4.2023-08-23.zip", files);
        assert!(d.yaml.contains("requires: [keen4-1.4-ega]\n"), "{}", d.yaml);
    }

    #[test]
    fn says_what_the_page_and_the_files_leave_open() {
        let mut wiki = page();
        wiki.released = Some("2022".into());
        wiki.updated = None;
        wiki.status = Some("Beta".into());
        wiki.game_id = Some("keen5".into());
        wiki.downloads[0].version = None;
        wiki.pckf = Some("http://example.com/thread".into());
        wiki.downloads.push(WikiDownload {
            url: "https://example.com/old.zip".into(),
            label: "old".into(),
            note: Some("less buggy".into()),
            version: None,
        });
        let files: &[(&str, &[u8])] = &[("Mod/KEEN4E.EXE", b"x"), ("Mod/1/LEVEL01.CK4", b"l")];
        let d = draft_of(&wiki, "mod.zip", files);
        let notes = format!("{:#?}", (&d.todo, &d.check));
        for wanted in [
            "released: the page says \\\"2022\\\"",
            "version: neither",
            "requires: left out",
            "game: keen4 by the files; the page says keen5",
            "copyToRoot: 1 in the mod's root",
            "https://example.com/old.zip (less buggy)",
            "status: the page calls the mod \\\"Beta\\\"",
            "links.pckf: commented out",
        ] {
            assert!(notes.contains(wanted), "{wanted} in {notes}");
        }
        assert!(d.yaml.contains("  # pckf: http://example.com/thread\n"));
        assert!(!d.yaml.contains("released:") && !d.yaml.contains("\nversion:"));
    }

    #[test]
    fn a_level_pack_and_a_source_mod() {
        let mut wiki = page();
        wiki.downloads[0].url = "https://example.com/l.a.k4.2020-01-02.zip".into();
        let d = draft_of(&wiki, "l.a.k4.2020-01-02.zip", &[("LEVEL01.CK4", b"l")]);
        assert!(d.yaml.contains("type: levelpack\n"), "{}", d.yaml);

        let files: &[(&str, &[u8])] = &[
            ("FITF.EXE", b"menu"),
            ("FITF-C.EXE", b"game"),
            ("FITF-X.EXE", b"game"),
            ("GAMEMAPS.CKS", b"maps"),
        ];
        let d = draft_of(&page(), "foray.zip", files);
        assert!(d.yaml.contains("minAppVersion: 1.1.0\n"), "{}", d.yaml);
        assert!(d.yaml.contains("standalone: true\nrecipe:"), "{}", d.yaml);
        assert!(
            d.check.iter().any(|n| n.starts_with("standalone: ")),
            "{:?}",
            d.check
        );
        assert!(
            d.check.iter().any(|n| n.starts_with("cover: ")),
            "{:?}",
            d.check
        );
    }

    #[test]
    fn scalars_stay_bare_only_where_yaml_agrees() {
        assert_eq!(scalar("https://a.b/c_d.zip?x=1"), "https://a.b/c_d.zip?x=1");
        assert_eq!(scalar("CK4PATCH.EXE"), "CK4PATCH.EXE");
        assert_eq!(scalar("1.PAT"), "\"1.PAT\"");
        assert_eq!(scalar("no"), "\"no\"");
        assert_eq!(scalar("my mod.pat"), "\"my mod.pat\"");
    }

    #[test]
    fn a_version_cut_short_by_the_page() {
        assert!(longer_in_name("m.a.foray_v2.0a.sc.2025-04-30.zip", "2.0"));
        assert!(!longer_in_name("m.a.aurora_v.1.2.k4.2023-08-23.zip", "1.2"));
        assert!(!longer_in_name("mod.zip", "1.2"));
    }

    #[test]
    fn dates_and_known_pages() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(20_513), "2026-03-01");
        assert_eq!(date_of(20_763), "2026-11-06");
        let yaml = "links:\n  keenwiki: https://keenwiki.shikadi.net/wiki/The_Alphamatic\n";
        assert!(links_page(yaml, "keenwiki.shikadi.net/wiki/the_Alphamatic"));
        assert!(!links_page(yaml, "https://keenwiki.shikadi.net/wiki/Other"));
    }

    #[test]
    fn options_need_a_link() {
        assert!(options(vec![]).is_err());
        assert!(options(vec!["a".into(), "b".into()]).is_err());
        assert!(options(vec!["a".into(), "--download".into(), "0".into()]).is_err());
        assert!(options(vec!["a".into(), "--game".into(), "keen9".into()]).is_err());
        let o = options(vec!["a".into(), "--id".into(), "x".into()]).unwrap();
        assert_eq!(
            (o.link.as_str(), o.id.as_deref(), o.download),
            ("a", Some("x"), 1)
        );
    }
}

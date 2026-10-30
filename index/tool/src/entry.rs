//! One mod of the index, as written in `index/mods/<id>.yaml` and published
//! in `index.json` (PLAN.md §10.1), and the checks every entry has to pass.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub name: String,
    /// Catalog game id, e.g. `keen4`.
    pub game: String,
    /// Catalog version ids the mod is known to work with; empty: any.
    #[serde(default)]
    pub requires: Vec<String>,
    pub authors: Vec<String>,
    /// Author name → their page, e.g. their KeenWiki user page.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub author_links: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `YYYY-MM-DD`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    /// When the entry joined the index, `YYYY-MM-DD`; drives "New" badges.
    pub added: String,
    #[serde(rename = "type")]
    pub kind: Kind,
    /// In English.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Language code (one of [`LANGUAGES`]) → the texts in that language.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub translations: BTreeMap<String, Translation>,
    #[serde(default)]
    pub links: Links,
    /// Newest first; the app installs the first one.
    pub downloads: Vec<Download>,
    pub recipe: Recipe,
    /// Catalog version id → recipe used instead of `recipe` on it, for mods
    /// that start differently per game version.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub version_recipes: BTreeMap<String, Recipe>,
    /// Folders in the mod's root whose files the readme says to copy into
    /// the root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copy_to_root: Vec<String>,
    /// The entry this mod is played over, e.g. a level pack for a mod: its
    /// files go below this one's, and the recipe may run them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    /// Files of this mod that mods copied over it lack, e.g. its start batch:
    /// the app finds the mod they need by them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub featured: bool,
    /// Oldest app version that can install this entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,
    /// For maintainers, e.g. why the entry links somewhere unusual.
    #[serde(default, skip_serializing)]
    pub license_note: Option<String>,
}

/// The languages of the app besides English, which entries are written in.
pub const LANGUAGES: &[&str] = &["de"];

/// An entry's texts in another language.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Translation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// A new adventure: levels and usually graphics, sounds, story.
    Episode,
    /// New levels for the original game.
    Levelpack,
    /// Changes the game into something else entirely.
    Conversion,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Links {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keenwiki: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pckf: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keenmodding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Download {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub url: String,
    pub sha256: String,
    /// Bytes.
    pub size: u64,
}

/// How the app starts the mod; same shape as the app's `Recipe`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Recipe {
    Patch {
        patcher: String,
        patch: String,
        #[serde(default)]
        args: Vec<String>,
    },
    Program {
        program: String,
        #[serde(default)]
        args: Vec<String>,
    },
    Batch {
        file: String,
    },
    Episode,
}

impl Entry {
    /// `recipe` and the ones for particular game versions.
    pub fn recipes(&self) -> impl Iterator<Item = &Recipe> {
        std::iter::once(&self.recipe).chain(self.version_recipes.values())
    }
}

impl Recipe {
    /// Files the recipe runs, which have to be in the mod's folder.
    pub fn files(&self) -> Vec<&str> {
        match self {
            Recipe::Patch { patcher, patch, .. } => vec![patcher, patch],
            Recipe::Program { program, .. } => vec![program],
            Recipe::Batch { file } => vec![file],
            Recipe::Episode => vec![],
        }
    }
}

/// Game id → its catalog version ids, from the app's `games.json`.
pub type Catalog = BTreeMap<String, BTreeSet<String>>;

pub fn load_catalog(path: &Path) -> Result<Catalog, String> {
    #[derive(Deserialize)]
    struct File {
        games: Vec<Game>,
    }
    #[derive(Deserialize)]
    struct Game {
        id: String,
        versions: Vec<Version>,
    }
    #[derive(Deserialize)]
    struct Version {
        id: String,
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file: File = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(file
        .games
        .into_iter()
        .map(|g| (g.id, g.versions.into_iter().map(|v| v.id).collect()))
        .collect())
}

pub fn parse(yaml: &str) -> Result<Entry, String> {
    serde_yaml_ng::from_str(yaml).map_err(|e| e.to_string())
}

/// Everything wrong with an entry that can be told without downloading.
pub fn problems(entry: &Entry, file_stem: &str, catalog: &Catalog) -> Vec<String> {
    let mut out = Vec::new();
    let mut fail = |msg: String| out.push(msg);

    if entry.id != file_stem {
        fail(format!("id `{}` must match the file name", entry.id));
    }
    if !is_slug(&entry.id) {
        fail("id must be lower-case letters, digits and single dashes".into());
    }
    if entry.name.trim().is_empty() {
        fail("name is empty".into());
    }
    match catalog.get(&entry.game) {
        None => fail(format!("unknown game `{}`", entry.game)),
        Some(versions) => {
            for v in &entry.requires {
                if !versions.contains(v) {
                    fail(format!("`{v}` is not a version of {}", entry.game));
                }
            }
            for v in entry.version_recipes.keys() {
                if !versions.contains(v) {
                    fail(format!(
                        "versionRecipes: `{v}` is not a version of {}",
                        entry.game
                    ));
                } else if !entry.requires.is_empty() && !entry.requires.contains(v) {
                    fail(format!("versionRecipes: `{v}` is not in requires"));
                }
            }
        }
    }
    for (lang, t) in &entry.translations {
        if !LANGUAGES.contains(&lang.as_str()) {
            fail(format!(
                "translations: `{lang}` is not one of {}",
                LANGUAGES.join(", ")
            ));
        }
        match &t.description {
            Some(d) if d.trim().is_empty() => {
                fail(format!("translations.{lang}.description is empty"))
            }
            Some(_) if entry.description.is_none() => fail(format!(
                "translations.{lang}.description translates a missing description"
            )),
            _ => {}
        }
    }
    if entry.authors.is_empty() || entry.authors.iter().any(|a| a.trim().is_empty()) {
        fail("authors must list at least one name".into());
    }
    for (name, url) in &entry.author_links {
        if !entry.authors.contains(name) {
            fail(format!("authorLinks: `{name}` is not one of the authors"));
        }
        if !url.starts_with("https://") {
            fail(format!("authorLinks: {url} must use https"));
        }
    }
    for (field, date) in [
        ("released", entry.released.as_deref()),
        ("updated", entry.updated.as_deref()),
        ("added", Some(entry.added.as_str())),
    ] {
        if let Some(d) = date
            && !is_date(d)
        {
            fail(format!("{field} `{d}` is not a YYYY-MM-DD date"));
        }
    }
    if let Some(v) = &entry.min_app_version
        && !is_version(v)
    {
        fail(format!("minAppVersion `{v}` is not like 1.2.3"));
    }

    if let Some(url) = &entry.links.keenwiki
        && !url.starts_with("https://keenwiki.shikadi.net/wiki/")
    {
        fail("links.keenwiki must be a https://keenwiki.shikadi.net/wiki/ page".into());
    }
    for url in [
        &entry.links.pckf,
        &entry.links.keenmodding,
        &entry.links.homepage,
    ]
    .into_iter()
    .flatten()
    {
        if !url.starts_with("https://") {
            fail(format!("link {url} must use https"));
        }
    }

    if entry.downloads.is_empty() {
        fail("downloads must list at least one file".into());
    }
    for d in &entry.downloads {
        if !(d.url.starts_with("https://") || d.url.starts_with("http://")) {
            fail(format!("download {} is not a web address", d.url));
        }
        if d.sha256.len() != 64 || !d.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            fail(format!("sha256 of {} must be 64 hex digits", d.url));
        }
        if d.size == 0 {
            fail(format!("size of {} is missing", d.url));
        }
    }

    for file in entry.recipes().flat_map(Recipe::files) {
        if !is_plain_name(file) {
            fail(format!(
                "recipe file `{file}` must be a plain DOS file name"
            ));
        }
    }
    for file in &entry.provides {
        if !is_plain_name(file) {
            fail(format!("provides: `{file}` must be a plain DOS file name"));
        }
    }
    for folder in &entry.copy_to_root {
        if !is_plain_name(folder) || folder == "." || folder == ".." {
            fail(format!(
                "copyToRoot: `{folder}` must be a folder right in the mod's root"
            ));
        }
    }
    if (!entry.version_recipes.is_empty()
        || !entry.copy_to_root.is_empty()
        || entry.extends.is_some())
        && entry.min_app_version.is_none()
    {
        fail("versionRecipes, copyToRoot and extends need a minAppVersion".into());
    }
    if entry.extends.as_deref() == Some(entry.id.as_str()) {
        fail("extends names the entry itself".into());
    }
    for t in &entry.tags {
        if !is_slug(t) {
            fail(format!("tag `{t}` must be lower-case with dashes"));
        }
    }
    out
}

/// What's wrong with the `extends` of `entries` among each other: the entry
/// it names has to exist, be for the same game and not extend another one.
pub fn extends_problems(entries: &[Entry]) -> Vec<String> {
    let mut out = Vec::new();
    for e in entries {
        let Some(base_id) = &e.extends else { continue };
        match entries.iter().find(|b| b.id == *base_id) {
            None => out.push(format!("{}: extends unknown entry `{base_id}`", e.id)),
            Some(b) if b.game != e.game => out.push(format!(
                "{}: extends `{base_id}`, a mod for {}",
                e.id, b.game
            )),
            Some(b) if b.extends.is_some() => out.push(format!(
                "{}: extends `{base_id}`, which extends another mod itself",
                e.id
            )),
            Some(_) => {}
        }
    }
    out
}

/// Files more than one entry of a game `provides`: the app couldn't tell
/// which mod lacks them.
pub fn provides_problems(entries: &[Entry]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        for file in &e.provides {
            let other = entries[..i].iter().find(|o| {
                o.game == e.game && o.provides.iter().any(|f| f.eq_ignore_ascii_case(file))
            });
            if let Some(o) = other {
                out.push(format!("{}: `{file}` is provided by `{}` too", e.id, o.id));
            }
        }
    }
    out
}

fn is_plain_name(s: &str) -> bool {
    !s.is_empty() && !s.contains(['/', '\\', ' '])
}

fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

fn is_version(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AURORA: &str = r#"
id: aurora-bliss
name: Aurora Bliss
game: keen4
requires: [keen4-1.4-ega]
authors: [Dave216]
authorLinks:
  Dave216: https://keenwiki.shikadi.net/wiki/User:Dave216
version: "1.2"
released: 2023-08-23
added: 2026-10-17
type: episode
links:
  keenwiki: https://keenwiki.shikadi.net/wiki/Aurora_Bliss
downloads:
  - url: https://files.shikadi.net/keenwiki/x.zip
    sha256: 0000000000000000000000000000000000000000000000000000000000000000
    size: 10
recipe:
  type: patch
  patcher: CK4PATCH.EXE
  patch: abiathar.pat
"#;

    fn catalog() -> Catalog {
        let mut c = Catalog::new();
        c.insert("keen4".into(), ["keen4-1.4-ega".to_string()].into());
        c
    }

    #[test]
    fn parses_a_valid_entry() {
        let e = parse(AURORA).unwrap();
        assert_eq!(e.released.as_deref(), Some("2023-08-23"));
        assert_eq!(
            e.recipe,
            Recipe::Patch {
                patcher: "CK4PATCH.EXE".into(),
                patch: "abiathar.pat".into(),
                args: vec![]
            }
        );
        assert_eq!(
            problems(&e, "aurora-bliss", &catalog()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn rejects_typos_and_unknown_recipes() {
        assert!(parse(&AURORA.replace("authors:", "author:")).is_err());
        assert!(parse(&AURORA.replace("type: patch", "type: magic")).is_err());
        assert!(
            parse(&AURORA.replace("patch: abiathar.pat", "patch: a.pat\n  pacher: x")).is_err()
        );
        // Unquoted versions keep their spelling.
        let e = parse(&AURORA.replace("\"1.2\"", "1.10")).unwrap();
        assert_eq!(e.version.as_deref(), Some("1.10"));
    }

    #[test]
    fn reports_problems() {
        let mut e = parse(AURORA).unwrap();
        e.game = "keen9".into();
        e.added = "17.10.2026".into();
        e.downloads[0].sha256 = "abc".into();
        e.links.keenwiki = Some("https://example.com/wiki/x".into());
        e.author_links
            .insert("Nobody".into(), "http://example.com".into());
        let p = problems(&e, "other-name", &catalog());
        assert_eq!(p.len(), 7, "{p:#?}");

        let mut e = parse(AURORA).unwrap();
        e.requires = vec!["keen4-1.0".into()];
        assert_eq!(problems(&e, "aurora-bliss", &catalog()).len(), 1);
    }

    #[test]
    fn translations_are_for_known_languages_and_texts() {
        let yaml = format!(
            "{AURORA}description: Blissful.\ntranslations:\n  de:\n    description: Glückselig.\n"
        );
        let e = parse(&yaml).unwrap();
        assert_eq!(
            e.translations["de"].description.as_deref(),
            Some("Glückselig.")
        );
        assert_eq!(
            problems(&e, "aurora-bliss", &catalog()),
            Vec::<String>::new()
        );
        assert!(parse(&format!("{AURORA}translations:\n  de:\n    name: X\n")).is_err());

        let mut bad = e.clone();
        bad.description = None;
        bad.translations.insert(
            "fr".into(),
            Translation {
                description: Some(" ".into()),
            },
        );
        let p = problems(&bad, "aurora-bliss", &catalog());
        assert_eq!(p.len(), 3, "{p:#?}");
    }

    #[test]
    fn version_recipes_and_copies() {
        let yaml = format!(
            "{AURORA}versionRecipes:\n  keen4-1.4-ega: {{type: patch, patcher: X.EXE, patch: Y.PT}}\n\
             copyToRoot: [\"1\"]\nminAppVersion: 0.2.0\n"
        );
        let e = parse(&yaml).unwrap();
        assert_eq!(e.recipes().count(), 2);
        assert_eq!(e.copy_to_root, ["1"]);
        assert_eq!(
            problems(&e, "aurora-bliss", &catalog()),
            Vec::<String>::new()
        );

        let mut bad = e.clone();
        bad.min_app_version = None;
        bad.copy_to_root = vec!["a/b".into()];
        bad.version_recipes
            .insert("keen4-1.0".into(), Recipe::Episode);
        let p = problems(&bad, "aurora-bliss", &catalog());
        assert_eq!(p.len(), 3, "{p:#?}");
    }

    #[test]
    fn provided_files_are_plain_and_of_one_entry_per_game() {
        let mut base = parse(AURORA).unwrap();
        base.provides = vec!["AURORA.BAT".into()];
        assert_eq!(
            problems(&base, "aurora-bliss", &catalog()),
            Vec::<String>::new()
        );
        let mut bad = base.clone();
        bad.provides = vec!["sub/aurora.bat".into()];
        assert_eq!(problems(&bad, "aurora-bliss", &catalog()).len(), 1);

        let mut other = base.clone();
        other.id = "aurora-2".into();
        other.provides = vec!["aurora.bat".into()];
        assert_eq!(provides_problems(&[base.clone(), other.clone()]).len(), 1);
        other.game = "keen5".into();
        assert_eq!(provides_problems(&[base, other]), Vec::<String>::new());
    }

    #[test]
    fn extends_another_entry_of_the_same_game() {
        let base = parse(AURORA).unwrap();
        let mut pack = base.clone();
        pack.id = "aurora-pack".into();
        pack.extends = Some("aurora-bliss".into());
        assert_eq!(problems(&pack, "aurora-pack", &catalog()).len(), 1);
        pack.min_app_version = Some("0.1.0".into());
        assert_eq!(
            problems(&pack, "aurora-pack", &catalog()),
            Vec::<String>::new()
        );
        assert_eq!(
            extends_problems(&[base.clone(), pack.clone()]),
            Vec::<String>::new()
        );

        assert_eq!(extends_problems(std::slice::from_ref(&pack)).len(), 1);
        let mut other_game = base.clone();
        other_game.game = "keen5".into();
        assert_eq!(extends_problems(&[other_game, pack.clone()]).len(), 1);
        let mut chained = base.clone();
        chained.extends = Some("x".into());
        assert_eq!(extends_problems(&[chained, pack]).len(), 2);
    }

    #[test]
    fn slugs_and_versions() {
        assert!(is_slug("the-alphamatic-2"));
        assert!(!is_slug("The-Alphamatic") && !is_slug("a--b") && !is_slug("-a"));
        assert!(is_version("0.1.0") && !is_version("1.2") && !is_version("1.x.0"));
    }
}

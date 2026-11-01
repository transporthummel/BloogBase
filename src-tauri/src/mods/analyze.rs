//! What a folder or archive holds: game, recipes, files to install, the
//! mod it belongs over, and what it would replace or miss.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;
use specta::Type;

use crate::catalog::{self, Game};
use crate::ckpatch;
use crate::detect::{detect, sha256_hex};
use crate::error::{Error, Result};
use crate::readme;
use crate::recipe::{self, Recipe, has_ext, patcher_game};
use crate::source::{Source, split_dir};

use super::metadata::guess_metadata;
use super::{BaseCandidate, BaseMod, ModInfo, RecipeOption};

/// The mod inside a source.
pub(super) struct Analysis {
    /// Directory of the mod inside the source, `""` for the root.
    pub(super) root: String,
    /// Files to install, relative to the source.
    pub(super) files: Vec<String>,
    /// More places for some of them: source path → path in the mod.
    pub(super) copies: Vec<(String, String)>,
    pub(super) info: ModInfo,
}

/// `game_hint` is used when neither the files nor the archive name tell.
pub fn inspect(path: &Path, game_hint: Option<&str>) -> Result<ModInfo> {
    Ok(analyze(&Source::open(path)?, path, game_hint)?.info)
}

/// The mod a way to start a mod lacks `missing_files` of, among `candidates`:
/// an index entry that says it provides them all, with the installed mod
/// tied to it (see [`crate::mod_index::link_installed`]), else an installed
/// one that has them, which may just bring the files along.
pub fn needed_mod(missing_files: &[String], candidates: &[BaseCandidate]) -> Option<BaseMod> {
    if missing_files.is_empty() {
        return None;
    }
    let has_all = |names: &[String]| {
        missing_files
            .iter()
            .all(|m| names.iter().any(|n| n.eq_ignore_ascii_case(m)))
    };
    let installed = |dir: &Path| -> Vec<String> {
        fs::read_dir(dir)
            .map(|d| {
                d.filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned()))
                    .collect()
            })
            .unwrap_or_default()
    };
    candidates
        .iter()
        .find(|c| has_all(&c.provides))
        .or_else(|| {
            candidates
                .iter()
                .find(|c| c.files.as_deref().is_some_and(|d| has_all(&installed(d))))
        })
        .map(BaseCandidate::base)
}

/// The mod levels alone are for when nothing tells by its files: the one of
/// `candidates` named as `made_for` says, punctuation and case aside
/// (Community Smile Levelpack's `episode_smile` for Episode Smile).
pub fn named_mod(made_for: &str, candidates: &[BaseCandidate]) -> Option<BaseMod> {
    let key = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let wanted = key(made_for);
    candidates
        .iter()
        .find(|c| !wanted.is_empty() && key(&c.name) == wanted)
        .map(BaseCandidate::base)
}

impl BaseCandidate {
    fn base(&self) -> BaseMod {
        BaseMod {
            name: self.name.clone(),
            mod_id: self.mod_id.clone(),
            index_id: self.index_id.clone(),
        }
    }
}

/// What a folder or ZIP dropped on the window holds.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Dropped {
    Games,
    Mod { info: Box<ModInfo> },
    Nothing,
}

/// Tells games from a mod. Game detection goes by file names, and many mods
/// ship the game's files, often with their own executable. So whatever starts
/// through a patch or a batch file is a mod, even where a game is detected.
pub fn classify(path: &Path) -> Result<Dropped> {
    let source = Source::open(path)?;
    let game = !detect(&source)?.is_empty();
    let analysis = match analyze(&source, path, None) {
        Ok(a) => Some(a.info),
        Err(Error::NoModFound) => None,
        Err(e) => return Err(e),
    };
    Ok(decide(game, analysis))
}

pub(super) fn decide(game: bool, info: Option<ModInfo>) -> Dropped {
    let started_by_mod = info.as_ref().is_some_and(|i| {
        i.recipes
            .first()
            .is_some_and(|o| matches!(o.recipe, Recipe::Patch { .. } | Recipe::Batch { .. }))
    });
    match info {
        _ if game && !started_by_mod => Dropped::Games,
        Some(info) => Dropped::Mod {
            info: Box::new(info),
        },
        None => Dropped::Nothing,
    }
}

pub(super) fn analyze(source: &Source, path: &Path, game_hint: Option<&str>) -> Result<Analysis> {
    let files: Vec<String> = source
        .files()?
        .into_iter()
        .filter(|f| !is_junk(split_dir(f).1))
        .collect();
    // Patchers by name, or by the usage line of renamed ones.
    let patchers: BTreeMap<&str, &'static str> = files
        .iter()
        .filter_map(|f| {
            let name = split_dir(f).1;
            if !(has_ext(name, "EXE") || has_ext(name, "COM")) {
                return None;
            }
            let game = patcher_game(name).or_else(|| {
                let exe = source.read(f).ok()?;
                recipe::patcher_game_of(&exe)
            })?;
            Some((f.as_str(), game))
        })
        .collect();
    let root = find_root(&files, |f| patchers.contains_key(f)).ok_or(Error::NoModFound)?;
    let in_root = |f: &&String| root.is_empty() || f.starts_with(&format!("{root}/"));
    let mut files: Vec<String> = files.iter().filter(in_root).cloned().collect();
    // Bare names in the mod root, where recipes have to live.
    let names: Vec<String> = files
        .iter()
        .filter(|f| split_dir(f).0 == root)
        .map(|f| split_dir(f).1.to_string())
        .collect();
    let rel = |name: &str| {
        if root.is_empty() {
            name.to_string()
        } else {
            format!("{root}/{name}")
        }
    };
    let read = |name: &str| {
        source
            .read(&rel(name))
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    };

    let meta = guess_metadata(path, &root);
    let is_patcher = |name: &str| patchers.contains_key(rel(name).as_str());
    let by_patcher = names
        .iter()
        .find_map(|n| patchers.get(rel(n).as_str()).copied());
    let game = find_game(&names, &read, by_patcher, meta.episode.or(game_hint))
        .ok_or(Error::NoModFound)?;

    // Pristine game executables would hide the install's own version.
    let mut pristine = Vec::new();
    for name in &names {
        if game
            .executables
            .iter()
            .any(|e| e.eq_ignore_ascii_case(name))
        {
            let hash = sha256_hex(&source.read(&rel(name))?);
            if game
                .versions
                .iter()
                .any(|v| v.files.values().any(|h| *h == hash))
            {
                pristine.push(rel(name));
            }
        }
    }
    files.retain(|f| !pristine.contains(f));
    // Keen 1-3's settings: a mod's are its author's keys and sound, which the
    // game's own menus change, so the game starts with its defaults instead.
    files.retain(|f| {
        let (dir, name) = split_dir(f);
        let settings = name
            .rsplit_once('.')
            .is_some_and(|(stem, _)| stem.eq_ignore_ascii_case("CTLPANEL"));
        !(dir == root && settings && episode_of_ext(name) == Some(game.id.as_str()))
    });

    // Without a patcher of its own, a patch file runs with BloogBase's.
    let bundled = ckpatch::needed(
        &game.id,
        names.iter().map(|n| (n.as_str(), read(n))),
        is_patcher,
    )
    .then(|| ckpatch::bundled(&game.id))
    .flatten()
    .map(|(name, _)| name.to_string());
    let with_bundled: Vec<String> = names.iter().cloned().chain(bundled.clone()).collect();
    let is_patcher = |name: &str| {
        is_patcher(name)
            || bundled
                .as_deref()
                .is_some_and(|b| b.eq_ignore_ascii_case(name))
    };
    let guess = recipe::guess(
        &with_bundled,
        read,
        &game.executables,
        is_patcher,
        &meta.name,
    );
    let mut recipes: Vec<RecipeOption> = guess
        .candidates
        .into_iter()
        .map(|c| {
            let game_versions = c
                .recipe
                .patch_file()
                .and_then(read)
                .map(|t| recipe::patch_versions(&t))
                .unwrap_or_default();
            RecipeOption {
                version_ids: version_ids(game, &game_versions),
                game_versions,
                missing_files: missing_files(
                    &c.recipe,
                    &files,
                    &root,
                    &with_bundled,
                    read,
                    is_patcher,
                ),
                base_mod: None,
                made_for: None,
                recipe: c.recipe,
                origin: c.origin,
            }
        })
        .collect();
    // Levels alone have nothing that loads the files they lack, but their
    // readme may say how the mod they go over starts: xky1rooms' "Then run
    // XkyKeen1.bat".
    if let [only] = recipes.as_mut_slice()
        && only.recipe == Recipe::Episode
        && only.origin.is_empty()
    {
        let docs = readme::find(source).unwrap_or_default();
        only.missing_files = named_starts(docs.iter().map(|d| d.text.as_str()), &files, |name| {
            recipe::is_patcher(name)
                || game
                    .executables
                    .iter()
                    .any(|e| e.eq_ignore_ascii_case(name))
        });
    }

    Ok(Analysis {
        info: ModInfo {
            name: meta.name,
            authors: meta.authors,
            version: meta.version,
            released: meta.released,
            game_id: game.id.clone(),
            recipes,
            confident: guess.confident,
            file_count: files.len() as u32,
            index_id: None,
            made_for: meta.made_for,
        },
        root,
        files,
        copies: Vec::new(),
    })
}

/// Files the patch file `start` starts the mod with loads (`%patchfile`)
/// that the mod doesn't have, anywhere in its root (`files`, relative to the
/// source): a patch recipe's, or what its batch file passes to the patcher.
/// Only that one counts: others lying around may be leftovers (The
/// Electromagnet's `tileinfo.pat`).
fn missing_files(
    start: &Recipe,
    files: &[String],
    root: &str,
    names: &[String],
    read: impl Fn(&str) -> Option<String>,
    is_patcher: impl Fn(&str) -> bool,
) -> Vec<String> {
    let in_mod: Vec<String> = files
        .iter()
        .map(|f| {
            let inner = if root.is_empty() {
                f.as_str()
            } else {
                &f[root.len() + 1..]
            };
            inner.to_ascii_lowercase()
        })
        .collect();
    // A batch file's `ck1patch lego.pat` names the patcher without `.EXE`.
    let runs_patcher = |cmd: &str| {
        names.iter().any(|n| {
            let stem = n.rsplit_once('.').map_or(n.as_str(), |(s, _)| s);
            (n.eq_ignore_ascii_case(cmd) || stem.eq_ignore_ascii_case(cmd)) && is_patcher(n)
        })
    };
    let patches: Vec<String> = match start {
        Recipe::Patch { patch, .. } => vec![patch.clone()],
        Recipe::Batch { file } => read(file)
            .map(|t| recipe::batch_commands(&t))
            .unwrap_or_default()
            .into_iter()
            .filter(|cmd| cmd.len() > 1 && runs_patcher(&cmd[0]))
            .map(|cmd| cmd[1].clone())
            .collect(),
        Recipe::Program { .. } | Recipe::Episode => Vec::new(),
    };
    let mut missing: Vec<String> = Vec::new();
    for patch in &patches {
        for file in read(patch)
            .map(|t| recipe::patch_files(&t))
            .unwrap_or_default()
        {
            let path = file.replace('\\', "/").to_ascii_lowercase();
            if !in_mod.contains(&path) && !missing.iter().any(|m| m.eq_ignore_ascii_case(&file)) {
                missing.push(file);
            }
        }
    }
    missing
}

/// Batch files and programs the documents `docs` name that the mod (`files`)
/// lacks, other than the `common` ones every copy of the game brings.
fn named_starts<'a>(
    docs: impl Iterator<Item = &'a str>,
    files: &[String],
    common: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    for word in
        docs.flat_map(|t| t.split(|c: char| !(c.is_ascii_alphanumeric() || "_-.".contains(c))))
    {
        let word = word.trim_end_matches('.');
        // DOS names; `.COM` would also catch web addresses.
        let start = word
            .rsplit_once('.')
            .is_some_and(|(stem, _)| (1..=8).contains(&stem.len()) && !stem.contains('.'))
            && (has_ext(word, "BAT") || has_ext(word, "EXE"));
        if start
            && !common(word)
            && !files
                .iter()
                .any(|f| split_dir(f).1.eq_ignore_ascii_case(word))
            && !named.iter().any(|n| n.eq_ignore_ascii_case(word))
        {
            named.push(word.to_string());
        }
    }
    named
}

fn is_junk(name: &str) -> bool {
    name.starts_with("._")
        || ["THUMBS.DB", ".DS_STORE", "DESKTOP.INI"]
            .iter()
            .any(|j| j.eq_ignore_ascii_case(name))
}

/// Whether a file name says "Commander Keen data": episode data, patches,
/// patchers, episode executables.
fn is_keen_file(name: &str) -> bool {
    episode_of_ext(name).is_some()
        || has_ext(name, "PAT")
        || patcher_game(name).is_some()
        || catalog::games()
            .iter()
            .any(|g| g.executables.iter().any(|e| e.eq_ignore_ascii_case(name)))
}

fn episode_of_ext(name: &str) -> Option<&'static str> {
    let (_, ext) = name.rsplit_once('.')?;
    Some(match ext.to_ascii_uppercase().as_str() {
        "CK1" => "keen1",
        "CK2" => "keen2",
        "CK3" => "keen3",
        "CK4" => "keen4",
        "CK5" => "keen5",
        "CK6" => "keen6",
        "KDR" => "kdreams",
        _ => return None,
    })
}

/// The directory with the most Keen files directly inside it; the shallowest
/// one on a tie. Mods are often zipped with a folder around them. Folders the
/// mod can start from (a patcher, a patch or a game executable) come first:
/// Keen Rescue keeps more levels in `levels/` than files next to its patcher.
/// `is_patcher` takes a path and also knows renamed patchers.
pub(super) fn find_root(files: &[String], is_patcher: impl Fn(&str) -> bool) -> Option<String> {
    let mut score: BTreeMap<&str, (bool, usize)> = BTreeMap::new();
    for f in files {
        let (dir, name) = split_dir(f);
        if is_keen_file(name) || is_patcher(f) {
            let s = score.entry(dir).or_default();
            s.0 |= episode_of_ext(name).is_none();
            s.1 += 1;
        }
    }
    score
        .into_iter()
        .max_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| depth(b.0).cmp(&depth(a.0)))
                .then_with(|| b.0.cmp(a.0))
        })
        .map(|(dir, _)| dir.to_string())
}

fn depth(dir: &str) -> usize {
    if dir.is_empty() {
        0
    } else {
        dir.matches('/').count() + 1
    }
}

/// The target episode: the patcher says it best, then a patch file's `%ext`,
/// then the data files, then the archive name.
fn find_game(
    names: &[String],
    read: &impl Fn(&str) -> Option<String>,
    by_patcher: Option<&'static str>,
    hint: Option<&str>,
) -> Option<&'static Game> {
    let by_ext = || {
        names
            .iter()
            .filter_map(|n| {
                let text = read(n)?;
                (has_ext(n, "PAT") || recipe::is_patch_text(n, &text)).then_some(text)
            })
            .filter_map(|t| recipe::patch_ext(&t))
            .find_map(|ext| episode_of_ext(&format!("x.{ext}")))
    };
    let by_data = || {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for n in names {
            if let Some(g) = episode_of_ext(n) {
                *counts.entry(g).or_default() += 1;
            }
        }
        counts.into_iter().max_by_key(|(_, c)| *c).map(|(g, _)| g)
    };
    let by_exe = || {
        catalog::games()
            .iter()
            .find(|g| {
                g.executables
                    .iter()
                    .any(|e| names.iter().any(|n| n.eq_ignore_ascii_case(e)))
            })
            .map(|g| g.id.as_str())
    };
    let id = by_patcher
        .or_else(by_ext)
        .or_else(by_data)
        .or_else(by_exe)
        .or(hint)?;
    catalog::game(id)
}

/// Catalog versions of `game` matching patch file versions like `1.4`,
/// plus third-party builds of those.
pub(super) fn version_ids(game: &Game, versions: &[String]) -> Vec<String> {
    let originals: Vec<&str> = game
        .versions
        .iter()
        .filter(|v| {
            v.patch_version
                .as_ref()
                .is_some_and(|p| versions.contains(p))
        })
        .map(|v| v.id.as_str())
        .collect();
    game.versions
        .iter()
        .filter(|v| originals.contains(&v.original.as_deref().unwrap_or(&v.id)))
        .map(|v| v.id.clone())
        .collect()
}

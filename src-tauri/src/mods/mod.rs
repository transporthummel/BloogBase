//! Mods: analysis, install pipeline, removal (PLAN.md §9.2, §9.4, §9.5).
//!
//! Installing is two steps so the user can confirm what was detected:
//! [`inspect`] looks at a folder or ZIP, [`install`] stores the mod and creates
//! its play profile.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::recipe::Recipe;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ModSource {
    Index,
    Keenwiki,
    Manual,
}

impl ModSource {
    fn as_str(self) -> &'static str {
        match self {
            ModSource::Index => "index",
            ModSource::Keenwiki => "keenwiki",
            ModSource::Manual => "manual",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "index" => ModSource::Index,
            "keenwiki" => ModSource::Keenwiki,
            "manual" => ModSource::Manual,
            _ => return None,
        })
    }
}

/// What [`inspect`] found, for the user to confirm.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    pub name: String,
    pub authors: Vec<String>,
    pub version: Option<String>,
    /// `YYYY-MM-DD`, when known.
    pub released: Option<String>,
    pub game_id: String,
    /// Ways to start the mod, best first.
    pub recipes: Vec<RecipeOption>,
    /// False when the user should pick from `recipes`.
    pub confident: bool,
    pub file_count: u32,
    /// The index entry whose download this is, byte for byte.
    pub index_id: Option<String>,
    /// The mod a level pack's archive name says its levels are for
    /// (`l.keen_community.episode_smile`), for [`named_mod`].
    #[serde(skip)]
    pub made_for: Option<String>,
}

/// A mod another one is made to be copied over, installed or in the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BaseMod {
    pub name: String,
    /// The installed mod, if it is.
    pub mod_id: Option<String>,
    /// Its index entry, to install it from.
    pub index_id: Option<String>,
}

/// A mod [`needed_mod`] may pick.
#[derive(Debug, Clone)]
pub struct BaseCandidate {
    pub name: String,
    pub mod_id: Option<String>,
    pub index_id: Option<String>,
    /// The installed mod's files.
    pub files: Option<PathBuf>,
    /// Files its index entry says mods copied over it lack.
    pub provides: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeOption {
    pub recipe: Recipe,
    /// The file that suggested this recipe, e.g. `PLAY.BAT`.
    pub origin: String,
    /// Versions the patch file names, e.g. `1.4`.
    pub game_versions: Vec<String>,
    /// Catalog versions matching `game_versions`. Empty when any version
    /// may do, or the catalog doesn't know the named versions.
    pub version_ids: Vec<String>,
    /// Files its patch file loads that the mod doesn't have, as a mod made to
    /// be copied over another one (Lego Levels over Lego Keen); for levels
    /// alone, the start its readme names (xky1rooms' `XkyKeen1.bat`).
    pub missing_files: Vec<String>,
    /// The mod it's made to be copied over, when files are missing and
    /// BloogBase knows one that fits.
    pub base_mod: Option<BaseMod>,
    /// For levels alone, the mod their archive name says they're for, known
    /// to BloogBase or not ([`ModInfo::made_for`]).
    pub made_for: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InstallModRequest {
    /// The folder or ZIP passed to [`inspect`].
    pub path: String,
    pub name: String,
    /// One of the recipes [`inspect`] offered.
    pub recipe: Recipe,
    /// The install the mod is played on; `None` to install it without its
    /// game, which it then waits for.
    pub install_id: Option<String>,
    /// Set when the mod came from a KeenWiki page.
    #[serde(default)]
    pub keenwiki: Option<KeenwikiOrigin>,
    /// An installed mod of the same game this one is played over, as it's
    /// made to be copied over that one.
    #[serde(default)]
    pub extends: Option<String>,
    /// An installed mod of the same game this one replaces, e.g. an older
    /// version. Its id and saves stay; `None` installs alongside.
    #[serde(default)]
    pub replace: Option<String>,
}

/// Where a KeenWiki mod came from, and what its page says.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeenwikiOrigin {
    pub page_url: String,
    pub download_url: String,
    /// The page's "Modded episode", for archives that don't tell.
    pub game_id: Option<String>,
    /// Replace the guessed ones when not empty.
    pub authors: Vec<String>,
    /// Author name → their page.
    #[serde(default)]
    pub author_links: BTreeMap<String, String>,
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// `YYYY-MM-DD`, when known.
    #[serde(default)]
    pub released: Option<String>,
    /// The mod's own website.
    #[serde(default)]
    pub homepage: Option<String>,
    /// The discussion thread on PCKF.
    #[serde(default)]
    pub pckf: Option<String>,
    /// The discussion thread on the Keen: Modding forum.
    #[serde(default)]
    pub keenmodding: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Mod {
    pub id: String,
    pub game_id: String,
    pub name: String,
    pub authors: Vec<String>,
    /// Author name → their page, for the authors that have one.
    pub author_links: BTreeMap<String, String>,
    pub version: Option<String>,
    pub description: Option<String>,
    /// `YYYY-MM-DD`, when known.
    pub released: Option<String>,
    pub recipe: Recipe,
    pub source: ModSource,
    /// The index entry it was installed from.
    pub index_id: Option<String>,
    /// The installed mod it's played over.
    pub extends: Option<String>,
    pub keenwiki_url: Option<String>,
    /// The mod's own website, from its KeenWiki page.
    pub homepage: Option<String>,
    /// Its discussion thread on PCKF, from its KeenWiki page.
    pub pckf: Option<String>,
    /// Its thread on the Keen: Modding forum, from its KeenWiki page.
    pub keenmodding: Option<String>,
    /// Unix epoch seconds.
    #[specta(type = u32)]
    pub added_at: i64,
    /// The play profile created for the mod.
    pub profile_id: String,
}

mod analyze;
mod fit;
mod install;
mod metadata;
mod queries;

pub use analyze::*;
pub use fit::*;
pub use install::*;
pub use queries::*;

#[cfg(test)]
mod tests;

/// Checks the heuristics against real mods listed in
/// `tests/mod-corpus.json`: download URL, SHA-256 and the expected result.
/// The archives are never committed; missing ones are downloaded once into
/// `BLOOGBASE_MOD_CACHE` (default `.local/mods`).
///
/// `cargo test corpus -- --ignored --nocapture`
/// `BLOOGBASE_CORPUS_UPDATE=1 cargo test corpus -- --ignored` rewrites the
/// expectations from the current results (review the diff!).
#[cfg(test)]
mod corpus;

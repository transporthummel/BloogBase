//! Listing, details, origin and removal of installed mods.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use specta::Type;

use crate::backups::{self, Contents, Reason};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::keenwiki::WikiMod;
use crate::paths::AppPaths;
use crate::recipe::{self, Recipe, VersionRecipes};

use super::install::slug;
use super::{Mod, ModSource};

pub fn list(db: &Db) -> Result<Vec<Mod>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT m.id, m.game_id, m.name, m.authors, m.version, m.recipe_json, m.source,
                m.added_at, p.id, m.index_id, m.keenwiki_url, m.description, m.released,
                m.author_links, m.version_recipes_json, i.version_id, m.extends, m.homepage,
                m.pckf, m.keenmodding
         FROM mods m JOIN profiles p ON p.mod_id = m.id
         LEFT JOIN installs i ON i.id = p.install_id
         ORDER BY m.added_at, m.id",
    )?;
    let rows = stmt.query_map([], |r| {
        let authors: String = r.get(3)?;
        let recipe: String = r.get(5)?;
        let source: String = r.get(6)?;
        let game_id: String = r.get(1)?;
        let recipe = serde_json::from_str(&recipe).unwrap_or(Recipe::Episode);
        let by_version: VersionRecipes = r
            .get::<_, Option<String>>(14)?
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        let version_id: Option<String> = r.get(15)?;
        // The one it starts with on its game.
        let recipe =
            recipe::for_version(&recipe, &by_version, &game_id, version_id.as_deref()).clone();
        Ok(Mod {
            id: r.get(0)?,
            game_id,
            name: r.get(2)?,
            authors: serde_json::from_str(&authors).unwrap_or_default(),
            author_links: r
                .get::<_, Option<String>>(13)?
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default(),
            version: r.get(4)?,
            recipe,
            source: ModSource::parse(&source).expect("valid mod source"),
            added_at: r.get(7)?,
            profile_id: r.get(8)?,
            index_id: r.get(9)?,
            extends: r.get(16)?,
            keenwiki_url: r.get(10)?,
            homepage: r.get(17)?,
            pckf: r.get(18)?,
            keenmodding: r.get(19)?,
            description: r.get(11)?,
            released: r.get(12)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// What a mod's KeenWiki page says about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PageDetail {
    Authors,
    Description,
    Released,
    Homepage,
    Pckf,
    Keenmodding,
}

/// Takes over what a mod's KeenWiki page now says about it; what the page
/// leaves out stays. Mods from the index keep the index's details. Returns
/// what changed.
pub fn refresh_details(db: &Db, mod_id: &str, page: &WikiMod) -> Result<Vec<PageDetail>> {
    type Row = (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let conn = db.conn();
    let (source, authors, author_links, description, released, homepage, pckf, keenmodding): Row =
        conn.query_row(
            "SELECT source, authors, author_links, description, released, homepage, pckf,
                    keenmodding
             FROM mods WHERE id = ?1",
            [mod_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))?;
    if source == ModSource::Index.as_str() {
        return Ok(Vec::new());
    }
    let mut authors: Vec<String> = serde_json::from_str(&authors).unwrap_or_default();
    let mut author_links: BTreeMap<String, String> = author_links
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();
    let mut changed = Vec::new();
    if !page.authors.is_empty() && (page.authors != authors || page.author_links != author_links) {
        authors = page.authors.clone();
        author_links = page.author_links.clone();
        changed.push(PageDetail::Authors);
    }
    let mut take = |field: &mut Option<String>, new: &Option<String>, detail| {
        if new.is_some() && field != new {
            *field = new.clone();
            changed.push(detail);
        }
    };
    let (mut description, mut released, mut homepage, mut pckf, mut keenmodding) =
        (description, released, homepage, pckf, keenmodding);
    take(&mut description, &page.description, PageDetail::Description);
    take(&mut released, &page.released, PageDetail::Released);
    take(&mut homepage, &page.homepage, PageDetail::Homepage);
    take(&mut pckf, &page.pckf, PageDetail::Pckf);
    take(&mut keenmodding, &page.keenmodding, PageDetail::Keenmodding);
    if !changed.is_empty() {
        conn.execute(
            "UPDATE mods SET authors = ?2, author_links = ?3, description = ?4, released = ?5,
                             homepage = ?6, pckf = ?7, keenmodding = ?8
             WHERE id = ?1",
            params![
                mod_id,
                serde_json::to_string(&authors).expect("strings serialize"),
                serde_json::to_string(&author_links).expect("strings serialize"),
                description,
                released,
                homepage,
                pckf,
                keenmodding
            ],
        )?;
        tracing::info!("took {changed:?} of mod {mod_id} from its KeenWiki page");
    }
    Ok(changed)
}

/// Where an installed mod came from: its KeenWiki page, the download (or
/// picked file) and its version.
pub fn origin(db: &Db, mod_id: &str) -> Result<(Option<String>, String, Option<String>)> {
    db.conn()
        .query_row(
            "SELECT keenwiki_url, source_url, version FROM mods WHERE id = ?1",
            [mod_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))
}

/// A link to a prefilled GitHub issue suggesting a KeenWiki mod for the
/// index (§9.3), with a draft entry built from what was installed.
pub fn index_suggestion(db: &Db, mod_id: &str) -> Result<String> {
    let m = list(db)?
        .into_iter()
        .find(|m| m.id == mod_id)
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))?;
    let (source_url, sha256, path): (Option<String>, Option<String>, String) =
        db.conn().query_row(
            "SELECT source_url, archive_sha256, path FROM mods WHERE id = ?1",
            [mod_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
    let Mod {
        name,
        game_id,
        authors,
        author_links,
        version,
        recipe,
        keenwiki_url,
        homepage,
        pckf,
        keenmodding,
        ..
    } = m;
    let size = fs::read_dir(Path::new(&path).join("source"))
        .ok()
        .and_then(|mut d| d.next())
        .and_then(|e| e.ok()?.metadata().ok())
        .map(|m| m.len());

    // JSON scalars and arrays are valid YAML.
    fn json(v: &impl Serialize) -> String {
        serde_json::to_string(v).expect("plain values serialize")
    }
    let recipe = serde_json::to_value(&recipe).expect("recipes serialize");
    let mut yaml = format!(
        "id: {}\nname: {}\ngame: {game_id}\nrequires: []\nauthors: {}\n",
        slug(&name),
        json(&name),
        json(&authors)
    );
    if !author_links.is_empty() {
        yaml += "authorLinks:\n";
        for (name, url) in &author_links {
            yaml += &format!("  {}: {url}\n", json(name));
        }
    }
    if let Some(v) = &version {
        yaml += &format!("version: {}\n", json(v));
    }
    yaml += "type: episode  # episode | levelpack | conversion\nlinks:\n";
    if let Some(url) = &keenwiki_url {
        yaml += &format!("  keenwiki: {url}\n");
    }
    if let Some(url) = &pckf {
        yaml += &format!("  pckf: {url}\n");
    }
    if let Some(url) = &keenmodding {
        yaml += &format!("  keenmodding: {url}\n");
    }
    if let Some(url) = &homepage {
        yaml += &format!("  homepage: {url}\n");
    }
    yaml += "downloads:\n";
    yaml += &format!(
        "  - url: {}\n    sha256: {}\n",
        source_url.as_deref().unwrap_or(""),
        sha256.as_deref().unwrap_or("")
    );
    if let Some(v) = &version {
        yaml += &format!("    version: {}\n", json(v));
    }
    if let Some(size) = size {
        yaml += &format!("    size: {size}\n");
    }
    yaml += "recipe:\n";
    if let serde_json::Value::Object(fields) = &recipe {
        for (key, value) in fields {
            yaml += &format!("  {key}: {value}\n");
        }
    }

    let body = format!(
        "BloogBase installed this mod from KeenWiki and it could go into the index.\n\n\
         Page: {}\n\nDraft entry (please check the recipe, `requires`, `type` and the dates):\n\n\
         ```yaml\n{yaml}```\n",
        keenwiki_url.as_deref().unwrap_or("-"),
    );
    let url = reqwest::Url::parse_with_params(
        concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new"),
        [
            ("title", format!("Index suggestion: {name}")),
            ("body", body),
        ],
    )
    .expect("valid URL");
    Ok(url.to_string())
}

/// The mods played over `mod_id`.
pub fn extensions(db: &Db, mod_id: &str) -> Result<Vec<String>> {
    let conn = db.conn();
    let mut stmt = conn.prepare("SELECT id FROM mods WHERE extends = ?1 ORDER BY added_at, id")?;
    let ids = stmt
        .query_map([mod_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

/// Removes a mod with its profile, and the mods played over it. With
/// `keep_saves`, what the game wrote is archived into `backups/` first (§9.5).
pub fn delete(paths: &AppPaths, db: &Db, mod_id: &str, keep_saves: bool) -> Result<()> {
    for extension in extensions(db, mod_id)? {
        delete(paths, db, &extension, keep_saves)?;
    }
    let profile_id: String = db
        .conn()
        .query_row("SELECT id FROM profiles WHERE mod_id = ?1", [mod_id], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("mod {mod_id}")))?;
    let backup = if keep_saves {
        backups::create(paths, db, &profile_id, Contents::State, Reason::ModRemoved)?
    } else {
        None
    };
    backups::forget(db, &profile_id, keep_saves)?;
    {
        let mut conn = db.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM profiles WHERE id = ?1", [&profile_id])?;
        tx.execute("DELETE FROM mods WHERE id = ?1", [mod_id])?;
        tx.commit()?;
    }
    for dir in [
        paths.mods.join(mod_id),
        paths.profiles.join(&profile_id),
        paths.composed(&profile_id),
    ] {
        if let Err(e) = fs::remove_dir_all(&dir)
            && e.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!("could not remove {}: {e}", dir.display());
        }
    }
    crate::covers::remove(paths, &profile_id);
    tracing::info!("removed mod {mod_id} (saves kept: {})", backup.is_some());
    Ok(())
}

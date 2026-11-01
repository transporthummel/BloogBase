//! Name, authors, version and date guessed from file and folder names.

use std::path::Path;

use crate::source::split_dir;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Metadata {
    pub(super) name: String,
    pub(super) authors: Vec<String>,
    pub(super) version: Option<String>,
    pub(super) released: Option<String>,
    pub(super) episode: Option<&'static str>,
    /// The mod a level pack is for, as its name says.
    pub(super) made_for: Option<String>,
}

/// Name and details from the file name. Understands the archive naming
/// scheme `m.<author>.<name>[_v<version>].<episode>.<date>`
/// (e.g. `m.dave216.aurora_bliss_v.1.2.k4.2023-08-23.zip`); otherwise uses
/// the mod's folder or the file name.
pub(super) fn guess_metadata(path: &Path, root: &str) -> Metadata {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = if path.is_dir() {
        file_name.as_str()
    } else {
        file_name
            .rsplit_once('.')
            .map_or(file_name.as_str(), |(s, _)| s)
    };

    let parts: Vec<&str> = stem.split('.').collect();
    // Level packs without a name: `l.<author>.<episode>.<date>`.
    if parts.len() == 4
        && parts[0].eq_ignore_ascii_case("l")
        && is_date(parts[3])
        && let Some(episode) = episode_code(parts[2])
    {
        return Metadata {
            name: format!("{} Level Pack", title_case(&parts[1].replace('_', " "))),
            authors: vec![parts[1].to_string()],
            version: None,
            released: Some(parts[3].to_string()),
            episode: Some(episode),
            made_for: None,
        };
    }
    if parts.len() >= 5
        && parts[0].len() == 1
        && is_date(parts[parts.len() - 1])
        && let Some(episode) = episode_code(parts[parts.len() - 2])
    {
        let raw = parts[2..parts.len() - 2].join(".");
        let (name, version) = split_version(&raw);
        return Metadata {
            name: title_case(&name.replace('_', " ")),
            authors: vec![parts[1].to_string()],
            version,
            released: Some(parts[parts.len() - 1].to_string()),
            episode: Some(episode),
            made_for: None,
        };
    }
    // Without the episode, as KeenWiki's packs for mods: a level pack's
    // name is the mod its levels are for (`l.keen_community.lego_keen`).
    if parts.len() >= 4 && parts[0].len() == 1 && is_date(parts[parts.len() - 1]) {
        let raw = parts[2..parts.len() - 1].join(".");
        let (name, version) = split_version(&raw);
        let name = title_case(&name.replace('_', " "));
        let levels = parts[0].eq_ignore_ascii_case("l");
        return Metadata {
            name: if levels {
                format!("{name} Level Pack")
            } else {
                name.clone()
            },
            authors: vec![parts[1].to_string()],
            version,
            released: Some(parts[parts.len() - 1].to_string()),
            episode: None,
            made_for: levels.then_some(name),
        };
    }

    let folder = split_dir(root).1;
    let name = if folder.is_empty() { stem } else { folder };
    let name = name.replace('_', " ").trim().to_string();
    let name = if name.chars().any(|c| c.is_ascii_uppercase()) {
        name
    } else {
        title_case(&name)
    };
    Metadata {
        name: if name.is_empty() { "Mod".into() } else { name },
        authors: Vec::new(),
        version: None,
        released: None,
        episode: None,
        made_for: None,
    }
}

fn episode_code(code: &str) -> Option<&'static str> {
    Some(match code.to_ascii_lowercase().as_str() {
        "k1" => "keen1",
        "k2" => "keen2",
        "k3" => "keen3",
        "k4" => "keen4",
        "k5" => "keen5",
        "k6" => "keen6",
        "kd" => "kdreams",
        _ => return None,
    })
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

/// `aurora_bliss_v.1.2` → (`aurora_bliss`, `1.2`); also `simulator_keen.v.1.1`.
fn split_version(raw: &str) -> (String, Option<String>) {
    for sep in ["_v", ".v"] {
        if let Some((name, v)) = raw.rsplit_once(sep) {
            let v = v.trim_start_matches('.');
            if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.') {
                return (name.to_string(), Some(v.to_string()));
            }
        }
    }
    (raw.to_string(), None)
}

pub(super) fn title_case(s: &str) -> String {
    const SMALL: &[&str] = &["a", "an", "and", "in", "of", "on", "the", "to"];
    s.split_whitespace()
        .enumerate()
        .map(|(i, w)| {
            if i > 0 && SMALL.contains(&w) {
                return w.to_string();
            }
            if is_roman_numeral(w) {
                return w.to_ascii_uppercase();
            }
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether `w` is a sequel number from I to XXXIX, as in "Patchwork Mod III".
fn is_roman_numeral(w: &str) -> bool {
    let w = w.to_ascii_uppercase();
    let rest = w.trim_start_matches('X');
    if w.len() - rest.len() > 3 {
        return false;
    }
    !w.is_empty() && ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX"].contains(&rest)
}

//! Mod series, as written in `index/series/<id>.yaml` and published in
//! `index.json` next to the mods, and the checks they have to pass.
//!
//! A series lists its parts in order: numbered episodes, then unnumbered
//! bonus parts (level packs and extras). A part is a mod of the index or a
//! game of the catalog, so the original episodes form series as well.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::entry::{Catalog, Entry, LANGUAGES, is_slug};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Series {
    /// The file name; not written in the file.
    #[serde(skip_deserializing)]
    pub id: String,
    pub name: String,
    /// In English, a sentence or two.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Language code (one of [`LANGUAGES`]) → the texts in that language.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub translations: BTreeMap<String, SeriesTranslation>,
    /// The main parts, numbered in this order.
    pub episodes: Vec<SeriesPart>,
    /// Level packs and extras, without a number.
    #[serde(default)]
    pub bonus: Vec<SeriesPart>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesTranslation {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesPart {
    /// Id of an index entry.
    #[serde(rename = "mod", default, skip_serializing_if = "Option::is_none")]
    pub mod_id: Option<String>,
    /// Id of a catalog game, e.g. `keen4`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    /// The episode's own title, where the mod's name starts with the
    /// series'; left out, the app shows the mod's or game's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Overrides the running number, e.g. `"0"` or `"3.5"`. [`publish`]
    /// writes it for every episode, so the app doesn't count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
}

pub fn parse(yaml: &str, id: &str) -> Result<Series, String> {
    let mut s: Series = serde_yaml_ng::from_str(yaml).map_err(|e| e.to_string())?;
    s.id = id.to_string();
    Ok(s)
}

/// The episodes' numbers: a written `number`, else one more than the whole
/// part of the episode before (starting at 1).
pub fn numbers(series: &Series) -> Vec<String> {
    let mut last: Option<u64> = None;
    series
        .episodes
        .iter()
        .map(|p| {
            let n = match &p.number {
                Some(n) => n.clone(),
                None => last.map_or(1, |l| l + 1).to_string(),
            };
            last = n.split('.').next().and_then(|w| w.parse().ok()).or(last);
            n
        })
        .collect()
}

/// The series as published: sorted by id, every episode with its number.
pub fn publish(series: &[Series]) -> Vec<Series> {
    let mut out: Vec<Series> = series
        .iter()
        .map(|written| {
            let mut s = written.clone();
            for (part, n) in s.episodes.iter_mut().zip(numbers(written)) {
                part.number = Some(n);
            }
            s
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn is_number(s: &str) -> bool {
    let mut parts = s.split('.');
    let whole = parts.next().unwrap_or_default();
    let frac = parts.next();
    parts.next().is_none()
        && !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && frac.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
}

/// Everything wrong with one series that can be told on its own.
pub fn problems(series: &Series, entries: &[Entry], catalog: &Catalog) -> Vec<String> {
    let mut out = Vec::new();
    let mut fail = |msg: String| out.push(msg);

    if !is_slug(&series.id) {
        fail("the file name must be lower-case letters, digits and single dashes".into());
    }
    if series.name.trim().is_empty() {
        fail("name is empty".into());
    }
    if series
        .description
        .as_ref()
        .is_some_and(|d| d.trim().is_empty())
    {
        fail("description is empty".into());
    }
    for (lang, t) in &series.translations {
        if !LANGUAGES.contains(&lang.as_str()) {
            fail(format!(
                "translations: `{lang}` is not one of {}",
                LANGUAGES.join(", ")
            ));
        }
        if t.name.trim().is_empty() {
            fail(format!("translations.{lang}.name is empty"));
        }
        match &t.description {
            Some(d) if d.trim().is_empty() => {
                fail(format!("translations.{lang}.description is empty"))
            }
            Some(_) if series.description.is_none() => fail(format!(
                "translations.{lang}.description translates a missing description"
            )),
            _ => {}
        }
    }
    if series.episodes.len() < 2 {
        fail("a series needs at least two episodes".into());
    }

    let mut seen = BTreeSet::new();
    let parts = series
        .episodes
        .iter()
        .map(|p| ("episodes", p))
        .chain(series.bonus.iter().map(|p| ("bonus", p)));
    for (list, part) in parts {
        let at = match (&part.mod_id, &part.game) {
            (Some(m), None) => {
                if !entries.iter().any(|e| e.id == *m) {
                    fail(format!("{list}: unknown mod `{m}`"));
                }
                format!("mod `{m}`")
            }
            (None, Some(g)) => {
                if !catalog.contains_key(g) {
                    fail(format!("{list}: unknown game `{g}`"));
                }
                format!("game `{g}`")
            }
            _ => {
                fail(format!("{list}: a part names either `mod` or `game`"));
                continue;
            }
        };
        if !seen.insert(at.clone()) {
            fail(format!("{list}: {at} is in the series twice"));
        }
        if part.title.as_ref().is_some_and(|t| t.trim().is_empty()) {
            fail(format!("{list}: {at}: title is empty"));
        }
        match &part.number {
            Some(_) if list == "bonus" => fail(format!("bonus: {at}: bonus parts have no number")),
            Some(n) if !is_number(n) => fail(format!(
                "{list}: {at}: number `{n}` must be digits, optionally with a dot"
            )),
            _ => {}
        }
    }
    let mut numbers_seen = BTreeSet::new();
    for n in numbers(series) {
        if !numbers_seen.insert(n.clone()) {
            fail(format!("episodes: number {n} is used twice"));
        }
    }
    out
}

/// Mods and games that are in more than one series: the app shows one.
pub fn overlap_problems(series: &[Series]) -> Vec<String> {
    let mut owner: BTreeMap<String, &str> = BTreeMap::new();
    let mut out = Vec::new();
    for s in series {
        for part in s.episodes.iter().chain(&s.bonus) {
            let key = match (&part.mod_id, &part.game) {
                (Some(m), None) => format!("mod `{m}`"),
                (None, Some(g)) => format!("game `{g}`"),
                _ => continue,
            };
            match owner.get(&key) {
                Some(other) if *other != s.id => {
                    out.push(format!("{}: {key} is in the series `{other}` too", s.id))
                }
                Some(_) => {}
                None => {
                    owner.insert(key, &s.id);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry;

    fn entries() -> Vec<Entry> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample");
        ["sample-levels", "sample-patch"]
            .iter()
            .map(|id| {
                entry::parse(&std::fs::read_to_string(dir.join(format!("{id}.yaml"))).unwrap())
                    .unwrap()
            })
            .collect()
    }

    fn catalog() -> Catalog {
        [("keen1", ["keen1-1.31"]), ("keen4", ["keen4-1.4-ega"])]
            .into_iter()
            .map(|(g, v)| (g.to_string(), v.iter().map(|v| v.to_string()).collect()))
            .collect()
    }

    fn check(yaml: &str) -> Vec<String> {
        problems(&parse(yaml, "toast").unwrap(), &entries(), &catalog())
    }

    const GOOD: &str = r#"
name: Toast
description: Two parts.
translations: { de: { name: Toast, description: Zwei Teile. } }
episodes:
  - mod: sample-patch
    title: Patched
  - game: keen1
bonus:
  - mod: sample-levels
"#;

    #[test]
    fn a_good_series_passes() {
        assert_eq!(check(GOOD), Vec::<String>::new());
    }

    #[test]
    fn unknown_fields_are_refused() {
        let yaml = GOOD.replace("name: Toast\n", "name: Toast\nauthor: x\n");
        assert!(parse(&yaml, "toast").is_err());
        let yaml = GOOD.replace("title: Patched", "titel: Patched");
        assert!(parse(&yaml, "toast").is_err());
    }

    #[test]
    fn unknown_ids_are_found() {
        let yaml = GOOD
            .replace("sample-patch", "no-such-mod")
            .replace("keen1", "keen9");
        let p = check(&yaml);
        assert!(p.contains(&"episodes: unknown mod `no-such-mod`".to_string()));
        assert!(p.contains(&"episodes: unknown game `keen9`".to_string()));
    }

    #[test]
    fn a_part_names_one_thing() {
        let yaml = GOOD.replace("  - game: keen1", "  - game: keen1\n    mod: sample-levels");
        assert!(check(&yaml).contains(&"episodes: a part names either `mod` or `game`".into()));
    }

    #[test]
    fn one_episode_is_no_series() {
        let yaml = GOOD.replace("  - game: keen1\n", "");
        assert!(check(&yaml).contains(&"a series needs at least two episodes".into()));
    }

    #[test]
    fn a_mod_comes_once() {
        let yaml = GOOD.replace(
            "bonus:\n  - mod: sample-levels",
            "bonus:\n  - mod: sample-patch",
        );
        assert!(check(&yaml).contains(&"bonus: mod `sample-patch` is in the series twice".into()));
    }

    #[test]
    fn a_mod_is_in_one_series() {
        let a = parse(GOOD, "toast").unwrap();
        let b = parse(GOOD, "bread").unwrap();
        let p = overlap_problems(&[a, b]);
        assert!(p.contains(&"bread: mod `sample-patch` is in the series `toast` too".into()));
        assert!(p.contains(&"bread: game `keen1` is in the series `toast` too".into()));
    }

    #[test]
    fn numbers_count_on_from_the_one_before() {
        let mut s = parse(GOOD, "toast").unwrap();
        assert_eq!(numbers(&s), ["1", "2"]);
        s.episodes[0].number = Some("0".into());
        assert_eq!(numbers(&s), ["0", "1"]);
        s.episodes[0].number = Some("3.5".into());
        assert_eq!(numbers(&s), ["3.5", "4"]);
    }

    #[test]
    fn numbers_are_checked() {
        let yaml = GOOD.replace(
            "  - game: keen1",
            "  - game: keen1
    number: \"1\"",
        );
        assert!(check(&yaml).contains(&"episodes: number 1 is used twice".into()));
        let yaml = GOOD.replace("    title: Patched", "    number: two");
        assert!(check(&yaml)[0].contains("number `two` must be digits"));
        let yaml = GOOD.replace(
            "  - mod: sample-levels",
            "  - mod: sample-levels\n    number: \"3\"",
        );
        assert!(
            check(&yaml).contains(&"bonus: mod `sample-levels`: bonus parts have no number".into())
        );
    }

    #[test]
    fn translations_are_checked() {
        let yaml = GOOD.replace("de: { name: Toast,", "fr: { name: Toast,");
        assert!(check(&yaml)[0].starts_with("translations: `fr` is not one of"));
        let yaml = GOOD.replace("description: Two parts.\n", "");
        assert!(
            check(&yaml)
                .contains(&"translations.de.description translates a missing description".into())
        );
    }

    #[test]
    fn publishing_numbers_every_episode() {
        let mut s = parse(GOOD, "toast").unwrap();
        s.episodes[1].number = Some("5".into());
        let b = parse(GOOD, "bread").unwrap();
        let out = publish(&[s, b]);
        assert_eq!(out[0].id, "bread");
        assert_eq!(out[1].episodes[0].number.as_deref(), Some("1"));
        assert_eq!(out[1].episodes[1].number.as_deref(), Some("5"));
        assert_eq!(out[1].bonus[0].number, None);
    }
}

//! KeenWiki link import (PLAN.md §9.3): reads a single mod page and finds
//! what to download.
//!
//! Politeness rules: only on the user's request, one page per request,
//! cached for a day, honest `User-Agent`, and only the normal `/wiki/` HTML
//! (robots.txt disallows `/w/index.php?`). The parser only looks at the mod
//! infobox, the first paragraph and the "Credits" section; it's covered by
//! saved pages in `tests/keenwiki/` because wiki markup can change.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::Url;
use scraper::{ElementRef, Html, Node, Selector};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::detect::sha256_hex;
use crate::downloads::{self, USER_AGENT};
use crate::error::{Error, Result};

pub const HOST: &str = "keenwiki.shikadi.net";

/// A page is fetched again at most once a day.
const CACHE_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// Mod pages are ~50 KB.
const MAX_PAGE: usize = 4 * 1024 * 1024;

/// Far more than any Keen mod.
const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;

/// Namespaces that never hold mod pages.
const NAMESPACES: &[&str] = &[
    "special",
    "file",
    "image",
    "media",
    "user",
    "category",
    "template",
    "help",
    "talk",
    "mediawiki",
    "property",
    "module",
    "keenwiki",
];

/// What a KeenWiki mod page says about the mod.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WikiMod {
    pub page_url: String,
    pub name: String,
    /// Catalog game id from "Modded episode".
    pub game_id: Option<String>,
    pub authors: Vec<String>,
    /// Author name → their page, where the page links one (usually
    /// `User:<name>` on KeenWiki).
    pub author_links: BTreeMap<String, String>,
    /// e.g. `Complete`, `Beta`.
    pub status: Option<String>,
    /// As written on the page, usually `YYYY-MM-DD`.
    pub released: Option<String>,
    pub updated: Option<String>,
    pub levels: Option<String>,
    /// The discussion thread on PCKF.
    pub pckf: Option<String>,
    /// The discussion thread on the Keen: Modding forum.
    pub keenmodding: Option<String>,
    /// The mod's own website.
    pub homepage: Option<String>,
    pub description: Option<String>,
    pub credits: Vec<String>,
    /// As listed; the first one is the newest.
    pub downloads: Vec<WikiDownload>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WikiDownload {
    pub url: String,
    /// The link text, e.g. `newest` or `v.1.0`.
    pub label: String,
    /// Text after the link, e.g. `v.2.0` or `less buggy`.
    pub note: Option<String>,
    /// From the label or the note, e.g. `2.0`.
    pub version: Option<String>,
}

/// The canonical page URL for a pasted link, e.g.
/// `keenwiki.shikadi.net/wiki/the_Alphamatic#Credits` →
/// `https://keenwiki.shikadi.net/wiki/The_Alphamatic`.
pub fn page_url(input: &str) -> Result<String> {
    let input = input.trim();
    let bad = || Error::NotKeenWikiLink(input.to_string());
    let with_scheme = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    let url = Url::parse(&with_scheme).map_err(|_| bad())?;
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if !matches!(url.scheme(), "http" | "https") || host != HOST {
        return Err(bad());
    }
    let title = url
        .path()
        .strip_prefix("/wiki/")
        .ok_or_else(bad)?
        .replace("%20", "_");
    let title = title.trim_matches('_');
    let namespace = title.split_once(':').or_else(|| title.split_once("%3A"));
    if title.is_empty()
        || namespace.is_some_and(|(ns, _)| {
            let ns = ns.to_ascii_lowercase();
            NAMESPACES.contains(&ns.as_str()) || ns.ends_with("_talk")
        })
    {
        return Err(bad());
    }
    // MediaWiki titles start with a capital letter.
    let mut chars = title.chars();
    let first = chars.next().expect("not empty").to_uppercase();
    Ok(format!("https://{HOST}/wiki/{first}{}", chars.as_str()))
}

/// Whether two links point at the same page.
pub fn same_page(a: &str, b: &str) -> bool {
    matches!((page_url(a), page_url(b)), (Ok(a), Ok(b)) if a == b)
}

/// The page's HTML, from the cache when it's fresh.
pub async fn fetch(cache_dir: &Path, page_url: &str) -> Result<String> {
    let file = cache_dir.join(format!("{}.html", &sha256_hex(page_url.as_bytes())[..16]));
    let fresh = fs::metadata(&file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < CACHE_FOR);
    if fresh && let Ok(html) = fs::read_to_string(&file) {
        return Ok(html);
    }

    tracing::info!("fetching {page_url}");
    let client = reqwest::Client::builder().user_agent(USER_AGENT).build()?;
    let resp = client.get(page_url).send().await?;
    // MediaWiki answers pages that don't exist with 404.
    if resp.status().as_u16() == 404 {
        return Err(Error::NotAModPage);
    }
    if !resp.status().is_success() {
        return Err(Error::HttpStatus(resp.status().as_u16()));
    }
    let body = resp.bytes().await?;
    if body.len() > MAX_PAGE {
        return Err(Error::TooLarge(MAX_PAGE as u64));
    }
    let html = String::from_utf8_lossy(&body).into_owned();
    fs::create_dir_all(cache_dir)?;
    fs::write(&file, &html)?;
    Ok(html)
}

/// Forgets every page [`fetch`] kept, so the next read gets it fresh.
pub fn clear_cache(cache_dir: &Path) -> Result<()> {
    match fs::remove_dir_all(cache_dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

fn selector(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

/// Text with collapsed whitespace.
fn text(el: ElementRef) -> String {
    el.text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A paragraph's text without footnote markers such as `[1]`.
fn prose(el: ElementRef) -> String {
    let reference = selector("sup.reference");
    el.descendants()
        .filter(|n| {
            !n.ancestors()
                .filter_map(ElementRef::wrap)
                .any(|a| reference.matches(&a))
        })
        .filter_map(|n| n.value().as_text().map(|t| &**t))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn non_empty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

pub fn parse(html: &str, page_url: &str) -> Result<WikiMod> {
    let doc = Html::parse_document(html);
    let infobox = doc
        .select(&selector(".infobox"))
        .next()
        .ok_or(Error::NotAModPage)?;
    let rows: Vec<(String, ElementRef)> = infobox
        .select(&selector("tr"))
        .filter_map(|tr| {
            let th = tr.select(&selector("th")).next()?;
            let td = tr.select(&selector("td")).next()?;
            Some((text(th), td))
        })
        .collect();
    let row = |label: &str| {
        rows.iter()
            .find(|(l, _)| l.eq_ignore_ascii_case(label))
            .map(|(_, td)| *td)
    };
    let row_text = |label: &str| row(label).map(text).and_then(non_empty);
    // Game pages have an infobox too, without this row.
    let episode = row("Modded episode").ok_or(Error::NotAModPage)?;

    let heading = doc
        .select(&selector("#firstHeading"))
        .next()
        .map(text)
        .and_then(non_empty);
    let name = infobox
        .select(&selector("[itemprop=name]"))
        .next()
        .map(text)
        .and_then(non_empty)
        // The infobox shows the page title, which MediaWiki capitalizes; the
        // heading keeps a lowercase start set with `DISPLAYTITLE`.
        .map(|n| match &heading {
            Some(h) if h.to_lowercase() == n.to_lowercase() => h.clone(),
            _ => n,
        })
        .or(heading)
        .unwrap_or_else(|| {
            page_url
                .rsplit('/')
                .next()
                .unwrap_or("Mod")
                .replace('_', " ")
        });
    let authors = row_text("Author(s)")
        .map(|t| split_names(&t))
        .unwrap_or_default();
    let links = |label: &str| -> Vec<String> {
        row(label)
            .map(|td| {
                td.select(&selector("a[href]"))
                    .filter_map(|a| a.value().attr("href"))
                    .filter(|h| h.starts_with("http"))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let first_link = |label: &str| links(label).into_iter().next();
    let discussions = links("Discussion(s)");
    let thread = |on_forum: fn(&str, &str) -> bool| {
        discussions
            .iter()
            .find(|h| {
                Url::parse(h)
                    .ok()
                    .and_then(|u| Some(on_forum(u.host_str()?, u.path())))
                    .unwrap_or(false)
            })
            .cloned()
    };
    let base = Url::parse(page_url).ok();
    let author_links = row("Author(s)")
        .map(|td| author_links(td, base.as_ref(), &authors))
        .unwrap_or_default();
    let mut downloads = row("Download")
        .map(|td| parse_downloads(td, base.as_ref()))
        .unwrap_or_default();
    if downloads.is_empty()
        && let Some(url) = infobox
            .select(&selector("meta[itemprop=downloadUrl]"))
            .find_map(|m| m.value().attr("content"))
    {
        downloads.push(WikiDownload {
            url: url.replace("&#58;", ":"),
            label: "newest".into(),
            note: None,
            version: None,
        });
    }

    Ok(WikiMod {
        page_url: page_url.to_string(),
        name,
        game_id: game_of(&text(episode)).map(str::to_string),
        authors,
        author_links,
        status: row_text("Status"),
        released: row_text("Release date"),
        updated: row_text("Latest update"),
        levels: row_text("Number of levels"),
        // Before pckf.com, PCKF was hosted on Yuku; older threads are in the
        // Shikadi archive.
        pckf: thread(|h, _| h.ends_with("pckf.com")).or_else(|| {
            thread(|h, p| {
                h.starts_with("publiccommanderkeenforum.")
                    || (h == "archive.shikadi.net" && p.starts_with("/pckf/"))
            })
        }),
        keenmodding: thread(|h, _| h.ends_with("keenmodding.org")),
        homepage: first_link("Homepage"),
        description: doc
            .select(&selector(".mw-parser-output > p"))
            .map(prose)
            .find(|t| !t.is_empty()),
        credits: credits(&doc),
        downloads,
    })
}

/// `Keen 5` → `keen5`, `Keen Dreams` → `kdreams`.
fn game_of(episode: &str) -> Option<&'static str> {
    let e = episode.to_ascii_lowercase();
    if e.contains("dreams") {
        return Some("kdreams");
    }
    let digit = e.split("keen").nth(1)?.trim().chars().next()?;
    Some(match digit {
        '1' => "keen1",
        '2' => "keen2",
        '3' => "keen3",
        '4' => "keen4",
        '5' => "keen5",
        '6' => "keen6",
        _ => return None,
    })
}

fn split_names(s: &str) -> Vec<String> {
    s.replace(" & ", ",")
        .replace(" and ", ",")
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .collect()
}

/// Links to existing pages in the "Author(s)" cell, for names in `authors`.
/// Red links (pages that don't exist yet) are left out.
fn author_links(
    td: ElementRef,
    base: Option<&Url>,
    authors: &[String],
) -> BTreeMap<String, String> {
    td.select(&selector("a[href]"))
        .filter(|a| {
            !a.value()
                .has_class("new", scraper::CaseSensitivity::CaseSensitive)
        })
        .filter_map(|a| {
            let name = text(a);
            let href = a.value().attr("href")?;
            let url = match base {
                Some(b) => b.join(href).ok()?,
                None => Url::parse(href).ok()?,
            };
            let page = matches!(url.scheme(), "http" | "https")
                && !url.query_pairs().any(|(k, _)| k == "redlink");
            (page && authors.contains(&name)).then(|| (name, url.to_string()))
        })
        .collect()
}

/// Links in the "Download" cell, one per line: `<a>label</a> note<br>`.
/// A whole line may be in brackets: `(<a>label</a> note)`.
fn parse_downloads(td: ElementRef, base: Option<&Url>) -> Vec<WikiDownload> {
    let mut out: Vec<WikiDownload> = Vec::new();
    // Text before the line's first link, the one before the latest link,
    // and the text after it.
    let mut lead = String::new();
    let mut last_lead = String::new();
    let mut note = String::new();
    let mut on_line = false;
    let finish = |out: &mut Vec<WikiDownload>, lead: &str, note: &mut String| {
        if let Some(d) = out.last_mut()
            && d.note.is_none()
        {
            let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
            let (lead, n) = (squash(lead), squash(note));
            let n = match (lead.strip_prefix('('), n.strip_suffix(')')) {
                (Some(""), Some(n)) => n.trim(),
                _ => n
                    .strip_prefix('(')
                    .and_then(|n| n.strip_suffix(')'))
                    .unwrap_or(&n),
            };
            d.note = non_empty(n.to_string());
            d.version = version_in(&d.label).or_else(|| d.note.as_deref().and_then(version_in));
        }
        note.clear();
    };
    for node in td.children() {
        match node.value() {
            Node::Element(e) if e.name() == "a" => {
                finish(&mut out, &last_lead, &mut note);
                let href = e.attr("href").unwrap_or("");
                let url = match base {
                    Some(b) => b.join(href).ok(),
                    None => Url::parse(href).ok(),
                };
                if let Some(url) = url.filter(|u| matches!(u.scheme(), "http" | "https"))
                    && !out.iter().any(|d| d.url == url.as_str())
                {
                    out.push(WikiDownload {
                        url: url.to_string(),
                        label: text(ElementRef::wrap(node).expect("an element")),
                        note: None,
                        version: None,
                    });
                    last_lead = std::mem::take(&mut lead);
                }
                on_line = true;
            }
            Node::Element(e) if e.name() == "br" => {
                finish(&mut out, &last_lead, &mut note);
                lead.clear();
                on_line = false;
            }
            Node::Text(t) if on_line => note.push_str(t),
            Node::Text(t) => lead.push_str(t),
            _ => {}
        }
    }
    finish(&mut out, &last_lead, &mut note);
    out
}

/// `v.1.2`, `v1.2`, `version 1.2` → `1.2`.
fn version_in(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    lower.match_indices('v').find_map(|(i, _)| {
        let before = lower[..i].chars().next_back();
        if before.is_some_and(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        let rest = &lower[i + 1..];
        let rest = rest.strip_prefix("ersion").unwrap_or(rest);
        let rest = rest.trim_start_matches(['.', ' ']);
        let v: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let v = v.trim_end_matches('.');
        (v.starts_with(|c: char| c.is_ascii_digit())).then(|| v.to_string())
    })
}

/// How an installed mod compares to what its page offers now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStatus {
    UpToDate,
    Newer,
    /// The page's newest download can't be told apart from the installed
    /// one: neither its link nor the versions say.
    Unknown,
    /// The page has nothing to download.
    NoDownload,
}

/// Compares the installed download (`url`, `version`) with the page's
/// newest one. Downloads are listed newest first; mirrors are copies of the
/// newest one.
pub fn update_status(page: &WikiMod, url: &str, version: Option<&str>) -> UpdateStatus {
    let Some(newest) = page.downloads.first() else {
        return UpdateStatus::NoDownload;
    };
    let found = page.downloads.iter().position(|d| same_url(&d.url, url));
    if found == Some(0)
        || found.is_some_and(|i| {
            page.downloads[i]
                .label
                .to_ascii_lowercase()
                .contains("mirror")
        })
    {
        return UpdateStatus::UpToDate;
    }
    let have = found
        .and_then(|i| page.downloads[i].version.as_deref())
        .or(version);
    match newest
        .version
        .as_deref()
        .zip(have)
        .and_then(|(n, h)| compare_versions(n, h))
    {
        Some(std::cmp::Ordering::Greater) => UpdateStatus::Newer,
        Some(_) => UpdateStatus::UpToDate,
        None if found.is_some() => UpdateStatus::Newer,
        None => UpdateStatus::Unknown,
    }
}

/// Links differing only in the scheme or the host's case are the same.
fn same_url(a: &str, b: &str) -> bool {
    match (Url::parse(a.trim()), Url::parse(b.trim())) {
        (Ok(a), Ok(b)) => a.host_str() == b.host_str() && a.path() == b.path(),
        _ => a.trim() == b.trim(),
    }
}

/// `1.10` > `1.9`, `2` = `2.0`; `None` unless both are dotted numbers.
fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let parse = |v: &str| -> Option<Vec<u32>> { v.split('.').map(|p| p.parse().ok()).collect() };
    let (mut a, mut b) = (parse(a)?, parse(b)?);
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    Some(a.cmp(&b))
}

/// The list items of the "Credits" section.
fn credits(doc: &Html) -> Vec<String> {
    let Some(heading) = doc
        .select(&selector(".mw-parser-output > h2"))
        .find(|h| h.select(&selector("#Credits")).next().is_some())
    else {
        return Vec::new();
    };
    heading
        .next_siblings()
        .filter_map(ElementRef::wrap)
        .take_while(|e| e.value().name() != "h2")
        .flat_map(|e| e.select(&selector("li")).map(text).collect::<Vec<_>>())
        .filter(|t| !t.is_empty())
        .collect()
}

/// How far the download of a KeenWiki link has got, in bytes; `total` is 0
/// while unknown.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct KeenwikiProgress {
    pub url: String,
    pub received: u32,
    pub total: u32,
}

/// Downloads a mod archive into the empty folder `dir`. The file keeps its
/// name (KeenWiki archive names carry metadata) and gets a `.zip` extension
/// if it is one. `progress` gets (received, total) bytes.
pub async fn download(dir: &Path, url: &str, progress: &impl Fn(u64, u64)) -> Result<PathBuf> {
    let parsed = Url::parse(url).map_err(|_| Error::UnsupportedDownload(url.into()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(Error::UnsupportedDownload(url.into()));
    }
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir)?;
    let raw = dir.join("download");
    let fetched = downloads::fetch_unverified(url, MAX_DOWNLOAD, &raw, progress).await?;

    let mut head = [0u8; 512];
    let n = {
        use std::io::Read;
        fs::File::open(&raw)?.read(&mut head)?
    };
    match sniff(&head[..n]) {
        Kind::Zip => {}
        other => {
            let _ = fs::remove_file(&raw);
            return Err(match other {
                Kind::WebPage => Error::WebPage,
                Kind::Rar => Error::UnsupportedDownload("RAR archive".into()),
                Kind::SevenZip => Error::UnsupportedDownload("7z archive".into()),
                Kind::Exe => Error::UnsupportedDownload("EXE file".into()),
                Kind::Zip | Kind::Other => Error::UnsupportedDownload("unknown file type".into()),
            });
        }
    }

    let name = fetched.file_name.unwrap_or_else(|| {
        Url::parse(&fetched.final_url)
            .ok()
            .and_then(|u| u.path_segments()?.next_back().map(str::to_string))
            .unwrap_or_default()
    });
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_matches(['.', '_']).to_string();
    let mut name = if name.is_empty() {
        "mod".to_string()
    } else {
        name
    };
    if !name.to_ascii_lowercase().ends_with(".zip") {
        name.push_str(".zip");
    }
    let path = dir.join(name);
    fs::rename(&raw, &path)?;
    tracing::info!(
        "downloaded {url} ({} bytes, sha256 {})",
        fetched.size,
        fetched.sha256
    );
    Ok(path)
}

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Zip,
    Rar,
    SevenZip,
    Exe,
    WebPage,
    Other,
}

fn sniff(head: &[u8]) -> Kind {
    let trimmed = head.trim_ascii_start();
    // Old PKZIP put a spanning marker (`PK00`) before single-disk archives too.
    let zip = head.strip_prefix(b"PK00").unwrap_or(head);
    if zip.starts_with(b"PK\x03\x04") || zip.starts_with(b"PK\x05\x06") {
        Kind::Zip
    } else if head.starts_with(b"Rar!") {
        Kind::Rar
    } else if head.starts_with(b"7z\xBC\xAF\x27\x1C") {
        Kind::SevenZip
    } else if head.starts_with(b"MZ") {
        Kind::Exe
    } else if trimmed.starts_with(b"<") {
        Kind::WebPage
    } else {
        Kind::Other
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn fixture(name: &str) -> WikiMod {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/keenwiki")
            .join(format!("{name}.html"));
        let html = fs::read_to_string(path).unwrap();
        parse(&html, &format!("https://{HOST}/wiki/{name}")).unwrap()
    }

    #[test]
    fn descriptions_leave_out_footnote_markers() {
        let html = Html::parse_fragment(
            r##"<p>Galaxy Defender is a mod.<sup id="cite_ref-1" class="reference"><a href="#cite_note-1">&#91;1&#93;</a></sup> It scrolls.</p>"##,
        );
        let p = html.select(&selector("p")).next().unwrap();
        assert_eq!(prose(p), "Galaxy Defender is a mod. It scrolls.");
    }

    #[test]
    fn page_urls_are_canonical() {
        let ok = "https://keenwiki.shikadi.net/wiki/The_Alphamatic";
        for input in [
            ok,
            "  http://keenwiki.shikadi.net/wiki/The_Alphamatic#Credits ",
            "keenwiki.shikadi.net/wiki/the_Alphamatic",
            "https://KeenWiki.Shikadi.net/wiki/The%20Alphamatic?foo=1",
        ] {
            assert_eq!(page_url(input).unwrap(), ok, "{input}");
        }
        assert!(same_page(ok, "keenwiki.shikadi.net/wiki/the_Alphamatic"));
        assert!(!same_page(ok, "https://keenwiki.shikadi.net/wiki/Keenball"));
    }

    #[test]
    fn other_links_are_refused() {
        for input in [
            "",
            "https://example.com/wiki/The_Alphamatic",
            "https://keenwiki.shikadi.net.evil.com/wiki/X",
            "https://keenwiki.shikadi.net/w/index.php?title=The_Alphamatic&action=raw",
            "https://keenwiki.shikadi.net/wiki/",
            "https://keenwiki.shikadi.net/wiki/Special:Random",
            "https://keenwiki.shikadi.net/wiki/User:Gridlock",
            "https://keenwiki.shikadi.net/wiki/File:The_Alphamatic.png",
            "https://keenwiki.shikadi.net/wiki/User_talk:Gridlock",
            "ftp://keenwiki.shikadi.net/wiki/X",
        ] {
            assert!(
                matches!(page_url(input), Err(Error::NotKeenWikiLink(_))),
                "{input}"
            );
        }
        // A colon alone doesn't make a namespace.
        assert!(page_url("https://keenwiki.shikadi.net/wiki/Keen_7:_Foo").is_ok());
    }

    #[test]
    fn parses_a_mod_with_several_versions() {
        let m = fixture("The_Alphamatic");
        assert_eq!(m.name, "The Alphamatic");
        assert_eq!(m.game_id.as_deref(), Some("keen5"));
        assert_eq!(m.authors, ["Gridlock"]);
        assert_eq!(m.status.as_deref(), Some("Complete"));
        assert_eq!(m.released.as_deref(), Some("2017-07-04"));
        assert_eq!(m.updated.as_deref(), Some("2026-03-14"));
        assert_eq!(m.levels.as_deref(), Some("25"));
        assert_eq!(
            m.pckf.as_deref(),
            Some("https://pckf.com/viewtopic.php?f=4&t=4086")
        );
        assert_eq!(m.homepage, None);
        assert!(
            m.description
                .as_deref()
                .unwrap()
                .starts_with("The Alphamatic is a Keen 5 mod released on the Fourth of July 2017")
        );
        assert_eq!(m.credits.len(), 5);
        assert_eq!(m.credits[0], "Creator: Gridlock");
        assert_eq!(
            m.downloads,
            [
                WikiDownload {
                    url: "https://files.shikadi.net/keenwiki/mods/Keen5/m.gridlock.the_alphamatic_v2.0.k5.2021-03-14.zip".into(),
                    label: "newest".into(),
                    note: Some("v.2.0".into()),
                    version: Some("2.0".into()),
                },
                WikiDownload {
                    url: "https://files.shikadi.net/keenwiki/mods/Keen5/m.gridlock.the_alphamatic_v1.0.k5.2017-07-04.zip".into(),
                    label: "v.1.0".into(),
                    note: None,
                    version: Some("1.0".into()),
                },
            ]
        );
    }

    #[test]
    fn parses_a_long_version_list() {
        let m = fixture("Buried_in_Oblivion");
        assert_eq!(m.game_id.as_deref(), Some("keen4"));
        assert!(m.downloads.len() >= 4, "{:?}", m.downloads);
        assert_eq!(m.downloads[0].version.as_deref(), Some("1.4"));
        assert_eq!(m.downloads[1].label, "v.1.3");
        assert_eq!(m.downloads[1].note.as_deref(), Some("2019"));
        assert_eq!(m.downloads[1].version.as_deref(), Some("1.3"));
    }

    #[test]
    fn parses_keen_dreams_with_a_mirror() {
        let m = fixture("Candy_Dreams");
        assert_eq!(m.game_id.as_deref(), Some("kdreams"));
        assert_eq!(m.status.as_deref(), Some("Alpha"));
        assert_eq!(m.updated, None);
        let labels: Vec<&str> = m.downloads.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(labels, ["newest", "Mirror Download"]);
        assert_eq!(
            m.downloads[1].url,
            "http://levellord.rewound.net/KDREAMS.zip"
        );
    }

    #[test]
    fn parses_several_authors_and_variants() {
        let m = fixture("Ye_Olde_Pirate_Modde");
        assert_eq!(m.game_id.as_deref(), Some("keen3"));
        assert_eq!(m.authors, ["Levellass", "Ceilick"]);
        assert_eq!(
            m.author_links,
            BTreeMap::from([
                (
                    "Ceilick".to_string(),
                    "https://keenwiki.shikadi.net/wiki/User:Ceilick".to_string()
                ),
                (
                    "Levellass".to_string(),
                    "https://keenwiki.shikadi.net/wiki/User:Levellass".to_string()
                ),
            ])
        );
        assert_eq!(m.downloads[1].label, "without music");
        assert_eq!(m.downloads[1].note.as_deref(), Some("less buggy"));
        assert_eq!(m.downloads[1].version, None);
    }

    #[test]
    fn parses_a_single_download() {
        let m = fixture("Keenball");
        assert_eq!(m.game_id.as_deref(), Some("keen4"));
        assert_eq!(m.status.as_deref(), Some("Beta"));
        assert_eq!(m.downloads.len(), 1);
        assert_eq!(m.downloads[0].note, None);
        assert_eq!(
            m.keenmodding.as_deref(),
            Some("https://keenmodding.org/viewtopic.php?f=13&t=3224")
        );
        // Only on Keen: Modding.
        assert_eq!(m.pckf, None);
    }

    #[test]
    fn brackets_around_a_download_line_are_not_notes() {
        let m = fixture("Keen_3_Part_2");
        let shown: Vec<_> = m
            .downloads
            .iter()
            .map(|d| (d.label.as_str(), d.note.as_deref()))
            .collect();
        assert_eq!(shown, [("newest", None), ("easy", None)]);
    }

    #[test]
    fn names_keep_a_lowercase_start() {
        let m = fixture("Xky1tuned");
        assert_eq!(m.name, "xky1tuned");
        assert_eq!(m.game_id.as_deref(), Some("keen1"));
    }

    #[test]
    fn game_and_missing_pages_are_not_mods() {
        for name in ["Keen_4", "Missing_Page"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/keenwiki")
                .join(format!("{name}.html"));
            let html = fs::read_to_string(path).unwrap();
            assert!(
                matches!(parse(&html, "https://x/wiki/X"), Err(Error::NotAModPage)),
                "{name}"
            );
        }
    }

    #[test]
    fn author_links_skip_red_links_and_other_names() {
        let html = r#"<table class="infobox"><tr><th>Modded episode</th><td>Keen 4</td></tr>
            <tr><th>Author(s)</th><td>
              <a href="/wiki/User:Nisaba">Nisaba</a>,
              <a href="/index.php?title=User:Newbie&amp;action=edit&amp;redlink=1" class="new">Newbie</a>,
              <a href="javascript:alert(1)">Plain</a> &amp; Unlinked
              <a href="/wiki/Somewhere">see also</a>
            </td></tr></table>"#;
        let m = parse(html, &format!("https://{HOST}/wiki/Test")).unwrap();
        assert_eq!(
            m.authors,
            ["Nisaba", "Newbie", "Plain", "Unlinked see also"]
        );
        assert_eq!(
            m.author_links,
            BTreeMap::from([(
                "Nisaba".to_string(),
                format!("https://{HOST}/wiki/User:Nisaba")
            )])
        );
    }

    #[test]
    fn homepage_is_the_first_web_link() {
        let html = r#"<table class="infobox"><tr><th>Modded episode</th><td>Keen 1</td></tr>
            <tr><th>Homepage</th><td><a class="external text"
              href="http://web.archive.org/web/20050507103508/http://www.avk.frac.dk/">AvK Official Site</a></td></tr>
            </table>"#;
        let m = parse(html, &format!("https://{HOST}/wiki/Aliens_vs._Keen")).unwrap();
        assert_eq!(
            m.homepage.as_deref(),
            Some("http://web.archive.org/web/20050507103508/http://www.avk.frac.dk/")
        );
    }

    #[test]
    fn discussions_are_told_apart_by_forum() {
        let page = |links: &str| {
            let html = format!(
                r#"<table class="infobox"><tr><th>Modded episode</th><td>Keen 4</td></tr>
                <tr><th>Discussion(s)</th><td>{links}</td></tr></table>"#
            );
            parse(&html, &format!("https://{HOST}/wiki/The_Keys_of_Krodacia")).unwrap()
        };
        // As on The Keys of Krodacia: the thread on pckf.com wins over the
        // one from PCKF's Yuku days.
        let m = page(
            r#"<a href="http://publiccommanderkeenforum.yuku.com/topic/886/">PCKF</a>
            <a href="https://pckf.com/viewtopic.php?t=566">PCKF</a>
            <a href="http://keenmodding.org/viewtopic.php?t=834">Keen: Modding</a>"#,
        );
        assert_eq!(
            m.pckf.as_deref(),
            Some("https://pckf.com/viewtopic.php?t=566")
        );
        assert_eq!(
            m.keenmodding.as_deref(),
            Some("http://keenmodding.org/viewtopic.php?t=834")
        );
        // As on Lego Keen: an old thread in the Shikadi archive.
        let m = page(
            r#"<a href="http://archive.shikadi.net/pckf/unofficial/865.htm">PCKF</a>
            <a href="http://archive.shikadi.net/other/1.htm">Elsewhere</a>"#,
        );
        assert_eq!(
            m.pckf.as_deref(),
            Some("http://archive.shikadi.net/pckf/unofficial/865.htm")
        );
        assert_eq!(m.keenmodding, None);
    }

    #[test]
    fn episodes_map_to_games() {
        assert_eq!(game_of("Keen 1"), Some("keen1"));
        assert_eq!(game_of("Commander Keen 6"), Some("keen6"));
        assert_eq!(game_of("Keen Dreams"), Some("kdreams"));
        assert_eq!(game_of("Keen 9"), None);
        assert_eq!(game_of("Other"), None);
    }

    #[test]
    fn versions_in_labels() {
        assert_eq!(version_in("v.1.2").as_deref(), Some("1.2"));
        assert_eq!(version_in("v1.0.2").as_deref(), Some("1.0.2"));
        assert_eq!(version_in("Version 3").as_deref(), Some("3"));
        assert_eq!(version_in("(v. 2.0)").as_deref(), Some("2.0"));
        assert_eq!(version_in("newest"), None);
        assert_eq!(version_in("Mirror Download"), None);
        assert_eq!(version_in("dev build"), None);
    }

    #[test]
    fn installed_downloads_compare_to_the_page() {
        use UpdateStatus::*;
        let page = fixture("The_Alphamatic");
        let (v2, v1) = (&page.downloads[0].url, &page.downloads[1].url);
        assert_eq!(update_status(&page, v2, None), UpToDate);
        // Another scheme is the same file.
        assert_eq!(
            update_status(&page, &v2.replace("https", "http"), None),
            UpToDate
        );
        assert_eq!(update_status(&page, v1, None), Newer);
        // A link the page doesn't list: the versions tell.
        assert_eq!(update_status(&page, "C:/aurora.zip", Some("1.0")), Newer);
        assert_eq!(update_status(&page, "C:/aurora.zip", Some("2")), UpToDate);
        assert_eq!(update_status(&page, "C:/aurora.zip", Some("2.1")), UpToDate);
        assert_eq!(update_status(&page, "C:/aurora.zip", Some("beta")), Unknown);
        assert_eq!(update_status(&page, "C:/aurora.zip", None), Unknown);

        let mirrored = fixture("Candy_Dreams");
        assert_eq!(
            update_status(&mirrored, "http://levellord.rewound.net/KDREAMS.zip", None),
            UpToDate
        );
        let none = WikiMod {
            downloads: vec![],
            ..page.clone()
        };
        assert_eq!(update_status(&none, v2, None), NoDownload);
    }

    #[test]
    fn versions_compare_by_number() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("1.10", "1.9"), Some(Greater));
        assert_eq!(compare_versions("2", "2.0"), Some(Equal));
        assert_eq!(compare_versions("1.0", "1.0.1"), Some(Less));
        assert_eq!(compare_versions("1.0b", "1.0"), None);
    }

    /// Answers `GET /<path>` with the route's raw response head and body.
    fn serve(routes: Vec<(&'static str, String, Vec<u8>)>) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let count = hits.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                loop {
                    let mut h = String::new();
                    reader.read_line(&mut h).unwrap();
                    if h.trim().is_empty() {
                        break;
                    }
                }
                count.fetch_add(1, Ordering::SeqCst);
                let path = line.split_whitespace().nth(1).unwrap_or("");
                let (head, body) = routes
                    .iter()
                    .find(|(p, ..)| *p == path)
                    .map(|(_, h, b)| (h.clone(), b.clone()))
                    .unwrap_or(("HTTP/1.1 404 Not Found".into(), Vec::new()));
                let _ = stream.write_all(
                    format!("{head}\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes(),
                );
                let _ = stream.write_all(&body);
            }
        });
        (base, hits)
    }

    #[tokio::test]
    async fn downloads_zips_and_explains_everything_else() {
        let mut zip = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut zip));
            w.start_file("MOD.CK4", zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(b"x").unwrap();
            w.finish().unwrap();
        }
        let ok = "HTTP/1.1 200 OK".to_string();
        let (base, _) = serve(vec![
            ("/m.a.mod.k4.2020-01-01.zip", ok.clone(), zip.clone()),
            (
                "/dl",
                format!("{ok}\r\nContent-Disposition: attachment; filename=\"My Mod.ZIP\""),
                zip.clone(),
            ),
            (
                "/r",
                "HTTP/1.1 302 Found\r\nLocation: /files/thing".into(),
                Vec::new(),
            ),
            ("/files/thing", ok.clone(), zip.clone()),
            (
                "/page",
                ok.clone(),
                b"<!DOCTYPE html><html>login</html>".to_vec(),
            ),
            ("/rar", ok.clone(), b"Rar!\x1a\x07\x00".to_vec()),
        ]);
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("dl");

        let none = |_, _| {};

        let reports = std::sync::Mutex::new(Vec::new());
        let path = download(
            &dir,
            &format!("{base}/m.a.mod.k4.2020-01-01.zip"),
            &|received, total| reports.lock().unwrap().push((received, total)),
        )
        .await
        .unwrap();
        assert_eq!(path, dir.join("m.a.mod.k4.2020-01-01.zip"));
        assert_eq!(fs::read(&path).unwrap(), zip);
        let size = zip.len() as u64;
        assert_eq!(reports.lock().unwrap().last(), Some(&(size, size)));
        // Each download starts from an empty folder.
        let path = download(&dir, &format!("{base}/dl"), &none).await.unwrap();
        assert_eq!(path, dir.join("My_Mod.ZIP"));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        let path = download(&dir, &format!("{base}/r"), &none).await.unwrap();
        assert_eq!(path, dir.join("thing.zip"));

        let err = |e: Error| format!("{e:?}");
        assert_eq!(
            err(download(&dir, &format!("{base}/page"), &none)
                .await
                .unwrap_err()),
            "WebPage"
        );
        assert_eq!(
            err(download(&dir, &format!("{base}/rar"), &none)
                .await
                .unwrap_err()),
            "UnsupportedDownload(\"RAR archive\")"
        );
        assert_eq!(
            err(download(&dir, &format!("{base}/gone"), &none)
                .await
                .unwrap_err()),
            "DeadLink(404)"
        );
        assert!(matches!(
            download(&dir, "file:///etc/passwd", &none).await,
            Err(Error::UnsupportedDownload(_))
        ));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn pages_are_cached_and_missing_ones_are_not_mods() {
        let html =
            fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/keenwiki/Keenball.html"))
                .unwrap();
        let (base, hits) = serve(vec![("/wiki/Keenball", "HTTP/1.1 200 OK".into(), html)]);
        let tmp = tempfile::tempdir().unwrap();
        let url = format!("{base}/wiki/Keenball");
        let first = fetch(tmp.path(), &url).await.unwrap();
        let second = fetch(tmp.path(), &url).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(parse(&first, &url).unwrap().name, "Keenball");
        // Cleared, it's read again; clearing nothing is fine too.
        let cache = tmp.path().join("keenwiki");
        fetch(&cache, &url).await.unwrap();
        clear_cache(&cache).unwrap();
        clear_cache(&cache).unwrap();
        fetch(&cache, &url).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 3);
        assert!(matches!(
            fetch(tmp.path(), &format!("{base}/wiki/Nope")).await,
            Err(Error::NotAModPage)
        ));
    }

    #[test]
    fn downloads_are_sniffed() {
        assert_eq!(sniff(b"PK\x03\x04rest"), Kind::Zip);
        assert_eq!(sniff(b"PK00PK\x03\x04rest"), Kind::Zip);
        assert_eq!(sniff(b"PK00rest"), Kind::Other);
        assert_eq!(sniff(b"Rar!\x1a\x07"), Kind::Rar);
        assert_eq!(sniff(b"7z\xBC\xAF\x27\x1C"), Kind::SevenZip);
        assert_eq!(sniff(b"MZ\x90\x00"), Kind::Exe);
        assert_eq!(sniff(b"\r\n <!DOCTYPE html>"), Kind::WebPage);
        assert_eq!(sniff(b"hello"), Kind::Other);
    }
}

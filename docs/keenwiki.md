# KeenWiki mods

Players can paste a link to a mod page on
[KeenWiki](https://keenwiki.shikadi.net) and install the mod from there.
This covers reading the page, downloading the mod, deciding which game and
copy it plays on, and checking for updates later. The page is read in
`src-tauri/src/keenwiki.rs`, the dialog is `src/mods/KeenwikiImport.tsx`.

## The link

Any link into the wiki works, with or without `https://`, with a `#section`
or a lowercase start: `keenwiki.shikadi.net/wiki/the_Alphamatic#Credits`
becomes `https://keenwiki.shikadi.net/wiki/The_Alphamatic`. Links to other
hosts, outside `/wiki/`, or into namespaces that never hold mods (`User:`,
`File:`, `Category:`, talk pages, …) are refused.

## Politeness

The wiki is read only when the player asks, one page per request, with an
honest `User-Agent`, and only the normal `/wiki/` HTML (robots.txt
disallows `/w/index.php?`). Pages are cached for a day in the cache folder
(`keenwiki/`), and nothing else on the wiki is fetched.

## 1. Is the mod in the index?

If a curated index entry links to the same page, that entry is offered
instead, without fetching the page. This is checked again with the page's
own address, as the link may have led there under another name, and once
more after downloading: the archive may be an index entry's download even
when the page isn't the one the index links. The entry's tested recipe and
required versions are then used, and the rules below don't apply.

## 2. What the page says

The parser only reads the mod infobox, the first paragraph and the "Credits"
section. Markup can change, so it's tested against saved pages
(`src-tauri/tests/keenwiki/`, see docs/testing.md).

- **Mod page or not.** A page needs an infobox with a "Modded episode" row;
  game pages have an infobox without one. A page that doesn't exist (404)
  isn't a mod page either.
- **Name.** The infobox's name, in the spelling of the page heading (which
  keeps a lowercase start), else the heading, else the page title.
- **Details.** "Author(s)" (split at `,`, `&` and `and`, with links to
  authors' pages that exist), "Status", "Release date", "Latest update",
  "Number of levels", "Homepage", and the threads on PCKF (including its old
  Yuku and Shikadi archive addresses) and Keen: Modding under
  "Discussion(s)". The description is the first paragraph.
- **Downloads.** The links in the "Download" cell, one per line, newest
  first, each with the text after it as a note (`v.2.0`, `less buggy`).
  Brackets around the note are dropped (`(less buggy)`). Where a whole line
  is in brackets, link included (`(<a>easy</a>)`), the closing bracket isn't
  a note. The version comes from the label or the note
  (`v.1.2`, `v1.2`, `version 1.2`).
  Without such links, the page's `downloadUrl` is taken.

The player sees these details and picks a download if there are several.

## 3. The download

The chosen file is downloaded (up to 256 MB) into the cache folder
(`downloads/keenwiki/`) and must be a ZIP archive, old PKZIP's spanning
marker included. RAR, 7z, EXE files and web pages (a download page instead
of the file) are refused, and the player can open the link in the browser.
A link to nothing (404 or 410) is called a dead link, and so is a 403 from
the community's file server, `files.shikadi.net`: it answers 403 for files
it doesn't have (Ally's Mod III links `.k2` for a `.k3` file). Elsewhere,
a 403 stays an HTTP error.
The file keeps its name, as KeenWiki archive names carry metadata
(`m.<author>.<name>_v<version>.<episode>.<date>`), and gets `.zip` if it
lacks it.

The archive is then inspected like a mod added by hand
([recipes.md](recipes.md)). The page's details win over what's guessed from
the archive: name, authors, description, release date and links. The version
is the download's, else the archive name's. The mod remembers its page and
download for update checks.

If the player already has a mod from the same page, or one with the same
name for the same game, the dialog offers to replace it or to install the
new one alongside.

## 4. Which game is the mod for?

The page's "Modded episode" (Keen 1–6 or Keen Dreams) is only a hint. The
downloaded files decide, in this order:

1. a patcher program for one episode,
2. the episode named inside a patch file,
3. the episode data files (the most common episode wins),
4. an episode's executable,
5. the episode in the archive name,
6. and only then the page's "Modded episode".

If the page and the files disagree, the files win and the player is told.

## 5. Which versions does it need?

- **The patch file decides.** If it names game versions (e.g. 1.4), the mod
  plays on those, and on third-party builds based on them.
- **A patch that names no version**, or a mod without a patch, plays on any
  version.
- **Copies of an unknown version** are always allowed, since the catalog
  doesn't know every release. If the patch names versions, the player is
  told that the version couldn't be checked.
- **A known version the patch doesn't name is refused**, even when the
  catalog doesn't know the versions it does name (e.g. a CGA-only patch on
  the EGA release).

So a copy fits when it's the right game and the mod either has no version
requirement, or the copy's version is unknown, or the copy's version (or
the version it's built on) is one the mod needs.

## 6. Which copy does it go on?

Among the copies that fit, BloogBase picks:

1. an official release before a modified third-party build, and either one
   before a copy of unknown version,
2. then the game's preferred versions, in order,
3. and on a tie, the copy added first.

When a mod replaces an older version of itself, it stays on the old one's
copy if that fits. The player can pick another fitting copy, both while
installing and later. The rule is `mods::fit_rank` in Rust and `bestFit` in
`src/catalog.ts`.

## Warnings and waiting

None of these stop the install:

- A patch that names no version is most likely made for the version most
  mods use (the game's modding version). If the chosen copy is a known,
  different one, the player is warned.
- A patch going onto a modified third-party build gets a warning.
- If no copy fits, the mod is installed anyway and waits for its game. As
  soon as a fitting copy is added, the mod moves onto it.

## Updates

On the mod's page, "Check for updates" reads its KeenWiki page again (from
the cache if it was read today) and compares the page's newest download with
the installed one:

- **Up to date** if the installed download is the newest link or a mirror of
  it, or if its version isn't lower than the newest one's.
- **Newer** if the newest download has a higher version, or if the installed
  download is further down the list and the versions don't say otherwise.
- **Unknown** if neither the link nor the versions tell them apart; the
  newest download is offered anyway.
- **No download** if the page lists none.

Links count as the same if they differ only in the scheme or the host's
case. Versions compare as dotted numbers (`1.10` > `1.9`, `2` = `2.0`). An
update is downloaded and installed over the old mod through the same dialog.

The check also takes over what the page now says about the mod: authors,
description, release date, homepage and threads. What the page leaves out
stays. Mods from the index keep the index's details. They ask the index
first and their KeenWiki page only when the index has nothing newer;
updating from the page takes the mod out of the index.

## Suggesting a mod for the index

A mod installed from KeenWiki that isn't in the index shows a link that opens
a prefilled GitHub issue. It has a draft index entry with the page's
details, the download, its SHA-256 and the recipe, to check by hand
(index/README.md).

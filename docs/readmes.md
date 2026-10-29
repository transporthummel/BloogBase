# Mod documents

Mods ship their documentation under all kinds of names (`README.TXT`,
`Read Me.txt`, `READ.ME`, `CHEEKY README.rtf`, `LIESMICH.TXT`, …) and in all
kinds of formats. BloogBase finds it, ranks it and shows it as plain text:
in the README tab of a mod's profile page and, before installing, behind the
"readme" link of the "Add mod" dialog. The code is in `src-tauri/src/readme.rs`,
the panel in `src/profile/ReadmePanel.tsx`.

## Where it looks

- **Installed mods:** the kept archive (`mods/<id>/source/*.zip`), so files
  outside the installed mod folder count too, like a readme next to the
  folder the mod was zipped in. Without an archive (a mod added from a
  folder), the installed files.
- **Before installing:** the folder or ZIP the player picked or downloaded.

Files at most two folders deep count; deeper ones belong to tools or sources
shipped along. Files over 1 MB are hardly a README and are skipped (web
pages and PDF documents aren't read, so their size doesn't matter). At most
20 documents are listed.

## Which files count

A file counts by its extension: `txt`, `md`, `markdown`, `nfo`, `doc`,
`docx`, `rtf`, `asc`, `htm`, `html`, `pdf`, also `.ME` and `.1ST` (`READ.ME`,
`README.1ST`). Files without an extension count only if their name says
README. The name is compared lowercase, with everything but letters and
digits taken out (`Read_Me` → `readme`).

| Rank | Names                                                                                                             |
| ---- | ----------------------------------------------------------------------------------------------------------------- |
| 0    | Exactly `readme`, `readmefirst`, `readme1st`, `readthis`, `readfirst`, `liesmich`, `lisezmoi`, `leame`, `leesmij` |
| 1    | Containing `readme`, `readthis` or `liesmich` (`CHEEKY README.rtf`)                                               |
| 2    | `info`, `about`, `help`, `docs`, `documentation`, `anleitung`, containing `manual` or `instruction`, any `.nfo`   |
| 3    | Any other document (`story.txt`)                                                                                  |
| 4    | Change logs: containing `change`, `history`, `whatsnew`, `news`, `version`                                        |
| 5    | Spoilers last: containing `hint`, `solution`, `walkthrough`, `cheat`, `secret`, `spoiler`                         |

CKPatch's own manual (`ckpatch*`) comes along with the patcher in many mods
and never counts. Within a rank, text documents go before web pages and PDF
documents (only text shows in the app), then shallower files before deeper ones, then by
path. The first one is the README and opens first; the others are in a
picker.

## What's left out

After reading, a document is dropped if

- it's binary (a NUL byte before the end) and not a Word document,
- it's empty,
- it's a CKPatch script: its first line that isn't empty starts with `%`,
- it's one of the games' own documents, which mods ship along with a copy
  of a shareware episode (Apogee's `VENDOR.DOC`, Keen 4's `README.DOC`, Keen
  Dreams' `HELPINFO.TXT`, …). These are known by the SHA-256 of their decoded
  text (`GAME_DOCUMENTS`), so line ends and end-of-file markers don't matter.

## Decoding text

The text goes to the interface as plain text only.

1. **Encoding.** A UTF-8 or UTF-16 byte order mark decides. Otherwise valid
   UTF-8 is taken as is. Other text is DOS code page 437 or Windows-1252:
   bytes Windows-1252 leaves undefined mean DOS. Otherwise box drawing and
   accented letters below `0xE0` count for DOS, small accented letters and
   typographic punctuation for Windows, and the larger count wins.
2. **RTF** (`{\rtf`) is reduced to its text: paragraphs, tabs, typographic
   quotes and dashes, `\'hh` (Windows-1252) and `\uN` escapes. Font tables,
   style sheets, pictures, headers, footers, field instructions and other
   hidden groups are skipped.
3. **Cleaning.** Text ends at the DOS end-of-file marker (Ctrl-Z). Line ends
   become Unix ones, form feeds become line breaks, and ANSI escape
   sequences and other control characters except tabs are dropped.
   Whitespace at the end goes.

## Word documents

A `.doc` or `.docx` that is really a Word file shows its text and a button
that opens it in the player's word processor, for formatting, tables and
pictures. A `.doc` that is plain text is shown like any other text.

- **Word 97 and later** (`.doc`, OLE compound file): the text pieces from
  the piece table, 8-bit or UTF-16.
- **Word 6/95**: the text in one run. Fast-saved documents can't be read
  that way.
- **Word 2007 and later** (`.docx`): the text runs of `word/document.xml`,
  with tabs and line breaks.

Headers, footnotes and field codes are left out (field results stay), and
from `.docx` also deleted text. Encrypted, fast-saved or damaged documents show no text, only
the note to open them in their app.

## Web pages

HTML is never shown in the app. A web page is listed without text and opens
in the browser. From an archive it's copied to the cache first
(`readmes/<mod>/`, or `readme-preview/` before installing), with every file
in its folder, so pictures and style sheets load too. A Word document from an
archive is copied there the same way. The copy is made fresh each time, as an
update of the mod may have changed it.

## PDF documents

PDF is never shown in the app either: WebView2's viewer would bring its
own print and save toolbar, WebKitGTK has none, and text taken out of a PDF
loses its pictures and often its order. A PDF document is listed without
text and opens in the player's PDF viewer, copied to the cache from an
archive like a Word document (Bananasauros' `Bananasauros Instruction
Manual.pdf`).

## In the panel

The text shows in a monospace font, wrapped and selectable. `http` and
`https` links become buttons that open in the browser, never in the app's
window; trailing punctuation isn't part of the link. The profile page shows
the README tab only for mods that have documents, and opens with it until
the player picks another tab. When a mod is replaced by a newer version, its
documents are read again.

## Readmes for installing

`analyze` in `src-tauri/src/mods.rs` reads the same documents for one more
thing. A mod of levels alone has nothing that says how the mod it belongs
over starts. Its readme may say: xky1rooms' "Then run XkyKeen1.bat". DOS
names of batch files and programs (8.3, `.BAT` or `.EXE`) that the readme
names and the mod lacks become its missing files, except patchers and the
episode's executables. BloogBase then looks for the mod that has them, and
the dialog names the missing files with a link to the readme ([recipes.md](recipes.md)).

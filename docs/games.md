# Finding and importing games

BloogBase ships no game files. Players bring their own copies (a folder, a
ZIP, a Steam or GOG install) or download the free versions from public
archives. This covers how games are recognized, how their version is
identified, and what an import does. The code is in
`src-tauri/src/detect.rs`, `catalog.rs`, `library.rs`, `stores.rs` and
`free_games.rs`.

## The catalog

`src-tauri/catalog/games.json` describes the seven games (Keen 1–6 and Keen
Dreams). Per game:

- **`executables`**: one of these must be there (`KEEN4E.EXE`,
  `KEEN4C.EXE`, …).
- **`requiredFiles`**: all of these must be there; `A|B` means either.
- **`saves`, `stateFiles`**: patterns (`?`, `*`, case-insensitive) for
  files the game writes: saved games, settings, high scores.
- **`versions`**: every known release, with the SHA-256 of its files.
- **`moddingVersion`**: the release most mods are made for, where the
  community settled on one (v1.4 EGA for Keen 4–6).
- **`preferredVersions`**: where mods go when several copies fit, best
  first.

Per version:

- **`files`**: file name to SHA-256. At least one executable; other files
  tell apart releases that share one.
- **`kind`**: release, beta or demo; **`shareware`**.
- **`patchVersion`**: what patch files call it in `%version` (`1.4`), if
  they're made for it.
- **`start`**: a program to launch instead of the episode executable.
- **`original`**: for third-party builds (copy protection removed,
  re-releases), the version they were made from. Mods for that version are
  offered, but patches may not fit, so the player is warned.
  **`repackaged`** builds have the original's game files, only packaged
  differently, so patches fit too.

## Detection

`detect` looks at every folder of the source on its own: games are flat, so
subfolders are only candidates of their own, never merged. Inside a ZIP,
folders count the same way.

1. **Which game.** A folder holds a game when it has one of its executables
   and all its required files. A folder may hold several games (several episodes
   in one folder); each is found.
2. **Which version.** The catalog's versions are tried in order. A version
   matches when every file it lists is there with the right SHA-256. Each
   file is hashed at most once. If none matches, the version is unknown.
   That's allowed: the copy is imported, and mods treat it with the benefit
   of the doubt.
3. **Which files.** Files matching a save or state pattern of any game are
   state; everything else in the folder is the game itself.

## Importing

An import takes every game detected in the source:

1. **Duplicates.** A release already in the library would only be a second
   copy of the same files, so it's skipped. Releases are told apart by
   their catalog version, else by the SHA-256 of their executable. If
   nothing new is left, the import says what's already there.
2. **An id.** `<game>-<n>`, the first free one. It's claimed by creating
   its library folder, which is atomic even across concurrent imports. An
   id whose profile folder still exists is never reused: its saves would
   end up in the wrong place.
3. **Copying.** Game files go to `library/<id>/base/`, which is never
   written to again; state files to `profiles/<id>/state/`, the game's
   profile. Both are copied into a staging folder in the cache first and
   then moved into place, so a failed import leaves nothing half-done.
4. **Recording.** The install and its profile are written in one
   transaction, with where the files came from (a path or URL).
5. **Waiting mods.** Mods installed before their game was there move onto
   the new copy if it fits ([keenwiki.md](keenwiki.md), "Which copy does it
   go on?").

The original files are never touched or removed, also not when the game is
uninstalled.

### Damaged archives

Old ZIP archives often have entries whose checksum doesn't match, though
the file still works. Reading skips the check, as 7-Zip does. Deflate data
that refers to bytes before its start is inflated with zeros there, as
Info-ZIP and 7-Zip do. After importing, the app checks every entry and warns
which files may be damaged. Paths that would leave the target folder
(absolute ones, `..`) are refused, and macOS archive junk is skipped.

### Game or mod?

A file dropped onto the library may be a game or a mod. Many mods ship the
game's files, often with their own executable. So whatever starts through
a patch or a batch file is a mod, even where a game is detected; otherwise
detected games are imported as games.

## Steam and GOG

The setup wizard's scan looks only at the stores' own folders, never the
whole disk:

- **Steam:** its root (on Windows from the registry, `SteamPath`, and the
  program folders; on Linux the usual and Flatpak folders; on macOS
  Application Support) and every library listed in
  `steamapps/libraryfolders.vdf`, each with its `steamapps/common`.
- **GOG:** `C:\GOG Games`, GOG Galaxy's game folder, `~/GOG Games`, Heroic's
  folder, `/Applications` on macOS, and on Windows every GOG install path in
  the registry.

Of those, subfolders with "keen" in their name are scanned with `detect`.
Folders reached twice (a symlink, the registry and the default path) count
once, and an episode found twice in one folder (a backup copy) is listed
once. The scan shows what it found and which of it was imported from that
folder before; the player picks what to import.

## Free versions

`src-tauri/catalog/free-games.json` lists the versions that may be passed
around: the shareware episodes (Keen 1, Keen 4 EGA and CGA, Keen Dreams;
the setup wizard offers one per game), the demos of Keen 4 and 6 and the
Keen 1 betas. The Discover page offers all of them. Each has mirrors, mostly the links on KeenWiki's version pages,
each with its own archive SHA-256 and size.

Installing tries the mirrors in order. A download that fails or doesn't
match its hash moves on to the next one. The archive is imported only if
`detect` finds exactly the expected game and version in it, nothing else.

`cargo test free_games -- --ignored` downloads every mirror and checks it;
run it after changing the manifest.

## Uninstalling

Removing a game deletes its library folder, its profile and the profile's
caches (composed folder, cover). With "keep saves", the saves are archived
as a backup first; the profile's automatic backups go either way, the
player's own only without it ([saves.md](saves.md)). Mods played on it are either
removed too or move to another copy they fit; without one, they wait.

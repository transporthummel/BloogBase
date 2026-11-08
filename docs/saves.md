# Saves and backups

Every profile (a game, or a mod on a game) has its own saves. BloogBase
shows them with what each one says about itself, and keeps backups so a
save the game overwrote can come back. The code is in `src-tauri/src/saves.rs`,
`overlay_view.rs`, `backups.rs` and the `keen-saves` crate.

## Where saves live

DOSBox mounts drive C in two layers ([dosbox.md](dosbox.md)): the game (or
the mod over it) below, read only, and the profile's `state/` folder above.
Whatever the game writes lands in `state/`. Imported games bring the saves
found next to them there; mods may ship saves of their own, which stay in
the mod's files below until the game overwrites them.

The save list shows when each save was made from the file's modification
time, so copies keep it: files from a ZIP or folder (`Source::copy_to`)
get the entry's time, and the composed folder keeps its inputs' times
(Windows does that when copying anyway, Linux doesn't). From a ZIP, the
extended timestamp is used when the archive has one (UTC); otherwise the
DOS time every entry has, which is local time of an unknown zone and is
read as UTC, so it can be a few hours off.

### What DOS sees

`overlay_view.rs` merges the two layers as DOSBox Staging does, and every
feature that looks at a profile's files goes through it, so saves, backups
and covers agree with what DOS sees:

- Names compare case-insensitively, as in DOS. An upper file hides any
  lower file of that name; a lower file can't show through an upper folder
  of the same name.
- `DBOVERLAY_DEL_<NAME>` in an upper folder hides the lower file `<NAME>`,
  `DBOVERLAY_RMD_<NAME>` the lower folder and everything in it. Markers are
  never listed themselves.
- For a mod, the mod's own files stand in for the lower layer: the composed
  folder may not exist yet, and the base game's files never hold saves.

When BloogBase itself writes into `state/` (restoring a save), it first
removes a deletion marker that would keep hiding the file. Deleting a save
that came with the mod writes such a marker, as DOSBox would.

## Reading a save

Save files are found by the game's patterns in the catalog (`SAVED?.CK1`,
`SAVEGAM?.CK4`, `SAVEGM?.KDR`, …), in the drive root only. The `?` digit is the slot: Keen
1–3 show it as it is, the others number slots from 1 for file 0. Where two
saves share a slot (a mod saving under two names), the list shows their file
names too: "Slot 1 (SAVEGAM0.CKS)".

A mod may give these files other names. Its patch writes them over the
ones in the executable, as it does for its data files
([covers.md](covers.md)): Keen Q saves as `SAVED1.CKQ`, Norp the Yorp 2 as
`NSAVE1.NY2` with its settings in `SETTINGS.NY2`, Another Helping of
Veggies keeps them in `CONFIG.KD2`. So for a mod profile the patterns
follow (`saves::profile_game`): a whole name written where the catalog's
starts, else the stem before the `?` and the extension each on their own.
Only text that can be a DOS name counts. Everything that looks at saves or
state files (save list, backups, restores, the launcher clean-up) uses
these. The answer is kept while the files stay the same, also over
restarts in `cache/save-names.json` (by the fingerprint of the mod's files;
the library's profile list drops mods whose files changed or went away, and
`NAMES_VERSION` drops it all when the search changes). Unpacked executables
are kept by file, as many profiles share one.

A source mod's own executables may open them under another extension:
Foray in the Forest's `FITF-X.EXE` saves as `SAVEGAMx.CKS` and keeps its
settings in `CONFIG.CKS`, its `FITF-C.EXE` saves as `.CKC`. Where a program
in the mod's root names a pattern's stem, the `?` character(s) and a dot
with another extension than the game's, that pattern is added
(`saves::other_extensions`). These saves carry the extension as signature
and a layout of their own (see below).

`keen-saves` reads what a save says about itself. Saves come from any game
version or mod, so every read is bounds-checked; an unexpected file is an
error, never a panic, and the save is listed without details.

- **Keen 1–3:** a fixed 92-byte record. Lives, ammo (raygun shots) and score
  at fixed offsets. Levels done are counted as non-zero bytes over 32 bytes:
  that works both for the original 16 words and for the 32 levels of the
  level patch, since a finished level is stored as 1. Keen 1 also counts its
  four ship parts (battery, vacuum, joystick, whiskey; the pogo stick isn't
  one). These games only save on the world map.
- **Keen 4–6:** id's `SaveGame` header (the signature `CK4\0` to `CK6\0`, a
  pointer, a flag, the 33-byte name the player typed), then the episode's
  `gametype` struct, word-aligned. Levels done (25 words), score, ammo
  (neural stunner shots), whether it was saved on the world map, lives and
  difficulty. The offsets past the ammo differ by the episode's own items:
  two words in Keen 4, three in Keen 5, four in Keen 6.
- **Source mods:** the same header, mostly with a signature of their own,
  and a `gametype` from their source. Each is one `Layout` in
  `keen_saves::SOURCE_MODS`, read by one reader: whether the game state is
  word-aligned after the name, where the levels sit and how wide they are,
  the highest flag a level can have, maps that don't count, where score,
  ammo, map, lives and difficulty are, how many maps there are, and
  difficulties beyond Easy to Hard. A new source mod is a new entry plus a
  real save in the fixtures. Their saves have no version of their own, so
  values the game can't have written (an unknown difficulty or map, lives
  over 99, level flags too high) make the save show without details rather
  than wrong ones. On a profile of the episode, the first layout of the
  signature whose checks pass reads it. Under the episode's own signature
  (`CK4\0`) the episode's layout comes first where its levels read as 0 or
  1, and is taken last where no source mod's checks pass.
  - **Foray in the Forest 2.0** (Keen 4): `CKS\0` or `CKC\0` (its classic
    build). 27 level words (the world map and the opening rocket, which the
    game marks done before the first level, don't count; perfect is flag
    2), then score, two more longs, ammo, drops, three words of its own,
    map, lives and difficulty, with a fourth level, Expert. Checked against
    a real v2.0a save.
  - **Atroxian Realm 1.4** (on the Keen Dreams source, played as Keen 4):
    `CKD\0`, its file extension. Its game state is Keen 5's (`KD_DEF.H`): a
    level word for each of its 26 maps, score, `nextKeenAt`, ammo, six item
    words, map, lives and difficulty. A new game starts on map 25, the crash
    site, which is marked done after it, so it isn't counted. Earlier
    versions saved in another layout. Checked against real v1.4 saves.
  - **Return to the Shadowlands 1.8** (Keen 4): `CK4\0`, like Keen 4, but
    the game state follows the name without the padding byte, and has ten
    words between `nextKeenAt` and the map where Keen 4 has eight (ammo,
    drops, two items, four keys); the ammo is the third of the ten. Read in
    Keen 4's layout its level words come out as 0x0100 and the like, which
    tells the two apart. Map 20, the rocket the game starts on, is marked
    done from the start and map 19 holds the high scores; only maps 0-18
    count. Checked against real v1.8 saves.
  - **Keen Dreams Gold Edition** (on the Keen Dreams source, played as
    Keen 4): `KDG\0`, saved as `KDGOLD0.SAV` and on. Its game state
    (`KDG_DEF.H`) is packed in bytes: score before the levels, a done byte
    for each of 47 maps (21 and 22, where a new game starts, don't count),
    then items, map, lives and difficulty, with Keen and Grand (played as
    Mortimer) beyond Hard. The ammo is the flower powers, except on maps
    23-46, where the score box shows the gun's. It doesn't say whether it
    was saved on the world map. Checked against real saves of the
    2026-05-31 release.
- **Keen Dreams:** the same idea with `KDR\0`, 17 level slots (slot 0 is the
  world map, which the game marks as done on entering a level, so it isn't
  counted) and flower power seeds as ammo. Normal difficulty is stored as 0.

The offsets were checked against real saves of Keen 4 v1.4, Keen 5, Keen 6
v1.0 and Keen Dreams 1.13. Names are shown as ASCII,
other bytes as replacement characters.

## Global settings

The settings page can set sound, music, the sound device, the score box and
keys for all games (`game_settings`). Each stays with the game until set.

Before a session, the set values go into the settings file as DOS sees it,
as a file in `state/`. `generated/settings-session.json` records each changed
field's original and written value first. After the session each field still
as written gets its original back. Anything else stays: high scores, and what
the player changed in the game. A copy of a mod's file that ends up equal to
it goes away again. A session that never ended (a crash, or BloogBase quit
with the game running) is merged at the profile's next start, so backups made
before a launch hold the player's values.

Only files of a known size and signature are changed (`game_settings::format`),
others stay as they are. So do the games' diagonal keys, Quiet AdLib and
two-button firing. A setting overwrites an in-game change at the next start;
it comes back into effect when the setting goes back to unset.

## Backups

A backup is a ZIP in `backups/<profile>/` with a manifest
(`bloogbase-backup.json`: game, version, profile, label, time, files), so it
can be exported and imported elsewhere. The database also records each
file's SHA-256 per backup, which is how earlier versions of a save are found
without opening archives.

### What's in one

- **Everything in `state/`**, deletion markers included, so a restore puts
  back exactly what DOS saw. That's what manual backups and the automatic
  ones hold.
- **Single files as DOS sees them**, which may come from the mod below: the
  backup before deleting a save.

An automatic backup whose content (paths and data, hashed together) equals
the profile's newest backup isn't made again.

### When they're made

| When                                            | Kept                                                    |
| ----------------------------------------------- | ------------------------------------------------------- |
| By hand, with an optional label                 | Until the player deletes it                             |
| Before each launch (can be turned off)          | Only if the session overwrote or lost a save; see below |
| Before a restore                                | As an automatic backup                                  |
| Before deleting a save                          | As an automatic backup, offered to undo the deletion    |
| When a game or mod is removed with "keep saves" | Offered when it's added again                           |
| Before a launcher's copies are cleared          | As an automatic backup                                  |

Automatic backups are named by their reason, manual ones by the player's
label. Of the automatic ones, the newest 10 per profile are kept (1 to 100,
on the settings page); older ones are removed. Manual backups are never
pruned. A backup that fails before a launch is logged and never keeps
anyone from playing.

An automatic backup is skipped when the profile's newest backup holds the
same files. A backup before a launch counts for that only against another
one of its kind, since it goes again after a session that lost no save.

### Before and after a session

Most sessions change nothing worth keeping, so the backup before a launch
is temporary. After the session, the save files DOS sees now are hashed and
compared with the saves in that backup. If any save that existed before is
gone or has other content, the session overwrote it: the backup is kept and
named for that ("before overwriting"). Otherwise it's deleted. New saves
alone don't keep it, as nothing was lost.

### Earlier versions of a save

Every save in the save list says how many other versions of it the backups
hold: distinct contents of a file of that name, other than the current one,
newest first. Any of them can be put back, also into another slot.

### Restoring

A restore first backs up the current `state/`, then writes the chosen files
over it: by default only the saves, optionally also settings, high scores
and deletion markers, or a single save under another slot's name. If the
game wrote the file in other case, that file is replaced. Files not in the
backup stay as they are.

### Export and import

Exporting copies the backup's ZIP. Importing a ZIP adds it as a new backup
of a profile, to restore from there:

- A BloogBase export keeps everything, if its manifest names the same game.
- Any other archive contributes the files that look like this game's saves,
  config or high scores, wherever they are inside, one per name. Its file
  name becomes the label.

### Removed

When a game or mod is removed, so are its profile's automatic backups. With
"keep saves", the player's own backups stay, and so do the backups made for
this and earlier removals (one that was declined when the profile came back
has nothing newer). This removal's backup is made even when the newest one
holds the same files. Without "keep saves", all of the profile's backups go. The remove dialog shows the
"keep saves" checkbox when the profile has saves, or backups it decides
about (the player's own and removal backups), and names what it keeps.
Automatic backups of profiles removed before this rule existed are removed
on start.

### Added again

When a game or mod is removed with "keep saves", the backup made then is
offered when it comes back: for a profile never played, the newest such
backup made before it was added. A mod must be the same mod (same id), a
game the same game in any version. Restoring moves the backup, and the
other backups of the removed profile, to the new one. Either answer is
remembered for that profile; a profile added again later is asked again.

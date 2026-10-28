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
1–3 show it as it is, the others number slots from 1 for file 0.

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
- **Keen Dreams:** the same idea with `KDR\0`, 17 level slots (slot 0 is the
  world map, which the game marks as done on entering a level, so it isn't
  counted) and flower power seeds as ammo. Normal difficulty is stored as 0.

The offsets were checked against real saves of Keen 4 v1.4, Keen 5, Keen 6
v1.0 and Keen Dreams 1.13. Names are shown as ASCII,
other bytes as replacement characters.

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

### Added again

When a game or mod is removed with "keep saves", the backup made then is
offered when it comes back: for a profile never played, the newest such
backup made before it was added. A mod must be the same mod (same id), a
game the same game in any version. Restoring moves the backup, and the
other backups of the removed profile, to the new one. Either answer is
remembered for that profile; a profile added again later is asked again.

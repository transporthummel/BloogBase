# Composing and patching

Before a mod starts, BloogBase builds the folder DOSBox mounts as drive C
below the profile's saves: the game, the mod below if any, and the mod's
files on top, plus whatever the start needs. The original files are never
changed; everything here happens in a disposable copy. The code is in
`src-tauri/src/compose.rs`, `ckpatch.rs`, `firing.rs`, `custom_patches.rs`,
`patchfile.rs` and the `lzexe` module of `keen-graphics`.

## The composed folder

A profile gets one when it plays a mod, or for Keen 1–3 when one-button
firing is on. It lives in `cache/composed/<profile>/`.

1. **Layers.** The install's game files, then the files of the mod it's
   played over (`extends`), then the mod's own, all with their folders.
   Names compare case-insensitively, as in DOS, so a mod's `level01.ck1`
   replaces the game's `LEVEL01.CK1`.
2. **Left out.** Archives inside a mod (`.zip`, `.rar`, `.7z`: extras,
   sources) are never needed by the game. The saves of the mod below are
   left out too: they're for the levels this mod replaces.
3. **Up to date?** A hash of every input's path, size, modification time
   and source, the recipe, whether firing is on, and a version string
   (`composed v6`) is stored with the profile. Files are never changed in
   place, so that identifies them without reading them. If the hash matches
   and the folder exists, it's used as it is.
4. **Building.** The files are copied into a temporary folder, the steps
   below run there, and it then replaces the old folder. The new hash is
   stored.

The saves stay in the profile's `state/` folder above, mounted as an overlay
([dosbox.md](dosbox.md)), so rebuilding never touches them.

### Files a launcher left in `state/`

Some mods start through a launcher of their own that renames files on
every run: Keen Q's `KEENQ.COM` swaps `GEEL.PAT` and `GEEL3.PAT`, and
`LEVEL80.CKQ` and `LEVELX.CKQ`, and renames `KEEN2.EXE` and the patcher
while it runs. DOS writes each rename into `state/` like anything else, so
after the first run the profile holds copies, and deletion markers for the
old names, that hide the composed folder from then on. A mod update or
one-button firing would never reach the game.

So when the composed folder changes (its hash differs from the stored one,
or none is stored: a new profile, maybe with an earlier backup restored
into it, or one where a restore put back more than saves, which clears the
stored hash), `launcher_copies.rs` clears what the launcher left, for profiles whose
recipe runs a program: the files in the root of `state/` that the program
names as a whole file name, the deletion markers for those names, and the
`.OLZ` copy UNLZEXE keeps of an executable it names. Saves and the game's
other state files always stay, under any extension (Keen Q's
`CTLPANEL.CKQ` matches `CTLPANEL.CK2`). The launcher then starts as after a
fresh install. All of `state/` is backed up first ("Before refreshing the
mod's files", [saves.md](saves.md)).

What goes is decided by where a file comes from, never by its content: a
setting the player reset to the mod's default is byte for byte the mod's
file too. A launcher whose names can't be read (packed or computed) keeps
its copies, which is the old behaviour, not a loss.

Bump the version string when a change to these steps should rebuild every
composed folder.

## Unpacking for old patchers

Most Keen executables are packed with LZEXE 0.91. CKPatch before 0.9 only
patches unpacked ones and stops with "Did you remember to UNLZEXE the
executable?"; mods using it often left that step to the player. If the
recipe runs such a patcher (directly or from its batch file, recognized by
that message in the patcher), the episode's executables are written
unpacked, as UNLZEXE does.

The unpacker (`keen-graphics/src/lzexe.rs`) decodes the LZEXE 0.91 bit
stream into the load image and rebuilds the executable with a fresh header
and its relocation table, so DOS runs it like the original. The same module
gives the load image alone, which covers and the firing check read; PKLITE
1.x (Keen 4's special demo, Keen Dreams 1.0) is unpacked for reading only.

## A patcher for mods without one

Many mods ship only a patch file and expect the player to have CKPatch. A
mod with a patch file for its game (by `%ext`) and no patcher of its own,
renamed ones included, gets BloogBase's: CKPatch 0.10 for Keen 1–6,
KDrPatch 0.9.0 for Keen Dreams, unmodified (`src-tauri/assets/ckpatch/`).
Recipe detection already counts it ([recipes.md](recipes.md)).

### Adapting older patch files

CKPatch 0.10 reads some patch files differently from the versions they were
made for. Up to 0.9, a plain `%patch` value over `$FF` wrote just its low
byte; 0.10 stops with "Invalid numeric format" unless the value says `W` or
`L` (Robot Rebellion has `%patch $81D6 $1000`). So when the bundled patcher
is used, its patch files are rewritten: such values become their low byte,
as the older versions wrote them. Lines with quoted text, comments and
everything outside `%patch` stay as they are, and files are handled byte
for byte, since comments may be in any code page.

## A DPMI host

Programs built with DJGPP (recognized by `go32stub` in their first 2 KB) need
a DPMI host, which DOSBox doesn't have; Simulator Keen's level generator
stops with "no DPMI - Get csdpmi*b.zip" without one. If such a program is
in the root and no `CWSDPMI.EXE` is there, CWSDPMI r7 is added, unmodified
(`src-tauri/assets/cwsdpmi/`).

## One-button firing

Keen 1–3 shoot with Ctrl and Alt pressed together, which many keyboards
and players find awkward. With one-button firing on (settings page), Space
shoots and Enter opens the status window instead. The patch is KeenWiki's
"One button firing in Vorticons" for version 1.31.

### The patch

Per episode it changes three kinds of places in the load image:

- the two checks for the status window's key: Space becomes Enter,
- the six places where Keen checks both fire buttons before shooting: the
  two compares and the jump between them become a single compare of the
  Space key's state (and no-ops),
- one jump past such a check, which becomes no-ops.

Each place is stored as the bytes 1.31 ships and the bytes the patch writes
(`firing.rs`, `EPISODES`).

### Only where it fits

Many mods already patch the controls this way, or change them otherwise.
So the patch is applied only where the code is still exactly as the episode
shipped it. To know that, BloogBase reads the executable as DOS would run
it:

1. **What runs.** From the recipe: the patch file of a patch recipe; the
   patch files a launcher program names (Keen Q's `KEENQ.COM` swaps them
   around, so each one counts); the episode executable or patch files a
   batch file runs; or the executable itself. Names are found as DOS
   would, by their 8.3 alias too.
2. **What it changes.** For a patch file, the episode's load image is
   taken and the patch file's `%patch` and `%patchfile` writes are applied
   to it, as the patcher would for version 1.31 (sections for other
   `%version`s are skipped).
3. **Judging.** If every place has the patched bytes, the game fires with
   Space already. If every place has the shipped bytes, the patch fits.
   Anything else means another version or a different change, and nothing
   is patched.

With several targets, one that doesn't fit stops the patch for all.

### Applying it

- **Patch files** get the patch's `%patch` lines added before their `%end`,
  in a `%version 1.31` section if the file has version sections. The
  patcher then writes them with the mod's own changes.
- **An executable run directly** is written unpacked with the places
  patched.

With the setting on, the profile page says when the patch doesn't fit: a
mod that changes the controls differently, or a copy that isn't version
1.31. A game or mod that fires with Space already says so either way. The setting applies from the next start,
as it's part of the composed folder's hash.

## Custom patches

A player can enter patch file lines for a game copy in the Settings tab of
its profile page (`custom_patches.rs`). They're stored with the install
(`installs.custom_patches`), never exported, and go into the composed folder
of the game and of every mod played on that copy, after one-button firing,
so they come last. A copy with custom patches is composed even without a
mod. BloogBase ships no patches for this field and doesn't look for
particular ones.

- Only for versions with a `patchVersion` (or a `repackaged` build of one);
  other versions get no field.
- `%ext`, `%end` and `%version` are refused when saving, with the line:
  BloogBase writes them itself. So are offsets and values of `%patch` and
  `%patchfile` lines the patcher wouldn't take: CKPatch 0.10 reads every
  line, also in sections for other versions, and stops at the first bad one
  ("Invalid numeric format"), which would stop every mod on the copy.
  Checked in DOSBox Staging with Keen 4 v1.4: a trailing `%version 1.4`
  section in a patch file without sections of its own applies, and a bad
  value in a `%version 1.0` section stops the patcher too.
- **Patch files** of what the profile runs (the same targets as one-button
  firing) get the lines before their `%end`, always in a
  `%version <patchVersion>` section, so the patcher skips them on any other
  executable.
- **The episode's executable run directly** (the game itself, or a mod
  without a patch file) is written unpacked with the lines' `%patch` and
  `%patchfile` writes. Lines with other directives can't be applied that
  way, and an executable BloogBase can't unpack (PKLITE) can't be patched:
  then nothing is applied.
- **A program of the mod's own** is left alone.

The game's Settings tab says when its own start skips them; a mod's says
whether it gets them, with a link to the game copy. The text is part of the
composed folder's hash, so a change applies from the next start.

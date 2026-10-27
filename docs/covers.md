# Cover artwork

A profile's cover is the title screen of its game or mod, drawn from the
player's own files. Nothing is shipped: a profile whose title can't be read
keeps the placeholder. The files are read in `src-tauri/src/covers.rs`; the
decoders are in the `keen-graphics` crate (`src-tauri/keen-graphics`).

## Caching

Covers are made when they're first needed and kept in the cache folder
(`covers/<profile>.png`), next to a stamp file. The stamp holds a fingerprint
and the result: `png`, `scrambled` or `none`. The fingerprint is a hash of

- `VERSION` in `covers.rs` (bump it to make all covers again after an
  extraction fix),
- the mod's recipe,
- name, path, size and modification time of every file DOS sees in the drive
  root: the mod's over the mod below's over the game's.

Files aren't changed in place, so that's enough to notice a change. If the
fingerprint is unchanged, the stamp's result is used again, `none` included.
A mod waiting for its game is tried once with its own files, then again
once the game is there. Removing a profile deletes both files.

## What the mod's patch file says

When the recipe applies a patch file (its own, or the first `.PAT` a program
it runs names, like Keen Q's `KEENQ.COM`), the patch file is read for:

| Directive                        | Used for                                                       |
| -------------------------------- | -------------------------------------------------------------- |
| `%egahead`, `%egadict`           | Keen 4-6: the header and dictionary files instead of the usual |
| `%patchfile`                     | Files patched in, e.g. a CGA mod's `C16HEAD.CK4`               |
| `%patch … "NAME.EXT"`            | File names written over the executable, e.g. `KDREAMS2.CMP`    |
| `%patch <address> "text"`        | Keen 1-3: new names for the files the game opens               |
| `%patch` at a known palette spot | 16 VGA colors the game sets instead of the EGA ones            |
| the bytes `%patch` writes        | Composite CGA mode, VGA 640×480 (see `recipe.rs`)              |

Palette patches are recognized by their address: in Keen 1 adurdin's patch
at `$13054` and the fading palette patch at `$21850`, in Keen 4 K1n9_Duk3's
patch at `$3D5E`. The patch must start with 48 numbers up to 63 (red, green,
blue per color); they're scaled to 8 bits.

## Keen 1-3

The title screen is the title bitmap drawn over the title level, as the game
shows it:

1. **File names.** Usually `EGAHEAD`, `EGALATCH`, `LEVEL90` and `TILEHEAD`
   with the episode's extension. A patch may write other names over them in
   the executable (Norp the Yorp 3 opens `TITLES1.NY3`). A text counts when
   its `%patch` address is exactly where the original name sits in the
   unpacked executable. A file missing under a new extension is taken under
   the old one.
2. **Header.** `EGAHEAD` gives the plane size, where the 16×16 tiles and the
   bitmaps are, and whether `EGALATCH` is LZW-compressed. Keen 1 1.32 and
   later have a 64-byte header instead of 48; the fields after the screen
   graphics' move by 6 bytes.
3. **Background.** Unless the title fills the screen, the level's tiles are
   drawn behind it, scrolled past the two-tile border. Mods with the tileset
   loader patch name a tileset file for level 90 in `TILEHEAD` (16 bytes per
   level); its tiles are used instead. If the named file is missing, the
   tiles in `EGALATCH` are used, as the game does.
4. **Title.** The first bitmap is drawn at (32, 1), the game's spot; a
   bigger one goes into the corner, cut off at the edge. The "Press F1 for
   help" sign is left out: it sits somewhere else in each episode.
5. **Screen size.** 320×200, or 640×480 for a patch that switches to VGA
   mode 12h (Keen 1 640x480 Redux).

**Scrambled.** An uncompressed `EGALATCH` shorter than `EGAHEAD` says (Keen
Static, on purpose) is filled up with zeros as the game does, and the cover
is marked scrambled. The profile page then says that the game shows
scrambled graphics.

## Keen 4-6

The graphics are in `EGAGRAPH`, but its header (`EGAHEAD`) and Huffman
dictionary (`EGADICT`) are usually built into the executable. Instead of a
table of where they sit in each version, they're searched for:

1. **Executables.** The mod's own program first (its recipe may run a
   changed game executable), then the version's start executable, then the
   catalog's executables, all that exist. Each is unpacked (LZEXE 0.91 or
   PKLITE) to its load image.
2. **Header.** A separate file if the mod ships one (or names one with
   `%egahead`). Otherwise the header in the executable: a run of 3-byte
   offsets (4 in the Keen 4 special demo) ending in the size of the graphics
   file and starting at 0, rising in between, with empty slots allowed.
3. **Dictionary.** A separate file if there is one. Otherwise every spot in
   the executable where a dictionary could end (the root node followed by
   zero padding) is tried. The right one decodes the picture table (chunk 0)
   into plausible sizes. If none does, the chunks may be stored
   uncompressed (a patch can switch compression off).
4. **Pictures.** The pictures follow the fonts, whose number differs per
   game: they start at the first chunk from 3 on that has the size of
   picture 0.
5. **Title.** The original title picture's number, if the picture count says
   which game it is and that picture is still full-screen: mods change
   pictures, not their numbers. Otherwise the full-screen (320×200) picture
   with the most colors. In the original games the title is the only one
   using all 16.
6. **Other graphics files.** Renamed graphics files named in the patch go
   before `EGAGRAPH.CKn`.

**CGA.** If no EGA title is found, the CGA releases' `CGAGRAPH` is tried the
same way, with 2 bits per pixel. Mods may rename it (`C16GRAPH.CK4` in Keen 4
Composite CGA Edition) and patch their own header in with `%patchfile`; any
`*GRAPH.CKn` file other than EGA's counts. CGA pixels get the games' palette:
black, light cyan, light magenta, white.

## Keen Dreams

Keen Dreams 1.13 keeps `TITLESCR.LBM` in the Softdisk library `KDREAMS.CMP`
(LZH-compressed). The bitmap's `BMHD` and `BODY` are decoded like the game
does; the colors are the EGA ones. File names the patch writes into the
executable (`KDREAMS2.CMP`) are tried first.

## Colors

The image holds one of 16 color indexes per pixel. It becomes RGB with:

1. **Composite.** If it's a CGA title and the patch switches to 640×200 with
   the color burst on, the colors a composite monitor shows. Every two-bit
   pixel is two of the 640 one-bit pixels of a line. Four of those make one
   cycle of the color carrier, one of 16 artifact colors. Each one-bit pixel
   gets the mean of the five runs around it (closest to DOSBox's filtering),
   and every two make one pixel again.
2. **Palette.** The colors a palette patch sets.
3. **EGA.** The default EGA palette otherwise.

The PNG is written to a temporary file first and then renamed into place.

## Failure

Every decoder checks its input and returns an error instead of panicking:
the files come from any version or mod. The last error is logged ("no title
screen for …"), the stamp says `none`, and the profile shows the
placeholder until its files change.

# Launch recipes

A recipe says how a mod starts in DOSBox. Mods from the index bring a curated
one; for everything else BloogBase guesses it from the files the mod ships
(`src-tauri/src/recipe.rs`, used by `analyze` in `src-tauri/src/mods/analyze.rs`).

## Kinds

| Kind      | Fields                     | Runs                                                      |
| --------- | -------------------------- | --------------------------------------------------------- |
| `patch`   | `patcher`, `patch`, `args` | `CK4PATCH FIRE.PAT [args]`: the episode's patcher         |
| `program` | `program`, `args`          | A program the mod ships, e.g. a menu                      |
| `batch`   | `file`                     | A batch file the mod ships, as it is                      |
| `episode` | –                          | The install's own episode executable over the mod's files |

Every file a recipe names has to be in the mod's root. `episode` has no
command of its own: the launcher picks the executable of the install
(`profiles::find_executable`).

## Finding the mod root

Archives often wrap the mod in a folder, or keep levels in subfolders. The
root is the folder with the most Keen files directly inside it (episode data
like `.CK4`, `.PAT` files, patchers, episode executables). Folders that can
start something (a patcher, a patch, an executable) beat folders with data
only; on a tie the shallower one wins. Only bare names in that folder count
for the recipe.

Before guessing:

- **Patchers** are known by name (`CK1PATCH` … `CK6PATCH`, `KDRPATCH`) or, when
  renamed, by the usage line CKPatch prints (`Usage: CK3PATCH {patchfile}`;
  Norp the Yorp 3 ships it as `NORPER.EXE`).
- **The game** comes from the patcher, else a patch file's `%ext`, else the
  most common data extension, else the archive name.
- **Patch files** are `.PAT`, or any other text file (not a document like
  `.TXT`, `.BAT`) with at least two lines starting with a patch directive
  (`%ext`, `%version`, `%patch`, `%patchfile`, `%end`), like Norp the Yorp 3's
  `.NY3`.
- **Bundled patcher**: a mod with a patch file for its game but no patcher
  gets BloogBase's own CKPatch (`ckpatch.rs`) added to its names, so the
  guess can use it.

## The guess

`recipe::guess` collects candidates in tiers. The best tier that isn't
empty decides; the rest follow as alternatives, duplicates dropped.

### Tier 1: batch files

Most mods start with a batch file that runs the patcher, so batch files are
the strongest hint. They're tried `PLAY`, `START`, `RUN`, `GO` first, then one
named after the mod (XkyKeen4's `XkyKeen4.bat` before its `go4.bat`), then by
name. For each, `batch_commands` keeps the lines that start something: it
stops at Ctrl-Z, treats NUL as a space, and drops empty lines, `::` comments
and screen commands (`ECHO`, `REM`, `PAUSE`, `CLS`, `TYPE`, …).

A batch file is skipped when

- it has the same commands as one already seen (mods ship copies under two
  names), or
- every command only moves files (`REN`, `COPY`, `DEL`, …), runs a tool
  (`UNLZEXE`, `LIST`, `PKUNZIP`, `SETUP`, …), or runs a program the mod doesn't
  have. Install steps and documentation viewers start nothing.

What's left becomes a candidate. A batch file with exactly one command is
reduced to it:

- the episode executable without arguments → `episode`;
- a patcher with a file argument → `patch` (usually a `.PAT`, but any file
  will do, like `EASTERBB.CK1`; further words become `args`);
- another program the mod ships → `program`.

Before that, `-tedlevel <n>` is dropped from the arguments (any case, `-` or
`/`). TED5 passes it to start the game right in a level, and a batch file
left with it makes Keen 4 hang on a black screen after Quit, patched or not
(checked in DOSBox Staging 0.83). NY00123's Tests Level has it in a
`play.bat` added to the archive later; its author's `GO4.BAT`, which the
patch makes start in the test level anyway, gives the same recipe without it.
A batch file run as it is keeps the switch.

Anything else (several commands, a program that can't be resolved) runs the
batch file as it is: `batch`. Names are matched as DOS does: without
extension, cut to 8.3, or by their `~1` alias (`dunepa~1.EXE`).

### Tier 2: menus

When the mod has a patcher but no usable batch file, its own programs come
next: most likely a menu that runs the patcher itself (The Prisoner's
Dilemma's `DILEMMA.EXE`). Next to a patcher and a batch file, programs aren't
offered at all.

### Tier 3: patch files

With a patcher, every patch file no batch candidate uses becomes a `patch`
candidate, the largest first.

### Tier 4: programs

Without a patcher, the mod's own `.EXE` and `.COM` files, other than
patchers, episode executables and tools.

### Fallback

`episode` is added when the mod has no patcher, or when nothing else was
found. A mod of only levels and graphics thus plays with the original
executable.

### Confidence

The guess is confident when the deciding tier has exactly one candidate (or
none at all). Then the "Add mod" dialog starts the mod with the first one and
hides the others under "Advanced". Otherwise the player has to pick.

## After the guess

For each candidate `analyze` also works out:

- **Game versions**: the `%version` lines of the patch file (a version whose
  section says `%abort` is left out, `%version all` means any), mapped to
  catalog versions, third-party builds of them included. A mod with versions
  installs only on those; unknown versions of the install are let through
  (`check_install`, `fits` in `src/mods/recipes.ts`).
- **Missing files**: files the starting patch loads with `%patchfile` that the
  mod lacks. Such a mod is made to be copied over another one (Lego Levels
  over Lego Keen), and BloogBase looks for that mod (`base_mod`). For a mod
  that only has the `episode` fallback, the readme may name the start of the
  mod it belongs over instead (xky1rooms' "Then run XkyKeen1.bat").

Installed over another mod (`extends`), a level pack that would start with
`episode` takes the recipe of the mod below.

## Curated recipes

An index entry's `recipe` is used as is; the heuristics don't run for it
(`index/README.md`). `versionRecipes` gives other recipes for some game
versions (Norp the Yorp 3 has a patch per version). At launch
`recipe::for_version` picks the install's version, else the version its
third-party build is based on, else the default. Installing checks that all
files those recipes name are in the mod's root, or in the mod it extends.

## At launch

The launcher composes the game, the mod below (if any) and the mod into one
folder (`compose.rs`) and runs `Recipe::command` there. On the way:

- an old patcher that needs an unpacked executable gets one (`UNLZEXE`
  first);
- a mod without a patcher gets the bundled CKPatch;
- the patch file chooses the DOSBox machine when it needs a special one
  (`dosconf::mod_machine`): composite CGA for 640×200 with the color burst,
  Paradise SVGA for 128K of video memory at A0000h.

The recipe is part of the composition's hash, so changing it rebuilds the
folder.

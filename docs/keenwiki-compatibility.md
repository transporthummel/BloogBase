# Which games a KeenWiki mod plays on

When a mod is installed from a KeenWiki link, BloogBase decides in three
steps: which game the mod is for, which versions of that game it needs, and
which of the player's copies it goes on.

## 1. Is the mod in the index?

If a curated index entry links to the same KeenWiki page, that entry is
offered instead of the page. Its tested recipe and its required versions are
used, and the rules below don't apply.

## 2. Which game is the mod for?

The page's "Modded episode" field names the game (Keen 1–6 or Keen Dreams).
A page without that field isn't treated as a mod page.

That field is only a hint, though. Once the mod is downloaded, its files
decide, in this order:

1. a patcher program for one episode,
2. the episode named inside a patch file,
3. the episode data files (the most common episode wins),
4. an episode's executable,
5. the episode in the archive name,
6. and only then the page's "Modded episode".

If the page and the files disagree, the files win and the player is told.

## 3. Which versions does it need?

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

## 4. Which copy does it go on?

Among the copies that fit, BloogBase picks:

1. an official release before a modified third-party build, and either one
   before a copy of unknown version,
2. then the game's preferred versions, in order,
3. and on a tie, the copy added first.

The player can pick another fitting copy, both while installing and later.

## Warnings and waiting

None of these stop the install:

- A patch that names no version is most likely made for the version most
  mods use. If the chosen copy is a different one, the player is warned.
- A patch going onto a modified third-party build gets a warning.
- If no copy fits, the mod is installed anyway and waits for its game. As
  soon as a fitting copy is added, the mod moves onto it.

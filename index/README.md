# Mod index

The curated list of mods BloogBase offers under **Discover** (PLAN.md §10).
Every mod is one file in `mods/`. CI checks them, builds a single
`index.json`, signs it and publishes it on GitHub Pages; the app fetches it
from there, checks the signature and keeps a copy for offline use. New mods
reach every app without an app update. So do changes to an entry: mods
installed from it take its recipe (when their files have what it runs) and
its details (name, authors, description, links, cover) at the next index
check, for the download they are; a new download is offered as an update.

BloogBase never hosts mods: entries link to the original download, credit the
authors and link back to KeenWiki and PCKF.

The entries are licensed under [CC BY-SA 4.0](LICENSE); by adding or changing
one you agree to that. The mods they link to belong to their authors.

## Adding a mod

`pnpm index:add <KeenWiki link>` drafts the entry from the mod's page and
archive, with the parser and the detection the app uses: everything below but
the descriptions, `featured` and the special fields. It adds the archive to
the check's cache, runs the check on the new entry and then lists what's left
to fill in and to check by hand, above all the recipe and `type`. It saves the
mod's readmes to `.local/tmp/index-add/<id>/` for that. Options: `--id`,
`--game` (source mods, whose page names no game), `--download N` (not the
newest download), `--mods DIR`.

By hand:

1. Copy an existing file in `mods/`, name it `<id>.yaml`. The id is lower case
   with dashes and never changes once published: installed copies are tied to
   it.
2. Fill it in:

   | Field            | Meaning                                                                                     |
   | ---------------- | ------------------------------------------------------------------------------------------- |
   | `name`           | As the authors call it                                                                      |
   | `game`           | `keen1` … `keen6`, `kdreams`; for a standalone mod, the game it's built on                  |
   | `requires`       | Game versions it works with (ids from `src-tauri/catalog/games.json`); leave out for any    |
   | `authors`        | Names as on KeenWiki                                                                        |
   | `authorLinks`    | Name: page for authors whose name KeenWiki links (their `User:` page); leave out red links  |
   | `version`        | The mod's version, in quotes                                                                |
   | `released`       | First release, `YYYY-MM-DD`; `updated` for the latest one                                   |
   | `added`          | Today: when the entry joined the index (sorts "Newest" first)                               |
   | `type`           | `episode` (a new adventure), `levelpack` or `conversion` (plays unlike Keen, "Offbeat")     |
   | `status`         | `alpha`, `beta` or `abandoned`, from KeenWiki's "Status"; leave out when complete           |
   | `description`    | One or two sentences in your own words, in English                                          |
   | `translations`   | Language: texts in it; for now `de` with `description` (see below)                          |
   | `links`          | `keenwiki`, `pckf`, `keenmodding` (Keen: Modding forum), `homepage`                         |
   | `downloads`      | Newest first: `url` (the original source), `sha256`, `size` in bytes, `version`; see below  |
   | `recipe`         | How it starts: `patch` (`patcher`, `patch`, `args`), `program`, `batch` (`file`), `episode` |
   | `versionRecipes` | Version id: recipe, for game versions the mod starts differently on (one patch per version) |
   | `copyToRoot`     | Folders in the mod's root whose files the readme says to copy into the root                 |
   | `extends`        | Id of the entry this mod is copied over, e.g. a level pack for a mod; installed with it     |
   | `provides`       | Files of this mod that mods copied over it lack, e.g. its start batch (see below)           |
   | `cover`          | The program whose title screen is the cover, when the recipe runs a menu (see below)        |
   | `standalone`     | `true` for a mod that ships its whole game and plays without a copy of it (see below)       |
   | `worlds`         | For mods that generate their levels at every start: how to keep them as worlds (see below)  |
   | `root`           | Folder of the archive to install from, `""` for its root (see below)                        |
   | `saveDirs`       | Folders in the mod's root the game keeps saves and settings in (see below)                  |
   | `coverFrom`      | `dir` and `patch` whose title screen is the cover (see below)                               |
   | `variants`       | Ways to play the mod the player chooses from: difficulty, second part, … (see below)        |
   | `featured`       | `true` to suggest it in the setup wizard (keep it to a handful per game)                    |
   | `minAppVersion`  | Oldest BloogBase that can install it, if it needs something new                             |

   The app shows the description in the player's language when the entry
   has it, otherwise the English one:

   ```yaml
   description: "Keen 4 turned into a mix of pinball and snooker."
   translations:
     de:
       description: "Keen 4 als Mischung aus Flipper und Snooker."
   ```

   `sha256` and `size`: `certutil -hashfile <file> SHA256` (Windows) or
   `sha256sum <file>`; the size is in the file's properties. The recipe files
   must be in the same folder of the archive; the "Add mod" dialog of the app
   shows what it detects, which is usually right.

   `versionRecipes`, `copyToRoot` and `extends` need a `minAppVersion`;
   older apps would install the mod without them. `norp-the-yorp-3.yaml`
   uses the first two. With `extends`, the mod's files go over those of the
   entry it names, and its recipe may run that mod's files (e.g. its
   patcher). The entry must be for the same game and not extend another one.
   Packs like that usually need no entry: when the player adds one by hand,
   BloogBase finds the mod it belongs over and plays it there. It goes by
   the files the pack lacks: those its patch loads (Lego Levels lacks Lego
   Keen's `lego.tli`) or, for levels alone, the start its readme names
   (xky1rooms says to run Episode X's `XkyKeen1.bat`). List such files in
   the base mod's `provides`, only ones no other mod of the game has; the
   check makes sure the download has them. Older apps ignore `provides`.

   The cover is the title screen the mod's files hold, found through the
   program the recipe runs. When that is a menu starting several programs,
   name the one whose title is the cover in `cover` (Foray in the Forest:
   `FITF-X.EXE`); it has to be next to the recipe files. Older apps ignore
   it and show the game's title instead.

   A source mod ships its own executables and all its data, nothing of the
   game's (Foray in the Forest: `FITF.EXE` and its `*.CKS` files). With
   `standalone: true` BloogBase plays it without a copy of `game`, lists it
   under "Standalone" and suggests it in the setup wizard even to players
   without any Keen. `game` stays the game it's built on: its saves, settings
   and DOSBox defaults are that game's. The recipe has to run a `program` or
   `batch` file of the mod, without `requires`, `versionRecipes` or
   `extends`, and no other entry can extend it (that goes for every
   standalone mod). Mods added by hand can go over it, through its
   `provides` (Episode X for xky1rooms), and then need no copy either. Only entries can make a source mod standalone: added
   by hand, the same files play on a copy of `game`, unless the app knows
   them by their `programs` or `fingerprint` (below). Apps
   before 1.2.0 ignore the field and play the mod on that game, so an entry
   can take it up without a `minAppVersion`; mods installed from it leave
   their copy of the game at the next index check, with their saves.

   Every download of a standalone mod lists `programs`: the SHA-256 of each
   file its recipe runs and of the `cover`. By them the app knows the mod
   in a folder or ZIP the player already has, maybe an older version with
   saves, and installs those files with the entry's recipe; saves and
   settings go to the mod's profile, so updates keep them. Older releases
   are worth listing as further downloads for that. One that starts unlike
   the newest gets its own `recipe` (Operation Station Infiltration 1.0
   runs `osidos32.exe`). `pnpm index:add` fills in `programs`; the check
   says the right hash when one is wrong. Older apps ignore both fields.

   ```yaml
   downloads:
     - version: "1.4"
       url: https://files.shikadi.net/…/osi_v1.4.sc.2022-03-15.zip
       sha256: 0a9e…
       size: 3232443
       programs:
         OSI.EXE: 0c04…
     - version: "1.0"
       url: https://files.shikadi.net/…/osi_v1.0.sc.2022-12-25.zip
       sha256: 4c84…
       size: 4862807
       recipe: { type: program, program: osidos32.exe }
       programs:
         osidos32.exe: f8b4…
   ```

   Every download has a `fingerprint`: the files right in the mod's root
   that make the version (documents, archives, saves and settings aside)
   and one SHA-256 over them, a line `NAME sha256` per file, names upper
   case, sorted. By it the app knows the mod in a folder or ZIP the player
   has, unpacked or repacked, and among mods installed by hand, which then
   become the entry's. `pnpm index:add` fills it in; the check says the
   right SHA-256 when it's wrong. For the whole index,
   `BLOOGBASE_WRITE_FINGERPRINTS=1 cargo test --lib index_fingerprints -- --ignored`
   (in `src-tauri`, after `pnpm index:build` and with the downloads cached)
   writes them all. Older apps ignore it.

   ```yaml
   fingerprint:
     sha256: 3b47…
     files: [abiathar.pat, AUDIO.CK4, CK4PATCH.EXE, GAMEMAPS.CK4, play.bat]
   ```

   A mod that brings an executable and all the files of its game
   (`dataFiles` in `src-tauri/catalog/games.json`) reads nothing of the copy
   below it, so it plays on any version (decided 2026-11-14). It gets
   `standalone: true` too, but stays under its game in Discover, with its
   episode's cover and no "Source mod" badge, and lists no `programs`: it's
   known by its download. Its recipe may patch (Aurora Bliss), and it has no
   `requires`. Keen 1, Keen 4 and Keen Dreams mods like that then play
   without a copy of the game, as their shareware is free; those of Keen 2,
   3, 5 and 6 play on any copy (below). `pnpm index:add` drafts it, and
   `cargo test --lib index_entries_say -- --ignored` (in `src-tauri`, after
   `pnpm index:build` and `pnpm index:check`) checks that every entry says
   what its downloads are. One that ships the game's own executable
   unchanged needs `minAppVersion: 1.3.0`: older apps leave that out.

   Whether a standalone mod needs a copy of `game` isn't up to the entry:
   the app decides it, as a licence question (decided 2026-11-14). For a
   game with no free shareware (Keen 2, 3, 5 and 6) it does: the mod still
   plays from its own files alone, but only for players who own `game`; it's
   listed under that game, waits for a copy of it like any other mod and
   shows no "Source mod" badge (DarkAle's New Keen 5 Levelpack). Those of
   Keen 1, Keen 4 and Keen Dreams don't (Return to the Shadowlands). Apps
   before 1.3.0 read it from `requiresGame`, which the build adds to
   `index.json`; the check refuses it in an entry.

   A mod that generates its levels at every start (Keen 1 Randomiser,
   Simulator Keen) can keep each set as a world, so a player can go back to
   one, with its own saves, or start a particular seed. `new` runs the
   generator and plays (usually the same as `recipe`), `play` plays the
   world kept without generating anew; `files` are what the generator
   writes (DOS names, `*` and `?`), matched in the profile's state only. The
   files `new` and `play` run have to be in the download, but for those in
   `files`. With `seed`, the player can enter one (`arg` and the number are
   appended to `new`'s command, up to `max`), and `read` says where the
   generator notes the seed it used: the number right after `after` in
   `file`, one of `files`. `options` are offered in "Start new world": a
   `flag` passes `arg` when on, a `number` passes `arg n` (`min`, `max`,
   `default`); `label` in English and German. Older apps ignore the field
   and play `recipe`, so it needs no `minAppVersion`.

   ```yaml
   worlds:
     new: { type: batch, file: RNDKEEN1.BAT }
     play: { type: patch, patcher: CK1PATCH.EXE, patch: RNDKEEN1.PAT }
     files: [RNDLV*.CK1, RNDKEEN1.PAT]
     seed:
       arg: /SEED
       max: 2147483647
       read: { file: RNDKEEN1.PAT, after: "Seed: " }
     options:
       - id: startpogo
         arg: /STARTPOGO
         type: flag
         label: { en: "Start with the pogo stick", de: "Mit Pogo-Stick starten" }
   ```

   Some mods start from a launcher above the folders their parts run in
   (Revival of Bliss: `play.bat` runs `MKLAUNCH.EXE`, whose menu starts
   `KEEN\`, `CRAIG\` or `FUTURE\`). The app would install one of those
   folders on its own, so `root` names the folder of the archive to
   install from, `""` for the archive's root, with `/` between folders;
   the recipe files have to be right in it. Each part keeps its saves and
   settings in its own folder: list those in `saveDirs`, with the `name`
   the Saves tab shows them under (and a German one in `translations`, if
   it differs). Global settings go into each folder's settings file.
   `oldRoot: true` marks the one folder copies installed before the entry
   named its `root` played from: the app reinstalls them from their
   archive at the next index check and moves their saves into that folder.
   A mod with `worlds` can't have `saveDirs`. As the root holds no game,
   `coverFrom` names the folder (`dir`) and the `patch` the cover is the
   title screen of. All three need `minAppVersion: 1.4.0`.

   ```yaml
   root: ""
   recipe: { type: batch, file: play.bat }
   saveDirs:
     - dir: KEEN
       name: Keen
     - dir: CRAIG
       name: Craig
       oldRoot: true
     - dir: FUTURE
       name: Epilogue
       translations: { de: { name: Epilog } }
   coverFrom: { dir: CRAIG, patch: abiathar.pat }
   ```

   **Variants.** Some mods ship several ways to play them: difficulty
   levels in folders (Clouds Unseen `EASY/`, `HARD/`), one patch or batch
   file each (Episode Dash), files a setup batch renames (Shadow Keen 2),
   or further archives of the same version on KeenWiki (Clouds Unseen
   Verdant). Without `variants` the app installs one of them and nobody
   chooses. With them, the player picks one at the first start and can
   switch on the mod page; each keeps its own saves, settings stay shared.
   Each variant has:

   | Field         | Meaning                                                                               |
   | ------------- | ------------------------------------------------------------------------------------- |
   | `id`          | Lower case with dashes; never changes once published (saves are kept by it)           |
   | `default`     | `true` on exactly one: preselected, and what older apps install                       |
   | `name`        | In English; `translations.de.name` is required, `description` (both) optional         |
   | `archive`     | A key of the newest download's `parts`; left out: the download itself                 |
   | `root`        | Folder of its archive, `""` for its root; left out: the entry's `root`, else found    |
   | `recipe`      | In place of the entry's `recipe` and `versionRecipes`; never on the default           |
   | `copyToRoot`  | In place of the entry's: folders below the root whose files go into the root          |
   | `files`       | Name in the game: file below the root it's a copy of (`LEVEL80.CK2: Files/LE.COG`)    |
   | `cover`       | In place of the entry's `cover`                                                       |
   | `fingerprint` | As a download's, over the root as installed: `BLOOGBASE_WRITE_FINGERPRINTS` writes it |

   Further archives go into the newest download as `parts` (key: `url`,
   `sha256`, `size`); the app downloads them with it, and they share its
   version. Only the newest download has parts, and each part is used by a
   variant. The entry's `recipe` is the default's, so older apps, which
   ignore `variants` and `parts`, keep installing what they always did; no
   `minAppVersion` needed. A mod with variants can't have `worlds`,
   `extends`, `saveDirs` or `coverFrom`, no entry can extend it, and its
   downloads have no recipes of their own. Folders next to the root that a
   setup batch copies in: root `""` and `copyToRoot` with the shared
   folder first, then the variant's own (Shadow Keen 1 2004: `[sf, part1]`).

   ```yaml
   downloads:
     - url: https://files.shikadi.net/keenwiki/mods/Keen1/m.stupidbunny.clouds_unseen.k1.2010-05-20.zip
       sha256: …
       size: 333850
       fingerprint: { … } # the folder older apps install
       parts:
         verdant:
           url: https://files.shikadi.net/keenwiki/mods/Keen1/m.stupidbunny.clouds_unseen_verdant.k1.2010-05-20.zip
           sha256: …
           size: 137823
   variants:
     - id: hard
       default: true
       name: Hard
       translations: { de: { name: Schwer } }
       root: HARD
     - id: easy
       name: Easy
       translations: { de: { name: Leicht } }
       root: EASY
     - id: verdant
       name: Verdant
       description: Hard, with green skies.
       translations: { de: { name: Verdant, description: Schwer, mit grünem Himmel. } }
       archive: verdant
   ```

   Write the variants' fingerprints with `pnpm index:build`, then
   `BLOOGBASE_WRITE_FINGERPRINTS=1 cargo test index_fingerprints -- --ignored`
   in `src-tauri` (it reads the archives `pnpm index:check` cached).

3. Check it, including the download:

   ```sh
   pnpm index:check
   ```

4. Commit, or open a pull request. CI runs the same check.

To remove a mod (e.g. on the author's request), delete its file.

Discover marks a mod "New" for 30 days after an index update brought it to
the player's app; `added` doesn't decide it. The first index an app
downloads brings nothing new, since everything in it would be.

## Series

Mods that belong together are grouped in a series, one file per series in
`series/`, named `<id>.yaml`. The app shows the series above a part's title,
leads from one part to the next and has a page for the whole series. Only
complete series go in: every part has to be in the index (or a game of the
catalog), there are no placeholders for missing ones.

```yaml
name: "The Universe is Toast!"
description: "…" # optional, a sentence or two
translations: { de: { name: "…", description: "…" } } # optional
episodes: # the main parts, numbered in this order
  - mod: the-keys-of-krodacia
  - mod: dead-in-the-desert
    title: "Dead in the Desert" # optional
bonus: # optional: level packs and extras, without a number
  - mod: gridlock-s-dead-in-the-desert-levelpack
```

- A part names either `mod:` (an entry's id) or `game:` (a catalog game,
  e.g. `keen4`); the original episodes form series that way too.
- `title` is the episode's own title, for mods whose name starts with the
  series' ("Shadow Keen 1: A Shadow Over Dune" → "A Shadow Over Dune").
  Left out, the app shows the mod's name as it is.
- Episodes are numbered 1, 2, 3 …; `number: "0"` or `number: "3.5"` overrides
  one, and the next counts on from its whole part. Bonus parts have no
  number.
- The check refuses unknown ids, a mod or game in two series or twice in one,
  fewer than two episodes and a number used twice.

`index.json` lists them under `series`, sorted by id, with every episode's
number written out. Apps before series ignore the field.

## Trying the index locally

```sh
pnpm index:build     # writes index/dist/index.json + .minisig, signed with the dev key
pnpm tauri dev       # debug builds use index/dist/index.json when it exists
```

`BLOOGBASE_INDEX_URL` overrides the source (a URL or a local path to an
`index.json` with its `.minisig` next to it). Debug builds ignore
`minAppVersion`: they run the code of the next release, which entries
asking for it need.

## Signing

The index decides what the app downloads and runs, so the app only accepts it
with a valid [minisign](https://jedisct1.github.io/minisign/) signature from a
key listed in `PUBLIC_KEYS` in `src-tauri/src/mod_index.rs`. It also refuses
an index older than the one it has, so an old signed copy can't be replayed.

- Secret key: `.local/index-signing/minisign.key` (never committed).
- Public key: `.local/index-signing/minisign.pub`, embedded in the app.
- New key pair: `cargo run --manifest-path index/tool/Cargo.toml -- keygen --out <dir>`.
  When rotating, add the new public key to `PUBLIC_KEYS`, ship an app update,
  and only later sign with the new key and remove the old one.

## Publishing (not enabled yet)

The `publish` job in `.github/workflows/index.yml` is written but only runs
once these are done:

1. Make the repository public (Pages needs that on the free plan).
2. Settings → Pages → Source: **GitHub Actions**.
3. Settings → Secrets and variables → Actions:
   - Secret `INDEX_MINISIGN_KEY`: the full contents of the secret key file.
   - Variable `INDEX_PUBLISH`: `true`.
4. Run the "Mod index and website" workflow (or push a change to `index/`).

The index is then at
`https://transporthummel.github.io/BloogBase/index/index.json`, the address
built into the app. The same job publishes the project website
([docs/website.md](../docs/website.md)) at the root of that Pages site.

## The tool

`index/tool` (`bbindex`) does the checking, building and signing; see
`src/main.rs` for its options. `cargo test` there also keeps
`src-tauri/tests/index-sample.json`, which the app's tests parse, in sync with
what the tool writes.

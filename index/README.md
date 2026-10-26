# Mod index

The curated list of mods BloogBase offers under **Discover** (PLAN.md §10).
Every mod is one file in `mods/`. CI checks them, builds a single
`index.json`, signs it and publishes it on GitHub Pages; the app fetches it
from there, checks the signature and keeps a copy for offline use. New mods
reach every app without an app update.

BloogBase never hosts mods: entries link to the original download, credit the
authors and link back to KeenWiki and PCKF.

## Adding a mod

1. Copy an existing file in `mods/`, name it `<id>.yaml`. The id is lower case
   with dashes and never changes once published: installed copies are tied to
   it.
2. Fill it in:

   | Field            | Meaning                                                                                     |
   | ---------------- | ------------------------------------------------------------------------------------------- |
   | `name`           | As the authors call it                                                                      |
   | `game`           | `keen1` … `keen6`, `kdreams`                                                                |
   | `requires`       | Game versions it works with (ids from `src-tauri/catalog/games.json`); leave out for any    |
   | `authors`        | Names as on KeenWiki                                                                        |
   | `authorLinks`    | Name: page for authors whose name KeenWiki links (their `User:` page); leave out red links  |
   | `version`        | The mod's version, in quotes                                                                |
   | `released`       | First release, `YYYY-MM-DD`; `updated` for the latest one                                   |
   | `added`          | Today: when the entry joined the index (drives the "New" badge)                             |
   | `type`           | `episode` (a new adventure), `levelpack` or `conversion` (something else entirely)          |
   | `description`    | One or two sentences in your own words                                                      |
   | `links`          | `keenwiki`, `pckf`, `keenmodding` (Keen: Modding forum), `homepage`                         |
   | `downloads`      | Newest first: `url` (the original source), `sha256`, `size` in bytes, `version`             |
   | `recipe`         | How it starts: `patch` (`patcher`, `patch`, `args`), `program`, `batch` (`file`), `episode` |
   | `versionRecipes` | Version id: recipe, for game versions the mod starts differently on (one patch per version) |
   | `copyToRoot`     | Folders in the mod's root whose files the readme says to copy into the root                 |
   | `extends`        | Id of the entry this mod is copied over, e.g. a level pack for a mod; installed with it     |
   | `provides`       | Files of this mod that mods copied over it lack, e.g. its start batch (see below)           |
   | `featured`       | `true` to suggest it in the setup wizard (keep it to a handful per game)                    |
   | `minAppVersion`  | Oldest BloogBase that can install it, if it needs something new                             |

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

3. Check it, including the download:

   ```sh
   pnpm index:check
   ```

4. Commit, or open a pull request. CI runs the same check.

To remove a mod (e.g. on the author's request), delete its file.

## Trying the index locally

```sh
pnpm index:build     # writes index/dist/index.json + .minisig, signed with the dev key
pnpm tauri dev       # debug builds use index/dist/index.json when it exists
```

`BLOOGBASE_INDEX_URL` overrides the source (a URL or a local path to an
`index.json` with its `.minisig` next to it).

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
4. Run the "Mod index" workflow (or push a change to `index/`).

The index is then at
`https://transporthummel.github.io/BloogBase/index/index.json`, the address
built into the app.

## The tool

`index/tool` (`bbindex`) does the checking, building and signing; see
`src/main.rs` for its options. `cargo test` there also keeps
`src-tauri/tests/index-sample.json`, which the app's tests parse, in sync with
what the tool writes.

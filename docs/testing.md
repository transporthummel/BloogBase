# Testing

Commander Keen games and mods are copyrighted, so none of them are in the
repository. Tests come in three layers.

## 1. Unit tests (CI, every push)

```sh
pnpm test            # frontend (Vitest)
cd src-tauri && cargo test
```

They use small synthetic fixtures and need nothing else.

The KeenWiki parser (`src-tauri/src/keenwiki.rs`) is tested against saved
pages in `src-tauri/tests/keenwiki/` (fetched 2026-10-17: several mod pages, a
game page and a missing page). When the wiki markup changes, save the page
again with an honest `User-Agent` (one request per page) and update the
expectations.

The saved game parser is its own crate (`src-tauri/keen-saves`, part of the
workspace, so plain `cargo test` runs it). Its fixtures in
`keen-saves/tests/fixtures/` are real saves made by playing: Keen 1-3 whole
(92 bytes), Keen 4, Keen 6 and Keen Dreams cut right after the game state, so
no level data from the games is in the repository. Keen 5 is tested with a
synthetic save until someone contributes a real one.

## 2. Mod corpus (CI weekly, or locally)

`src-tauri/tests/mod-corpus.json` lists real mods: their download URL on the
KeenWiki file server, SHA-256 and what BloogBase should detect (game, name,
start recipe, game versions). The test downloads missing archives once into a
cache and compares.

```sh
cd src-tauri
cargo test corpus -- --ignored --nocapture
```

- Cache: `BLOOGBASE_MOD_CACHE`, default `.local/mods`.
- Adding a mod: append `{ "url": "…" }` to the manifest, put the archive into
  the cache, then run with `BLOOGBASE_CORPUS_UPDATE=1` to fill in the hash and
  expectations. Review the diff before committing.
- After a change to the heuristics, a failing entry means a real mod is now
  detected differently: fix the code or, if the new result is better, update
  the manifest the same way.
- CI (`.github/workflows/mod-corpus.yml`) runs it weekly and whenever the
  manifest changes, with the archives cached so the community's file server
  isn't hit on every run.

## 3. DOSBox end-to-end (local, before a release)

Launches every mod in DOSBox on every game version it fits, keeps it running
for a few seconds and saves a screenshot of the DOSBox window. Windows only
for now.

Folder layout (both ignored by git):

```
.local/games/<any name>/   one folder per game copy (folders with several
                           episodes in subfolders work too)
.local/mods/*.zip          the mod archives (the corpus cache)
```

```sh
cd src-tauri
BLOOGBASE_E2E_DOSBOX=<path to dosbox.exe> \
  cargo test e2e_mods -- --ignored --nocapture --test-threads=1
```

Screenshots land in `src-tauri/target/e2e-mods/shots/`. Look at them: "running"
only means DOSBox was still open after seven seconds. A mod patched onto the
wrong game version can still be running, just showing garbage.

## Mod index

The index (`index/`, see `index/README.md`) has its own checks:

```sh
pnpm index:check                       # entries + downloads (cached in .local/mods/index-cache)
cd index/tool && cargo test            # the tool itself
```

CI (`.github/workflows/index.yml`) runs both on every change to `index/` and
weekly, then also asking the servers whether each download still exists. The
app side (signature, cache, ETag, install from the index) is covered by the
unit tests in `src-tauri/src/mod_index.rs` and `src-tauri/src/mods.rs`.

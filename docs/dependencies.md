# Dependencies

What BloogBase pulls in from elsewhere, where each piece is pinned, and how
to bring everything up to date.

## Overview

| Source             | Declared in                                                       | Locked in                                 | Updated by                 |
| ------------------ | ----------------------------------------------------------------- | ----------------------------------------- | -------------------------- |
| Rust crates (app)  | `src-tauri/Cargo.toml` and its `keen-*/` crates                   | `src-tauri/Cargo.lock`                    | Dependabot, `cargo update` |
| Rust crates (tool) | `index/tool/Cargo.toml`                                           | `index/tool/Cargo.lock`                   | Dependabot, `cargo update` |
| npm packages       | `package.json`                                                    | `pnpm-lock.yaml`                          | Dependabot, `pnpm update`  |
| DOSBox Staging     | `src-tauri/runtimes/dosbox-staging.json`                          | same file (SHA-256 and size per platform) | by hand                    |
| Free game versions | `src-tauri/catalog/free-games.json`                               | same file (SHA-256 per mirror)            | by hand, rarely            |
| GitHub Actions     | `.github/workflows/*.yml`                                         | commit hash per action (`@<sha> # v4`)    | Dependabot                 |
| Toolchains         | Rust `stable`, Node 24, pnpm (`packageManager` in `package.json`) | —                                         | by hand (Node, pnpm)       |

Rust has no `rust-toolchain.toml`: CI and local builds use the latest
stable each.

`src-tauri/assets/` also holds CKPatch, KDrPatch and CWSDPMI, unmodified.
They are old DOS programs that don't change any more; nothing to update.

### Rust

`src-tauri/` is a Cargo workspace: the app plus `keen-saves` (reading saved
games) and `keen-graphics` (title screens for the covers). The main crates:

- `tauri` 2 with its plugins (`opener`, `dialog`, `log`, `updater`,
  `window-state`) and `tauri-build`.
- `tauri-specta`, `specta`, `specta-typescript`: generate
  `src/bindings.ts` from the commands. They are pinned to one release
  candidate (`=2.0.0-rc.…`) and only move together.
- `rusqlite` (with SQLite `bundled`, so the app brings its own SQLite) and
  `rusqlite_migration`.
- `reqwest` with `rustls` (no system OpenSSL), `tokio`.
- Archives and hashes: `zip`, `tar`, `lzma-rs`, `flate2`, `cfb`, `sha2`,
  `minisign-verify`.
- `scraper` (KeenWiki pages), `png`, `walkdir`, `serde`, `thiserror`,
  `tracing`.
- Windows only: `windows-sys`, `windows-core`, `webview2-com`.

`cargo tree` in `src-tauri/` shows the full tree, `cargo tree -d` the crates
that are in it in more than one version.

The index tool (`index/tool/`, `bbindex`) checks, builds and signs the mod
list. It doesn't ship in the app.

### npm

`package.json` lists exact versions. `pnpm-workspace.yaml` sets
`minimumReleaseAge` to 7 days: pnpm doesn't install a release younger than
that. The frontend runs on React, TanStack Router and Query, i18next,
CodeMirror (DOSBox settings editor), Motion and the `@tauri-apps/*`
packages. Build tools (Vite, TypeScript, ESLint, Vitest, Tailwind, the Tauri
CLI) are dev dependencies and don't ship.

The `@tauri-apps/*` packages belong to the Rust crates of the same name
(`@tauri-apps/api` to `tauri`, `@tauri-apps/plugin-dialog` to
`tauri-plugin-dialog`, …). Keep each pair on the same minor version; the
Tauri CLI refuses to build when they drift apart.

### DOSBox Staging

`src-tauri/runtimes/dosbox-staging.json` pins one version of DOSBox Staging
with a download (URL, SHA-256, size) per platform. Details in
[dosbox.md](dosbox.md).

### GitHub Actions

Every action is pinned to a commit, with its tag as a comment, so a moved
tag can't run foreign code (more in [releasing.md](releasing.md) and
[security.md](security.md)). Dependabot updates hash and comment together.

## Automatic checks

- **Dependabot** (`.github/dependabot.yml`) opens pull requests every Monday
  for Cargo, npm and GitHub Actions. A release has to be 7 days old first.
  Minor and patch updates come as one pull request per ecosystem, Tauri and
  specta in groups of their own; each major update gets its own pull
  request.
- **Dependabot security updates** (repository settings → Code security)
  open pull requests for known vulnerabilities right away. They are switched
  on in the settings, not in the file.
- **Audit** (`.github/workflows/audit.yml`) checks both lockfiles and the
  production npm packages against known vulnerabilities, weekly and on every
  lockfile change (see [testing.md](testing.md#dependency-audit)).

DOSBox Staging, the free game versions and Node are not watched by anything;
look at them before a release.

## Bringing everything up to date

### Dependabot pull requests

1. Read the pull request: the release notes and the changelog Dependabot
   quotes, especially for major updates.
2. Wait for CI. `ci.yml` checks pushes and pull requests on Ubuntu only and
   doesn't build the app; for Tauri, WebView or Windows-specific crates,
   check out the branch and run step 5 below locally.
3. If a Rust Tauri crate and its `@tauri-apps/*` package come in separate
   pull requests, merge them together.
4. Merge. Add the changelog line (step 8 below) in the next commit on
   `main`.

### By hand

All commands from the repository root unless noted.

1. **See what's new.**

   ```sh
   pnpm outdated
   cargo install cargo-outdated cargo-edit    # once
   cargo outdated --root-deps-only --manifest-path src-tauri/Cargo.toml
   cargo outdated --root-deps-only --manifest-path index/tool/Cargo.toml
   ```

2. **Toolchains.** `rustup update` (CI always uses the latest stable, so
   local should too). Node: stay on the version in the workflows
   (`node-version: 24`) and change it everywhere at once. pnpm:
   `corepack use pnpm@latest` updates `packageManager` in `package.json`.

3. **Minor and patch updates**, within the versions `Cargo.toml` and
   `package.json` allow:

   ```sh
   pnpm update
   cd src-tauri && cargo update && cd ..
   cd index/tool && cargo update && cd ../..
   ```

4. **Major updates**, one at a time, each with its release notes read:

   ```sh
   pnpm add <package>@latest          # or: pnpm add -D <package>@latest
   cd src-tauri && cargo upgrade --incompatible -p <crate> && cd ..
   ```

   For specta, raise `tauri-specta`, `specta` and `specta-typescript` in one
   go to versions that belong together, then regenerate `src/bindings.ts`
   with `cargo test --lib export_bindings` in `src-tauri/` and commit it
   too (CI fails when it is stale). For Tauri,
   raise each crate together with its `@tauri-apps/*` package.

5. **Check.**

   ```sh
   pnpm test:all
   cd src-tauri && cargo clippy --all-targets && cd ..
   pnpm tauri build --no-bundle
   ```

   Then start the app and play a game and a mod.

6. **Audit.** The commands in
   [testing.md](testing.md#dependency-audit). A warning about an
   unmaintained crate is no blocker, but worth a look.

7. **DOSBox Staging, if there's a new release.** Only with a minor release of
   BloogBase ([releasing.md](releasing.md#version-numbers)):
   - In `src-tauri/runtimes/dosbox-staging.json`, set `version` and, per
     platform, `url`, `sha256` and `size` from the GitHub release assets.
   - Redo the default bindings in `src-tauri/src/keymap.rs` for the new
     version.
   - Run the local DOSBox tests
     ([testing.md](testing.md#4-local-tests-with-dosbox-and-downloads-before-a-release)).

8. **Free game versions.** `cargo test free_games -- --ignored` in
   `src-tauri/` downloads every mirror and checks its hash. Replace a dead
   mirror in `src-tauri/catalog/free-games.json`.

9. **Changelog.** One line under Misc in `CHANGELOG.md`, "Updated
   dependencies" (name Tauri or DOSBox Staging when they moved). A new DOSBox
   version also goes into `src/updates/changelog.json`, as players notice
   it. Updated dependencies alone make a patch release
   ([releasing.md](releasing.md#version-numbers)).

10. **Commit** the manifests and both lockfiles together.

`public/third-party-licenses.txt` doesn't need updating by hand: the release
workflow writes it before building.

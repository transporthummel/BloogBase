<p align="center"><img src="docs/logo.png" alt="BloogBase" width="640"></p>

**BloogBase** is a cozy home for Commander Keen: a desktop launcher for
Windows, macOS and Linux that lets anyone play the Keen games and the
community's mods without unzipping, copying files or editing configs.

- A setup wizard downloads DOSBox Staging and the free shareware episodes
  (Keen 1, Keen 4, Keen Dreams) and finds games you own on Steam or GOG.
- Install mods with one click from a curated list, from a pasted KeenWiki
  link, or from a ZIP or folder.
- Every game and mod gets its own play profile: saves are kept apart, backed
  up automatically and can be restored; the original game files are never
  changed.
- Play time, save details, screenshots, videos and per-game DOSBox settings.
- Optional one-button firing in Keen 1-3: Space shoots instead of Ctrl and Alt.

## Installing

<!-- Keep in step with .github/release-downloads.md, shown on every release. -->

Download the newest release for your system from the
[releases page](https://github.com/transporthummel/BloogBase/releases).
BloogBase is a free fan project, so its installers aren't signed with paid
certificates; your system will ask once before the first start:

- **Windows:** `BloogBase_…_x64-setup.exe`. If "Windows protected your PC"
  appears, click _More info_ → _Run anyway_.
- **macOS:** `BloogBase_…_aarch64.dmg` (Apple Silicon) or `…_x64.dmg` (Intel).
  Move BloogBase to Applications and start it. When macOS blocks it, open
  _System Settings → Privacy & Security_ and click _Open Anyway_.
- **Linux:** `BloogBase_…_amd64.AppImage` (make it executable and start it) or
  the `.deb` for Debian/Ubuntu.

After that, BloogBase keeps itself up to date.

## Development

Requirements: Node.js 24, pnpm, Rust (stable) and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
pnpm install
pnpm tauri dev
```

Checks: `pnpm lint`, `pnpm format:check`, `pnpm typecheck`, `pnpm test`, and
in `src-tauri/` `cargo fmt --check`, `cargo clippy --all-targets` and
`cargo test`. `pnpm test:all` runs all of them plus the tests with real games
and mods. See [docs/testing.md](docs/testing.md) for those and
[docs/releasing.md](docs/releasing.md) for releases. How BloogBase works out
how a mod starts is in [docs/recipes.md](docs/recipes.md), how it draws
covers in [docs/covers.md](docs/covers.md). The mod list lives in
[index/](index/README.md).

## License

BloogBase is released under the [MIT license](LICENSE). Games run in
[DOSBox Staging](https://www.dosbox-staging.org/) (GPL-2.0-or-later), which
BloogBase downloads from its official releases.

Commander Keen is a trademark of id Software. BloogBase is an unofficial fan
project and is not affiliated with or endorsed by id Software, ZeniMax Media,
Apogee or Softdisk. It contains no game data.

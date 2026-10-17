<p align="center"><img src="docs/logo.png" alt="BloogBase" width="640"></p>

**BloogBase** is a friendly home for Commander Keen: a desktop launcher for
Windows, macOS and Linux that lets anyone play the Keen games and the
community's mods without unzipping, copying files or editing configs.

- A setup wizard downloads DOSBox Staging and the free shareware episodes
  (Keen 1, Keen 4, Keen Dreams) and finds games you own on Steam or GOG.
- Install mods with one click from a curated list, from a pasted KeenWiki
  link, or from a ZIP or folder.
- Every game and mod gets its own play profile: saves are kept apart, backed
  up automatically and can be restored; the original game files are never
  changed.
- Play time, save details, screenshots and per-game DOSBox settings.

## Development

Requirements: Node.js 24, pnpm, Rust (stable) and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
pnpm install
pnpm tauri dev
```

Checks: `pnpm lint`, `pnpm typecheck`, `pnpm test`, and in `src-tauri/`
`cargo clippy --all-targets` and `cargo test`. See
[docs/testing.md](docs/testing.md) for the end-to-end tests and
[docs/releasing.md](docs/releasing.md) for releases. The mod list lives in
[index/](index/README.md).

## License

BloogBase is released under the [MIT license](LICENSE). Games run in
[DOSBox Staging](https://www.dosbox-staging.org/) (GPL-2.0-or-later), which
BloogBase downloads from its official releases.

Commander Keen is a trademark of id Software. BloogBase is an unofficial fan
project and is not affiliated with or endorsed by id Software, ZeniMax Media,
Apogee or Softdisk. It contains no game data.

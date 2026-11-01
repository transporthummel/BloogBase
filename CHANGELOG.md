# Changelog

## Unreleased

### Features

- The page of a game, shareware episode or demo can open the folder with its files, as a mod's page opens the mod folder.
- Games, shareware episodes and demos show their own documents in a README tab, as mods do, the games' own documents included.
- "Mod won't start?": when a mod's first start with a new setup (base game, mod files or recipe) ends with a message or within 30 seconds, BloogBase offers to report it as a GitHub issue, with the details it needs; the mod's More menu offers it anytime.
- A Controls tab on every game and mod page shows the keys of its series as keycaps, following one-button firing, with the known cheat codes folded away; the first start of each series shows it before the game starts.

### Improvements

- A Content Security Policy keeps injected scripts from running, reaching the app's commands or loading anything from the internet; blocked sources are logged.
- The DOSBox settings editor loads only when it is first opened, which makes the app script loaded at startup about a third smaller.

### Bug Fixes

- Setting up a mod from a KeenWiki page shows the release date from the page, the one it is saved with, instead of one guessed from the files.

### Misc

- Updated dependencies, among them Tauri 2.12.5.

## 1.0.0 (2026-10-31)

### Features

- The first release of BloogBase.

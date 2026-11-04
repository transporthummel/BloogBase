# Changelog

## Unreleased

### Features

### Improvements

- The Windows installer updates an older BloogBase in place, as the in-app updater does, instead of offering to uninstall it first.
- Download sizes under 1 MB are shown in KB instead of as 0.4 MB.
- Download sizes use the decimal comma in German (2,0 MB).
- The Windows uninstaller has the BloogBase icon instead of the NSIS default.
- An older mod list from the server gets its own message instead of the one for a damaged download.
- The library shows its games sooner after a start: looking for the names a mod's programs save under no longer scans them byte by byte.

### Bug Fixes

### Misc

- A debug build takes its local mod list even when the cached one from the website is newer.
- Debug builds keep their data in `.local/dev-data`, apart from an installed BloogBase.

## 1.1.0 (2026-11-03)

### Features

- A Controls tab on every game and mod page shows the keys of its series as keycaps, following one-button firing, with the known cheat codes folded away; the first start of each series shows it before the game starts.
- The library has a search field, as Discover has: it filters the cards by name, game and version.
- The page of a game, shareware episode or demo can open the folder with its files, as a mod's page opens the mod folder.
- Games, shareware episodes and demos show their own documents in a README tab, as mods do, the games' own documents included.
- "Mod won't start?": when a mod's first start with a new setup (base game, mod files or recipe) ends with a message or within 30 seconds, BloogBase offers to report it as a GitHub issue, with the details it needs; the mod's More menu offers it anytime.

### Improvements

- New logo and app icon (a floppy disk with a planet and a rocket) in the app, its window and taskbar icons, the installers, the README and the website with its press kit; game windows get a matching floppy with a DOS prompt, and the logo follows the theme.
- The start screen shows the website's starry sky, with two shooting stars.
- Saves of Foray in the Forest show their details (name, score, lives, shots, levels done, difficulty including Expert).
- Source mods that bring all their own files and nothing of the game's, like Foray in the Forest, install from the mod list or a KeenWiki page instead of ending with "No Commander Keen mod found there".
- One-button firing in Keen 1–3 is on unless the player has switched it off on the settings page.
- Games start in fullscreen unless the player has switched it off on the settings page.
- Saves that share a slot show their file names, so a mod's two kinds of saves (Foray in the Forest's `.CKS` and `.CKC`) can be told apart.
- The About page links the BloogBase website.
- A Content Security Policy keeps injected scripts from running, reaching the app's commands or loading anything from the internet; blocked sources are logged.
- The DOSBox settings editor loads only when it is first opened, which makes the app script loaded at startup about a third smaller.
- When a package manager installed BloogBase (the AUR package), the app leaves updates to it instead of offering its own.
- "What's new" lists new mods and mod updates before the release notes, and its Close button stays in view while the dialog scrolls.
- After the setup wizard chose another data folder, the next start removes the default one the first start made, as long as nothing of the player's is in it; on Windows and macOS the app's own settings stay there.

### Bug Fixes

- Setting up a mod from a KeenWiki page shows the release date from the page, the one it is saved with, instead of one guessed from the files.
- Source mods that open their graphics under their own names, like Foray in the Forest, show their own title screen as cover, not the one of the game below; where the recipe runs a menu, the index entry names the program with the title (`cover`).
- Saves of source mods that store them under their own extension, like Foray in the Forest (`SAVEGAM0.CKS`, `.CKC`), show up in the Saves tab and get backups.
- A data folder restored from a backup or moved elsewhere finds its games, mods, saves, backups and DOSBox again: paths inside it are stored relative to it, and the absolute ones of earlier versions are rewritten on start.

### Misc

- Updated dependencies, among them Tauri 2.12.5.
- The Windows download is the installer in a ZIP, so Edge no longer hides it behind its "isn't commonly downloaded" warning; only SmartScreen asks before the first start.

## 1.0.0 (2026-10-31)

### Features

- The first release of BloogBase.

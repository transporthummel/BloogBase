# Changelog

## Unreleased

### Features

- Sound, music, the sound device, the score box and keys can be set once for all games and mods in the settings.
- BloogBase can go fullscreen, without the taskbar, from a button in the sidebar or with F11, and can start in fullscreen; while it is in fullscreen, games start in fullscreen too, whatever their settings say. Settings has one choice for what starts in fullscreen: off, games only (the default), or BloogBase and games.
- A "Quit BloogBase" button at the bottom of the sidebar closes the app, also in fullscreen, and asks first while games run, as the window's close button does.
- BloogBase knows 50 releases now, eight more: Keen 2 and 3 v1.1 and v1.32, the Keen 6 Apogee demo and id Anthology release, and Keen Dreams v1.05 and v1.20; mods get placed on them by the same ranking as on the others.
- Mods that bring their whole game with them (source mods such as Foray in the Forest) can be marked standalone in the mod list: they play without a copy of the game they're built on, show up under "No game needed" in Discover and in the library, and the setup wizard suggests them even without any Keen; installed ones leave their copy of the game at the next check of the mod list, saves included. `pnpm index:add` marks source mods standalone.
- Custom patches: patch file lines for a game copy, entered with line numbers in the new "DOSBox & patches" tab of its page, folded away below the DOSBox settings until there are some, go into the game and every mod played on that copy at each start, after the mod's own lines; a mod's tab says whether it gets them.
- A new setting for all games says what a game does while another window is active: keep running (as before), mute, or pause (DOSBox's `mute_when_inactive` and `pause_when_inactive`).
- A "Legacy PC mode" setting for all games (off by default) makes the hard disk slow and audible, as on a PC of the time (DOSBox's `hard_disk_speed = slow` and `hard_disk_noise = on`).

### Improvements

- The Windows installer updates an older BloogBase in place, as the in-app updater does, instead of offering to uninstall it first.
- Download sizes under 1 MB are shown in KB instead of as 0.4 MB.
- Download sizes use the decimal comma in German (2,0 MB).
- The Windows uninstaller has the BloogBase icon instead of the NSIS default.
- An older mod list from the server gets its own message instead of the one for a damaged download.
- The library shows its games sooner after a start: looking for the names a mod's programs save under no longer scans them byte by byte.
- The library remembers over restarts which names a mod saves under, so its games show right away after a start.
- The library shows a hopping Bloog with "Loading your library…" until its games are there, instead of an empty page.
- On Windows the title bar wears BloogBase's theme (Midnight Galaxy or Vintage Beige) instead of following Windows' light or dark mode and accent colour; Windows 10 gets light or dark only.
- A player's own DOSBox Staging has to be 0.82.0 or newer, since older versions don't know the setting for the game speed; 0.82.x gets a notice that the CRT switch has no effect there.
- Keen Dreams v1.0 and v1.05 get their title screen as cover instead of the placeholder; these releases draw it from a map rather than keep a picture.
- The library keeps its search when you switch to another page and back, until the app closes or a new game or mod is added.
- Discover's mods tab keeps its search, filters and sort when you switch to another page and back, until the app closes; the news dialog's link to the new mods starts without filters.
- The save list shows the details of Atroxian Realm saves (name, score, lives, ammo, difficulty, where it was saved).
- Return to the Shadowlands saves show their real score, lives, ammo, difficulty and levels done instead of numbers read from the wrong place.
- The settings page is split into Appearance, Display, Sound and background, Controls, Backups, Game engine and BloogBase, with a list that jumps to each and marks the one in view, beside the cards on wide windows and as a bar above them on narrow ones; keys come first under Controls, with how many are set, and "applies from the next start" is said once per card instead of under each setting.
- "What DOSBox gets" on a game's page names the settings page as the source of what it sets ("Settings page") instead of "Display options", since it sets sound, keys and more now.
- On a game's "DOSBox & patches" tab, "What DOSBox gets", BloogBase's defaults and custom patches fold away the same way under their headings; the first two start folded.
- The "Mod won't start?" and bug report dialogs say in two steps what to do, including that GitHub needs a free account, and fold the report away behind "Show the report".

### Bug Fixes

- A comma after a download on a KeenWiki page no longer shows up as the version's note, as in "v0.2 (demo) (,)".
- Versions a KeenWiki page lists inside a paragraph show up in the download choice too; Atroxian Realm's v1.3, v1.1 and v1.0 were missing.
- Source mods whose cutscenes are as colorful as their title, like Atroxian Realm, get the title screen as cover: BloogBase takes the full-screen picture the mod's program draws right after loading it.
- Switching a running game to fullscreen with Alt+Enter no longer garbles the picture or breaks the game: DOSBox now uses borderless fullscreen by default (`fullscreen_mode = forced-borderless`).

### Misc

- `pnpm index:add <KeenWiki link>` drafts a mod index entry from the mod's page and archive and lists what's left to do by hand.
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

# Changelog

## Unreleased

### Features

- The saves tab imports saves from a folder, such as an old copy of the game or a mod: the files lying in it that look like the game's saves, settings or high scores are restored from there, all or one into a chosen slot, without becoming a backup of their own.
- A game's page shows, like a mod's, a description in English and German, "by id Software" with a link to their website and the release date of its version (from KeenWiki); demos and betas have descriptions of their own.
- Discover can hide installed mods. A mod installed while the filter is on stays in the list until the filter is turned on again or the page is opened again.
- Mods that generate their levels at every start (Keen 1 Randomiser, Simulator Keen) are marked "Random worlds" and keep each set as a world with its own saves: "Continue world" plays the last one again, the arrow beside it offers "New world" (a seed and the randomiser's options; a seed and options already played continue that world) and up to five favourite or recent worlds, and the new Worlds tab lists every world with its seed, options and saves, to play again, make current without playing, star, rename, copy the seed or delete. Saves from before worlds go into a backup.
- Mods with several ways to play, such as difficulty levels, a second part or other graphics, install all of them and are marked "N variants" on their library card and page. The first Play asks which one, with the default marked as such and (or for a mod installed before, the one played so far) preselected; after that Play starts the one chosen, the arrow beside it the others, and the new Variants tab shows each with its cover, description, saves and last start. Each variant keeps its own saves, settings stay shared, and the library card and page show the cover and name of the one played, the DOSBox window and the screen shown while it starts its name in brackets after the mod's. The index lists them in `variants`, with further archives of a version in `parts`; installed mods get them at the next index check.

### Improvements

- Restoring saves (from a backup, an earlier version or a folder) makes the automatic backup only if there are saves to replace; restoring settings and high scores only if there are settings or high scores. The message says whether the previous state was backed up.
- Discover's filters are buttons instead of drop-downs: the games as the tiles of the cards and the types as chips, several of each at once, "I can play" (with a tooltip saying what it means) and "Not installed" as toggles, with "Clear filters" next to the count whenever a filter is set. Only the sort is still a drop-down, next to the search.
- "Runs on Keen 4 v1.4 EGA" under a mod, in the library and on its page, links the game's page, as the mod it's played over already was.
- The name of the variant played on a library card links to it in the mod's Variants tab, as it does on the mod's page.
- Library cards of mods with variants or random worlds have the arrow beside Play too, with the other variants, a new world and the recent worlds, as on the mod's page.
- The "DOSBox & patches" tab of a game's or mod's page is called "Settings".
- The tabs of a game's or mod's page stay in one line in a narrow window: those that don't fit go into a "More" menu at the end, and the one picked there takes its place in the line.
- A folder or ZIP holding a mod and its game offers to add the game only while the library has no copy of that game; another release of it is enough for the mod.
- Removing a mod or game says how many screenshots and videos go with it (with its mods, theirs too), as no backup keeps them, and can copy them to a folder first, one folder per game or mod.
- The Screenshots and Videos tabs can copy them to a folder, into a folder named after the game or mod.
- A mod from the mod list is recognised unpacked too, in a folder of its own or the game's, repacked or as an older version, by a fingerprint of its files (levels, graphics, sounds, patches, programs) that every index download now carries. It installs from the player's files with the entry's tested setup, saves included. Mods in the library that were added by hand or from KeenWiki become the entry's the same way, at every start and index check, with its name and details, unless the entry is installed already. The index tool drafts the fingerprint and checks it against the download.
- Discover, the library and a mod's page mark mods whose KeenWiki page calls them Alpha, Beta or Abandoned, abandoned ones in yellow. The index has a `status` field for it, which `index-add` fills in from the page; ten mods, Episode 59: Underworld Ultimate and Keenball among them, are marked Beta. Mods installed from their KeenWiki page keep its status, and checking for an update takes over a new one.
- A folder or ZIP the mod list knows installs as that entry; the add dialog no longer offers "Set it up myself instead", which made a copy without its tested setup and updates (a second one if the mod was installed already).
- A mod from the mod list shows its update on its page by itself, with an "Update" button, as the list is checked at every start and every few hours. "Check for updates" is left only for mods from KeenWiki; it no longer offers a KeenWiki download for mods from the list, which took them out of the list with an untested setup.
- Mods from the mod list can no longer be replaced with a local copy, and adding another version of one (same name or KeenWiki page) installs it alongside instead of offering to replace it. They update from the list; an old beta goes beside them with its own saves.
- Text fields no longer check spelling, so names, paths, seeds and Keen terms aren't underlined in red.
- Revival of Bliss starts its own menu: play as Keen or Craig, and the password-locked epilogue. Each keeps its saves and settings in its own folder; the Saves tab groups them under "Keen", "Craig" and "Epilogue", global settings reach all three, and the cover is Craig's title screen. Copies installed before move over at the next index check, their saves into Craig's folder. The index has the fields `root`, `saveDirs` and `coverFrom` for mods like it.
- Mods from the mod list follow their entry at every start and index check, without an update: a fixed name, description, authors, links or cover, and a new way to start them (when their files have what it runs, and not while they run). Commander Cool 1 thus starts without the "Use 2-Button Firing?" question, and Yorphius II Deluxe is called Yorphius II.
- Typing in the library's search keeps up in a big library: the cards are filtered just behind the keys, and only the cards that change are redrawn.
- The menu beside Play on a game's or mod's page is called "Options" instead of "More", which the tabs' menu for those that don't fit is called.
- A long description on a game's or mod's page shows four lines, cut after a word, with "Read more" for the rest, so Play stays in view.

### Bug Fixes

- A game folder with a mod's levels copied into it, without a patch file, is added as a mod instead of as the game, and its game isn't offered for the library.
- "Undo" after deleting a save is no longer offered once the backup holding it has been deleted.
- Keen 1–3 mods whose level files are 64 KB or larger because a level editor left old data after the level (Keen 0: Part 1 April Fools, Robot Rebellion, Hiding from the Sky) no longer crash DOSBox after quitting to DOS. The game read the whole file and wrote past its memory; the copy the game runs from now ends where the level does.
- The top of a game's or mod's page no longer jitters at some window widths, where its picture kept switching between two sizes.
- In the dark theme, the arrow beside "Play" or "Continue world" no longer casts a shadow onto the button's left half.
- A document that a mod variant copies into its folder, such as the readme of Shadow Keen 1's original part 1, is listed once instead of twice.
- A mod variant from an archive of its own, such as Shadow Keen 1's 2005 remake, no longer shows the readme of the mod's main download, which belongs to another release.

### Misc

## 1.3.0 (2026-11-14)

### Features

- Xbox and PlayStation controllers (DualShock 4, DualSense) play every game without setup, also on Linux with the button numbers Linux gives them: the controller presses the keys the game is set to, and with View (Share) held the keys of the menus, save slots in Keen 1–3 and letters for save names in Keen 4–6 and Keen Dreams; View+RT switches between window and full screen, like Alt+Enter. Without one-button firing in Keen 1–3, the fire buttons press Jump and Pogo together.
- The settings can set the controller's layout (Xbox or PlayStation) for controllers found wrongly or not at all, and a controller connected while a game runs that started without one brings a hint to start the game again, as DOSBox only looks for controllers at its start.
- The Controls tab shows the controller too: a drawing with what each button does while playing and, with View held, in the menus, with the Xbox or PlayStation buttons and names (as do the press hint and the on-screen keyboard). It opens on the controller when one is connected, and the first start of a series with a controller connected shows the controller's buttons once, apart from the keyboard's, which come again on a start without it.
- BloogBase itself can be used with a controller: the D-pad or left stick moves to the nearest element in that direction, A activates it, B closes a dialog or goes back, the right stick scrolls, LB/RB switch tabs, LT/RT scroll a page (the focus follows onto what's in view), and View or Menu jump to the main navigation and back. A on a text field opens an on-screen keyboard (QWERTZ in German, QWERTY in English, a symbols page, digits only for numbers) with X for Backspace, Y for Space, LB/RB to move the caret and Menu for Done.

### Improvements

- Keys follow the keyboard layout on Linux and macOS too, so a German keyboard answers "Y" with Y there as well.
- On macOS, keys are named as on Mac keyboards (⌃ control, ⌥ option, ↩ return, ⌫ delete, fn with the arrows for Home/End/Page Up/Page Down) in the Controls tab, the settings' hints, the one-button firing note and the controller's texts; the Controls tab notes that F-keys may need fn, and fullscreen is named ⌃⌘F (the View menu's shortcut; F11 shows the desktop there). The fullscreen button follows when the window switches through the menu or its green button.
- With a search or filter set, Discover's count says how many of the mods are shown ("12 of 96 mods").
- Adding a mod shows how many saves come with it, besides the number of files.
- A game whose executable was renamed (KEEN5.EXE for KEEN5E.EXE) is recognised by the executable's hash, as long as the extension is the same, and gets its usual name back in the library; the player's folder stays as it is.
- A mod that doesn't run on the player's copy of its game says which version that copy is, with "Learn more" explaining why patch mods need one exact release (CKPatch refuses FormGen, GT Interactive, CGA and other versions).
- Steam and GOG games already in the library show their version when importing.
- A mod that brings its own executable and all of its game's files plays on any version of the game, as the KeenWiki download does, and for Keen 1, Keen 4 and Keen Dreams without a copy of the game at all (their shareware is free). It stays with its game in Discover and the library, unlike source mods. Levels played over such a mod (xky1rooms over Episode X) need no copy either.
- Discover's "Works with my games" filter is now "Hide mods I can't play", as it keeps mods that need no game and those a free shareware version comes along with. Its game choice "No game needed" is now "Standalone", and so is the library's group for source mods.
- A mod made for a free shareware version the library lacks (Keen 1 v1.31, Keen 4 v1.4 EGA or CGA, Keen Dreams v1.13) installs it along with the mod, in Discover and when adding a mod: the button says "Install" with both sizes added up and "With Keen 4 v1.4 EGA (shareware)" instead of "Needs …".
- "What's new" shows the newest version's changes and folds away the versions skipped in between, with a link to all changes.
- Without DOSBox, a game's or mod's page turns Play red as "DOSBox missing" and says below it why, with the download (or the way to the settings); a mod whose game is missing gets the red button too, and both notes an icon.
- Release builds carry the mod list published when they were built, so mods can be found and installed before BloogBase first reaches GitHub Pages; a newer list from there replaces it as before.
- "Add game or mod" puts the cursor in the KeenWiki link's field, ready to paste a link.

### Bug Fixes

- At 150 % display scaling, pages no longer jump by a pixel when a menu or dialog opens or closes, after scrolling the library.
- On macOS, the Controls, Screenshots and Videos tabs name Ctrl+F5 and Ctrl+F7 for screenshots and videos, the keys DOSBox has there with BloogBase, instead of ⌘+F5 (which turns on VoiceOver) and ⌘+F7.
- A cover whose image file has gone missing is extracted again when it fails to show, instead of leaving an empty picture; if that fails too, the placeholder shows.
- The Controls tab no longer greys out the Up and Down arrow keys for Keen 1–3, which move Keen on the world map.

### Misc

- The Linux package (deb) carries a description, which Ubuntu's App Center showed as "(none)" before.
- Updated dependencies, among them the built-in SQLite for a security fix (RUSTSEC-2026-0459).

## 1.2.1 (2026-11-10)

### Improvements

- A folder or ZIP holding a mod and its game, in a release the library doesn't have yet, asks whether to add only the game, only the mod or both; with both, the game comes first and the mod is set up on it.
- Source mods from the mod list are recognised in a folder or ZIP the player already has, older versions too, by the SHA-256 of their programs: they install with the player's files and saves and start the way the list tested them; Discover then offers the newest version as an update.

### Bug Fixes

- A mod's page showed its old version after an update from the mod list until it was opened again.
- The add dialog no longer offers to set up a source mod from the mod list by hand, which would have played it on a copy of its game, where it doesn't run.
- A mod kept while its game was uninstalled showed an empty cover once the game was installed again.

## 1.2.0 (2026-11-08)

### Features

- Sound, music, the sound device, the score box and keys can be set once for all games and mods in the settings.
- BloogBase can go fullscreen, without the taskbar, from a button in the sidebar or with F11, and can start in fullscreen; while it is in fullscreen, games start in fullscreen too, whatever their settings say.
- A "Quit BloogBase" button at the bottom of the sidebar closes the app, also in fullscreen, and asks first while games run, as the window's close button does.
- A new setting for all games says what a game does while another window is active: keep running (as before), mute, or pause (DOSBox's `mute_when_inactive` and `pause_when_inactive`).
- A "Legacy PC mode" setting for all games (off by default) makes the hard disk slow and audible, as on a PC of the time (DOSBox's `hard_disk_speed = slow` and `hard_disk_noise = on`).
- BloogBase knows 50 releases now, eight more: Keen 2 and 3 v1.1 and v1.32, the Keen 6 Apogee demo and id Anthology release, and Keen Dreams v1.05 and v1.20.
- Custom patches: patch file lines for a game copy, entered in the new "DOSBox & patches" tab of its page, go into the game and every mod played on that copy at each start, after the mod's own lines; a mod's tab says whether it gets them. Can be used to add or remove features.
- Mods that bring their whole game with them (source mods such as Foray in the Forest) can be marked standalone in the mod list: they play without a copy of the game they're built on, show up under "No game needed" in Discover and in the library, and the setup wizard suggests them even without any Keen; installed ones leave their copy of the game at the next check of the mod list, saves included. Source mods that carry the game's own graphics or sounds also get `requiresGame`: they still play from their own files alone, but are listed, installed and kept like any other mod of that game and need a copy of it.
- Added support for Atroxian Realm, Keen Dreams Gold Edition and Return to the Shadowlands.

### Improvements

- The Windows installer updates an older BloogBase in place, as the in-app updater does, instead of offering to uninstall it first.
- Download sizes under 1 MB are shown in KB instead of as 0.4 MB.
- Download sizes use the decimal comma in German (2,0 MB).
- On Windows the title bar wears BloogBase's theme (Midnight Galaxy or Vintage Beige) instead of following Windows' light or dark mode and accent colour; Windows 10 gets light or dark only.
- The settings page is split into Appearance, Display, Sound and background, Controls, Backups, DOSBox and BloogBase, with a list that jumps to each and marks the one in view, beside the cards on wide windows and as a bar above them on narrow ones; keys come first under Controls, with how many are set, and "applies from the next start" is said once under the page title instead of under each setting.
- Discover's mods tab keeps its search, filters and sort when you switch to another page and back, until the app closes; the news dialog's link to the new mods starts without filters.
- Source mods get a tile of their own in Discover, and as placeholder cover, a "+" on EGA dark cyan instead of the blue number of the game they're built on.
- The library keeps its search when you switch to another page and back, until the app closes or a new game or mod is added.
- The library shows its games sooner after a start: looking for the names a mod's programs save under no longer scans them byte by byte.
- The library shows a hopping Bloog with "Loading your library…" until its games are there, instead of an empty page.
- Keen Dreams v1.0 and v1.05 get their title screen as cover instead of the placeholder; these releases draw it from a map rather than keep a picture.
- "What DOSBox gets" on a game's page names the settings page as the source of what it sets ("Settings page") instead of "Display options", since it sets sound, keys and more now.
- Discover and a mod's page say "Released on …" before a mod's date; dates known only to the month or year read "in July 2017" or "2017".
- On a game's "DOSBox & patches" tab, "What DOSBox gets", BloogBase's defaults and custom patches fold away the same way under their headings; the first two start folded.
- The "Mod won't start?" and bug report dialogs say in two steps what to do, including that GitHub needs a free account, and fold the report away behind "Show the report".
- The sidebar shows the logo's "BloogBase" lettering, in the theme's colours, instead of the name in pixel type.
- An older mod list from the server gets its own message instead of the one for a damaged download.
- The library remembers over restarts which names a mod saves under, so its games show right away after a start.
- A player's own DOSBox Staging has to be 0.82.0 or newer, since older versions don't know the setting for the game speed; 0.82.x gets a notice that the CRT switch has no effect there.
- The Windows uninstaller has the BloogBase icon instead of the NSIS default.

### Bug Fixes

- Switching a running game to fullscreen with Alt+Enter no longer garbles the picture or breaks the game: DOSBox now uses borderless fullscreen by default (`fullscreen_mode = forced-borderless`).
- Pixel-font headings no longer join "fi" into one glyph without the i's dot, which made "inoffizielles" on the About page read "inoffzielles".
- A comma after a download on a KeenWiki page no longer shows up as the version's note, as in "v0.2 (demo) (,)".
- Versions a KeenWiki page lists inside a paragraph show up in the download choice too; Atroxian Realm's v1.3, v1.1 and v1.0 were missing.
- Source mods whose cutscenes are as colorful as their title, like Atroxian Realm, get the title screen as cover: BloogBase takes the full-screen picture the mod's program draws right after loading it.
- Mods downloaded from a link with a query, such as Dropbox's `…/KDGOLD.ZIP?rlkey=…&dl=1`, install on Windows instead of failing with an invalid file name: the archive is kept under the name before the `?`.

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

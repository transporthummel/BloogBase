# Screenshots

The README's screenshots are taken by a script, so they can be taken again
after a new feature or a redesign:

```sh
node scripts/screenshots.mjs                 # all shots
node scripts/screenshots.mjs --only library  # some of them, comma-separated
node scripts/screenshots.mjs --fresh         # with a new data folder
```

Windows only (it drives the app's WebView2 over the Chrome DevTools
Protocol). The script starts a second dev instance on port 1430 with its own
data folder, build folder and WebView2 profile in `.local/` (the usual
`pnpm tauri dev` on port 1420 isn't touched) and stops it at the end. The
first run builds the app and fills the data folder, which takes a while;
later runs reuse it. `--keep` leaves the instance running, and `--attach`
takes shots from it again, for trying out a shot without waiting for the
start.

## What's in a shot

Everything the script does is in [`screenshots/shots.json`](screenshots/shots.json):

- `window`: the size of the page in CSS pixels and the scale factor.
- `localStorage`: preferences set before every shot.
- `seed`: what goes into a new data folder. `games` are folders with game
  copies, relative to `.local/games` (`SCREENSHOTS_GAMES` points elsewhere);
  `mods` are mod list ids, installed as from Discover (debug builds read the
  list from `index/dist/index.json`, so run `pnpm index:build` first);
  `sessions` adds play time (`minutes`, ending `hoursAgo`); `saves` copies
  saved games from another game folder into a profile (`from`, `files`);
  `backups` makes backups of a profile's saves. Profiles are named by id, mod
  id, mod list id, label, or game id for a game's own profile.
- `shots`: one entry per picture, written to `screenshots/<name>.png`:
  - `name`, `description` (what it shows and why, for whoever redoes it),
  - `route`, where `{profile:<ref>}` becomes the id of that profile,
  - `theme` (`dark` or `light`), `language` (`en` or `de`), `width`,
    `height`, `scale`, `localStorage`,
  - `invoke`: app commands (`cmd`, `args`) called before the page loads, e.g.
    `set_setup_step` to open the setup wizard at a step,
  - `steps`, run in order once the page has settled: `click` (CSS selector),
    `clickText` (a button, link, tab or option whose text starts with it;
    `selector` narrows it), `fill` (selector of an input, with `value`), `focus`, `type` (text typed into the focused field),
    `key` (`Enter`, `Escape`, `Tab`), `waitFor` (selector), `waitText`,
    `wait` (ms), `eval` (JavaScript in the page),
  - `clip`: a selector to take only that element (`clipPadding` around it).

Every shot starts from a fresh load of its route, with animations off
(reduced motion). The script waits for images to load before taking it.

## Adding or changing a shot

1. Add or change the entry in `shots.json`; the README links the picture by
   its name.
2. `node scripts/screenshots.mjs --keep --only <name>`, then
   `--attach --only <name>` while adjusting the steps.
3. Look at the picture before committing it.

When a shot needs something new in the data folder, add it to `seed` and run
with `--fresh`. Copyrighted games may show in the pictures; the games
themselves stay in `.local/games`.

# Website

The project website lives in `site/` and is published to GitHub Pages at
`https://transporthummel.github.io/BloogBase/` (English) and `…/de/`
(German), together with the mod index at `…/index/`. The press kit is at
`…/press/` and `…/de/press/`, linked from the footer.

## Files

- `page.html`: the main page, one template for both languages. `{{key}}` is a text
  from `i18n/<lang>.json`, `{{@root}}`, `{{@lang}}`, `{{@url}}` and
  `{{@otherUrl}}` are filled in per language, `{{@updated}}` (and
  `{{@updatedIso}}`) is the time of the last commit to the site, its
  screenshots, logo, icon or mod list (the build time without git history), in UTC,
  shown in the footer as "Last updated", `{{shot:<name>}}` is a
  screenshot from `docs/screenshots/` (`<name>.<lang>.png` where it exists)
  and `{{size:<name>}}` its `width`/`height`.
- `press.html`: the press kit, with the same placeholders, plus `{{@home}}`
  (the main page), `{{@base}}` (the site's address), `{{@modCount}}` (the
  files in `index/mods/`), `{{@zipSize}}` and `{{png:<name>}}`, a picture's
  type, size and file size ("PNG · 2560 × 1600 · 398 KB") for a screenshot's
  name or a file in `img/`. Its facts, texts to copy and story are in the
  `press.*` texts; the story is the owner's, from the PCKF introduction.
- `i18n/en.json`, `i18n/de.json`: every text, the same keys in both; the
  build stops when one is missing. Texts may hold inline HTML.
- `style.css`: the app's Midnight Galaxy colours, type and components
  (`.local/DESIGN.md`).
- `main.js`: the drifting sky, screenshots switching on, the Bloog hopping
  along the trail, the theme comparison, the download button for the
  visitor's system (the one in the download section; the hero's only names
  the system and leads there, so visitors see the install hints) and the file
  names under each system, which link straight
  to their files with the version filled in (from the latest release's assets
  via the GitHub API, else the release page; an `.asset` link's
  `data-asset` is the end of its file name). The page works without it;
  reduced motion turns the movement off.
- `press.js`: the press kit's drifting sky and Copy buttons (hidden without
  it).
- `fonts/`: Inter and Press Start 2P (latin), with their OFL licences.
- `build.mjs`: writes `site/dist/` (ignored): `index.html`, `de/index.html`,
  `press/index.html`, `de/press/index.html`, the screenshots, logo and icon
  (also full size, `docs/logo.png` and the 512px app icon), fonts, styles,
  scripts and `bloogbase-press-kit.zip` (logo, icon, every screenshot but the
  macOS help, by language, and a README from the `press.*` texts). Node only,
  no dependencies.

## Working on it

```sh
node site/build.mjs
```

Then serve `site/dist/` with any static server (fonts don't load from
`file://`), e.g. `pnpm dlx serve site/dist`.

The screenshots come from `node scripts/screenshots.mjs`
([screenshots.md](screenshots.md)); shots used here list `"languages": ["en", "de"]`
so the German page gets German ones. Take them again after a redesign, then
rebuild.

## Publishing

`.github/workflows/index.yml` builds the website on every change to `site/`
or the screenshots (as a check), and its `publish` job puts website and mod
index into one Pages artifact: a repository has one Pages site, so a second
workflow would overwrite the index. The index's path `index/index.json` is
built into the app and must not change. Publishing waits for the same
switch as the index (`INDEX_PUBLISH`, [index/README.md](../index/README.md)).

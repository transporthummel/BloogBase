# Website

The project website lives in `site/` and is published to GitHub Pages at
`https://transporthummel.github.io/BloogBase/` (English) and `…/de/`
(German), together with the mod index at `…/index/`.

## Files

- `page.html`: the page, one template for both languages. `{{key}}` is a text
  from `i18n/<lang>.json`, `{{@root}}`, `{{@lang}}`, `{{@url}}` and
  `{{@otherUrl}}` are filled in per language, `{{shot:<name>}}` is a
  screenshot from `docs/screenshots/` (`<name>.<lang>.png` where it exists)
  and `{{size:<name>}}` its `width`/`height`.
- `i18n/en.json`, `i18n/de.json`: every text, the same keys in both; the
  build stops when one is missing. Texts may hold inline HTML.
- `style.css`: the app's Midnight Galaxy colours, type and components
  (`.local/DESIGN.md`).
- `main.js`: the drifting sky, screenshots switching on, the Bloog hopping
  along the trail, the theme comparison and the download button for the
  visitor's system (from the latest release's assets via the GitHub API,
  else the release page). The page works without it; reduced motion turns
  the movement off.
- `fonts/`: Inter and Press Start 2P (latin), with their OFL licences.
- `build.mjs`: writes `site/dist/` (ignored): `index.html`, `de/index.html`,
  the screenshots, logo, icon, fonts, styles and script. Node only, no
  dependencies.

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

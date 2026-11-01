# Security

BloogBase downloads mods made by the community and runs them in DOSBox, so
it treats every mod, archive and web page as untrusted. This page says how,
what it doesn't cover, and how to report a problem.

## Reporting a vulnerability

Please report security problems privately, not in a public issue:
[Report a vulnerability](https://github.com/transporthummel/BloogBase/security/advisories/new)
(GitHub's private vulnerability reporting). Say what you found, how to
reproduce it and which version and system you use. BloogBase is a hobby
project, so an answer may take a few days.

Only the latest release gets fixes. The in-app updater brings them.

## What BloogBase does

### Updates and the mod list

- **App updates** are signed; the updater only installs an update with a
  valid signature from BloogBase's own key, and only from the GitHub
  releases (HTTPS).
- **The mod list** (the index behind Discover, with each mod's download and
  how to start it) is signed with a key of its own
  ([minisign](https://jedisct1.github.io/minisign/)). The app checks the
  signature every time it reads the list, also the copy it keeps for
  offline use, and ignores anything else. It refuses a list older than the
  one it has, so an old signed copy can't bring back old entries.

### Downloads

- Every download from the mod list, the free game versions and DOSBox has a
  known SHA-256 and is only used if it matches. It stops at twice its listed
  size, so a server can't fill the disk.
- Downloads from a KeenWiki link have no known hash (the wiki is open to
  everyone). They are limited to 256 MB, must be ZIP archives, and their file
  names are reduced to plain letters, digits and `._-`.

### Archives

- ZIP entries whose path would leave the target folder (absolute paths,
  `..`) are refused. Only files are copied, never links.
- An entry larger than 256 MB, or one that unpacks to more, is refused, so a
  damaged or hostile archive can't fill the memory.
- A mod's archive is kept under its file name only, never a path.

### Running mods

- Mods run in DOSBox Staging, which BloogBase starts with its own config
  files only: no `dosbox.conf` from a mod is read and no `[autoexec]` runs.
- Right after mounting the game, BloogBase switches DOSBox to secure mode
  (`config -securemode`). A mod's programs and batch files can then no longer
  `MOUNT` other folders of the computer, change the mounts or restart
  DOSBox to leave secure mode. Details in [dosbox.md](dosbox.md).
- Games and mods only see their own folder (drive C) and a folder for the
  screen at exit (drive X). The game's own files are never written to.

### What the app shows

- Readmes, descriptions and changelogs are shown as plain text, never as
  HTML or Markdown. Web links in them open in the system browser, and only
  `http`/`https` links count.
- Web pages, PDF and Word files that come with a mod open in the browser or
  their own app, never inside BloogBase.
- A Content Security Policy in the app window is the second layer: should
  some text ever end up as markup anyway, it can't run scripts, reach the
  app's commands or load anything from the internet. What it blocks is
  written to the log.

### Builds

- The signing keys live in GitHub's encrypted secrets and only in the jobs
  that sign. Actions in all workflows are pinned to a commit, and the mod
  list tool is compiled before its key is in the environment.
- Pull requests never get a key.
- A weekly job checks the dependencies against known vulnerabilities
  (`cargo audit`, `pnpm audit`), and on every change to a lockfile.
  Dependabot proposes updates weekly, each release at least 7 days old
  ([dependencies.md](dependencies.md)).
- BloogBase has no telemetry and no accounts. It only goes online for the
  mod list, updates, downloads and KeenWiki pages you ask for.

## What it doesn't cover

- **DOSBox is no sandbox.** Secure mode keeps mods away from your files, but
  a DOS program can still change DOSBox settings while it runs, e.g. turn on
  a modem on the serial port and connect to the internet. Mods from KeenWiki
  or a ZIP of your own are not checked by anyone; add mods you trust.
- **The installers are not code-signed** (paid certificates; see
  [releasing.md](releasing.md#no-code-signing-decided-2026-10-17)). Windows
  SmartScreen and macOS Gatekeeper warn on the first start. Download
  BloogBase only from its
  [GitHub releases](https://github.com/transporthummel/BloogBase/releases).
- Files a mod brings along (Word documents, PDFs, web pages) open in their
  usual app with that app's protection, not more.

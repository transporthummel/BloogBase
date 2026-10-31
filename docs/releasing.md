# Releasing BloogBase

Releases are made by `.github/workflows/release.yml`, started by hand. It
prepares the version on `main` (commit "Prepare version …" and tag `v…`),
builds installers for Windows (NSIS), Linux (AppImage, deb) and macOS (Apple
Silicon and Intel, `.app` + DMG), signs the updater artifacts and drafts a
GitHub release with everything plus `latest.json` for the in-app updater.
Publish the draft by hand after a smoke test.

## Changelogs

Two files collect changes as they're made, each with the commit that makes
them:

- `CHANGELOG.md` (English): every relevant change, under "Unreleased" in
  Features, Improvements, Bug Fixes or Misc. A release's section becomes its
  description on GitHub, followed by the download hints from
  `.github/release-downloads.md` (keep those in step with the README).
- `src/updates/changelog.json`: what players should hear about, in English and
  German (`unreleased.en`, `unreleased.de`). The app shows it once after
  updating ("What's new"); the update card before updating shows no notes.

## Checklist

1. Before the first release, and when a new macOS version changes how it
   blocks unsigned apps: Actions → macOS screenshots → Run workflow, then
   copy the good pictures to `docs/screenshots/` and commit them
   ([screenshots.md](screenshots.md#macos-open-anyway)).
2. Push `main` and check that both changelogs' unreleased entries are complete.
3. On GitHub: Actions → Release → Run workflow (from `main`). Enter the
   version (like `1.2.0`) and, to be safe, the commit you checked; the
   workflow stops if `main` is somewhere else by then.
4. The workflow stops before changing anything if the version isn't newer
   than the last release or than the project's, or if an unreleased section is
   empty. Otherwise it writes the version into `package.json`,
   `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and
   `src-tauri/tauri.conf.json`, moves the unreleased entries under it (dated,
   in `CHANGELOG.md`), commits that to `main` with the tag and builds.
5. `git pull`, so your `main` has the "Prepare version" commit.
6. Download the draft's installers, try them, publish the draft.

If a build fails, "Re-run failed jobs" builds the same tag again. To drop a
release that went wrong, delete the draft and the tag and revert the "Prepare
version" commit, which puts the unreleased entries back.

## Secrets

| Secret                               | Needed for                                                                                                                               |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | Updater signatures; without it the workflow fails. Paste the content of `.local/updater-signing/bloogbase-updater.key` (exists locally). |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Empty for the current key.                                                                                                               |

Losing the updater key means installed apps can't verify updates any more;
keep a backup of `.local/updater-signing/` outside the repo.

Workflows that hold a signing key (`release.yml`, the `publish` job of
`index.yml`) use actions pinned to a commit, with the tag as a comment
(`actions/checkout@<sha> # v4`), so a moved tag can't run foreign code next to
the key. To update an action, look up the commit its tag points at now
(`git ls-remote https://github.com/<owner>/<action> refs/tags/<tag>`) and
replace the hash. The index tool is compiled in a step of its own before the
index key is in the environment, so dependencies' build scripts never see it.
More in [security.md](security.md).

## No code signing (decided 2026-10-17)

Paid certificates (Apple Developer Program, Windows code signing) are not
worth their yearly cost for this project, so the installers are not
code-signed. The updater signature above is separate and free; it still
protects updates.

- **Windows:** SmartScreen warns about the unsigned installer ("Windows
  protected your PC" → _More info_ → _Run anyway_). The warning fades as more
  people download a release. A free option for later: the
  [SignPath Foundation](https://signpath.org/) signs open source projects
  for free once the repo is public (application required).
- **macOS:** builds are signed ad hoc (`signingIdentity: "-"`), which keeps
  Apple Silicon from calling the app damaged, but they are not notarized.
  The first start is blocked; users allow it under _System Settings →
  Privacy & Security → Open Anyway_ (macOS 15+; older versions: right-click →
  Open). Auto-updates afterwards install without that step.
- **Linux:** nothing to sign.

The download hints in `.github/release-downloads.md`, added to every
release's description, explain these steps, and so does the README.

## Updates

The app checks `https://github.com/transporthummel/BloogBase/releases/latest/download/latest.json`
once per start in release builds, and on demand in Settings → Updates. That
URL only works once the repository is public and a release is published.
Drafts and pre-releases are not "latest", so they are never offered.

## Third-party licenses

`pnpm licenses:generate` writes `public/third-party-licenses.txt` (all Rust
crates compiled into the app and all production npm packages, with their
license texts). The release workflow runs it before building; the About page
shows the file. It is not committed.

## Local release build

Bash:

```bash
pnpm licenses:generate
TAURI_SIGNING_PRIVATE_KEY="$(cat .local/updater-signing/bloogbase-updater.key)" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
pnpm tauri build --config src-tauri/tauri.release.conf.json
```

PowerShell:

```powershell
pnpm licenses:generate
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content .local/updater-signing/bloogbase-updater.key -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
pnpm tauri build --config src-tauri/tauri.release.conf.json
```

Without the release config (`pnpm tauri build`) no updater artifacts are made
and no key is needed.

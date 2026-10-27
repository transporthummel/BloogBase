# Releasing BloogBase

Releases are built by `.github/workflows/release.yml` when a tag `v*` is
pushed. It builds installers for Windows (NSIS), Linux (AppImage, deb) and
macOS (Apple Silicon and Intel, `.app` + DMG), signs the updater artifacts and
drafts a GitHub release with everything plus `latest.json` for the in-app
updater. Publish the draft by hand after a smoke test.

## Checklist

1. Bump the version in `package.json`, `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json` (all three must match).
2. Add the release's changes, in English and German, at the top of
   `src/updates/changelog.ts`. The app shows them once after updating.
3. `git tag v1.0.0 && git push origin v1.0.0`.
4. Wait for the workflow, download the draft's installers, try them.
5. Add release notes to the draft if you like (only GitHub shows them; the
   app's update card doesn't) and publish it.

## Secrets

| Secret                               | Needed for                                                                                                                               |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | Updater signatures; without it the workflow fails. Paste the content of `.local/updater-signing/bloogbase-updater.key` (exists locally). |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Empty for the current key.                                                                                                               |

Losing the updater key means installed apps can't verify updates any more;
keep a backup of `.local/updater-signing/` outside the repo.

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

The release notes template in `release.yml` explains these steps, and so
does the README.

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

```bash
pnpm licenses:generate
TAURI_SIGNING_PRIVATE_KEY="$(cat .local/updater-signing/bloogbase-updater.key)" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
pnpm tauri build --config src-tauri/tauri.release.conf.json
```

Without the release config (`pnpm tauri build`) no updater artifacts are made
and no key is needed.

# Releasing BloogBase

Releases are built by `.github/workflows/release.yml` when a tag `v*` is
pushed. It builds installers for Windows (NSIS), Linux (AppImage, deb) and
macOS (Apple Silicon and Intel, `.app` + DMG), signs the updater artifacts and
drafts a GitHub release with everything plus `latest.json` for the in-app
updater. Publish the draft by hand after a smoke test.

## Checklist

1. Bump the version in `package.json`, `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json` (all three must match).
2. `git tag v1.0.0 && git push origin v1.0.0`.
3. Wait for the workflow, download the draft's installers, try them.
4. Write release notes into the draft (they appear in the app's update card)
   and publish it.

## Secrets

| Secret                                                                      | Needed for                                                                                           | Status                                                                                                                                                         |
| --------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`                                                 | Updater signatures. Without it the workflow fails.                                                   | Key exists locally in `.local/updater-signing/` (no password); public key is in `tauri.conf.json`. Paste the content of `bloogbase-updater.key` as the secret. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`                                        | Same; empty for the current key.                                                                     | –                                                                                                                                                              |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD`                       | Windows code signing: base64 of a PFX and its password. Skipped while unset (SmartScreen will warn). | No certificate yet.                                                                                                                                            |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` | macOS signing (Developer ID Application certificate as base64 .p12).                                 | No Apple developer account yet.                                                                                                                                |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`                               | macOS notarization (app-specific password).                                                          | Same.                                                                                                                                                          |

Losing the updater key means installed apps can't verify updates any more;
keep a backup of `.local/updater-signing/` outside the repo.

Unsigned macOS builds only open via right-click → Open (or after
`xattr -dr com.apple.quarantine BloogBase.app`); unsigned Windows installers
show a SmartScreen warning. Both are fine for a first community preview, not
for 1.0.

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

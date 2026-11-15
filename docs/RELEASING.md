# Releasing

1. Move the `[Unreleased]` section of `CHANGELOG.md` under a new version heading.
2. Set that version in the workspace `Cargo.toml` and in `apps/desktop/package.json` (the
   `packaging` test fails if they differ), and commit.
3. Tag `vX.Y.Z` and push the tag. `.github/workflows/release.yml` builds the universal macOS
   app and dmg, checks both architectures, and creates a draft release with the dmg and the
   changelog section. Review the draft and publish it.

`workflow_dispatch` runs the same build without a release, as a downloadable artifact.

## Signing and notarisation

Needs an Apple Developer Program membership. Set these repository secrets once:

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | A "Developer ID Application" certificate exported as `.p12`, base64 encoded (`base64 -i cert.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | The password chosen when exporting the `.p12` |
| `APPLE_SIGNING_IDENTITY` | E.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | The Apple ID email used for notarisation |
| `APPLE_PASSWORD` | An app-specific password for that Apple ID |
| `APPLE_TEAM_ID` | The ten-character team id |

The Tauri CLI imports the certificate, signs with the hardened runtime and notarises when these
are present. Without them the build is unsigned: it runs after right-click → Open, and the
workflow skips the `codesign`/`spctl` checks.

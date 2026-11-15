# Releasing

1. Move the `[Unreleased]` section of `CHANGELOG.md` under a new version heading.
2. Set that version in the workspace `Cargo.toml` and in `apps/desktop/package.json` (the
   `packaging` test fails if they differ), and commit.
3. Tag `vX.Y.Z` and push the tag. `.github/workflows/release.yml` builds the universal macOS
   app and dmg (and checks both architectures), and the Linux deb, rpm and AppImage for x86_64
   and arm64. Then it creates a draft release with every package, a `SHA256SUMS` file and the
   changelog section. Review the draft and publish it.

`workflow_dispatch` runs the same builds without a release; the packages are downloadable
artifacts of the run (kept 14 days).

## Linux packages

Built on Ubuntu 22.04 runners, so they need glibc 2.35 or newer. `tauri.linux.conf.json` picks
the targets; Tauri adds the WebKitGTK and GTK dependencies to the deb and rpm. The job checks that
the binary and the desktop entry are in both packages, installs the deb, and starts the app under
Xvfb with no keyring running: it must still be running after 15 seconds.

The arm64 job uses the `ubuntu-22.04-arm` runner, which is free for public repositories. A
private repository needs an arm64 runner of its own (a larger runner, or a self-hosted one), or
remove that matrix entry.

The packages are not signed. An apt or dnf repository, which would sign them, is not set up.

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

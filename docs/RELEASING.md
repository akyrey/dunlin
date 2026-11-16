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

## In-app updates

Installed copies update from the latest published release (D-036). The release workflow signs the
updater bundles (the macOS `.app.tar.gz`, the AppImage, the deb and the rpm) and attaches a
`latest.json` that points at them. Set these repository secrets once:

| Secret | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | The contents of the updater private key (`~/.tauri/dunlin-updater.key`) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Its password; leave it unset for a key without one |

A tag build fails without the key. A manual run without it builds without updater bundles.
The public key is `plugins.updater.pubkey` in `tauri.conf.json`. **Back up the private key**:
without it no installed copy can update again, and replacing it means shipping the new public key
in a release users must download by hand.

To make a new key pair: `pnpm tauri signer generate -w ~/.tauri/dunlin-updater.key`, then put the
`.pub` contents in `tauri.conf.json`.

Installed copies see a release only once it is published: `releases/latest/download/latest.json`
never resolves to a draft or a pre-release.

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
workflow skips the `codesign`/`spctl` checks. The build step removes empty `APPLE_*` variables
first: Tauri tries to sign whenever `APPLE_CERTIFICATE` exists, even when it is empty.

If the import fails with `SecKeychainItemImport: Unable to decode the provided data`, the
`APPLE_CERTIFICATE` secret is not a base64 `.p12`. Re-create it with `base64 -i cert.p12 | pbcopy`;
the export must include the private key ("Developer ID Application" with its key, from Keychain
Access → My Certificates). Also check that `APPLE_CERTIFICATE_PASSWORD` is the export password.

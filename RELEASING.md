# Releasing

Releases are built by GitHub Actions (`.github/workflows/release.yml`) on Windows, macOS and
Linux, and uploaded to a **draft** GitHub Release. Nothing is public until you publish the draft.

## Cut a release

1. Set the version (updates `package.json`, `Cargo.toml` and `Cargo.lock` together):

   ```sh
   node scripts/version.mjs 3.1.0
   ```

   Use a suffix such as `3.1.0-beta.1` for a test version; it is marked as a pre-release.
   `tauri.conf.json` has no version of its own and takes it from `Cargo.toml`.

2. Commit, merge to `main`, then tag that commit and push the tag:

   ```sh
   git commit -am "Release 3.1.0"
   git push
   git tag v3.1.0
   git push origin v3.1.0
   ```

   The tag must be the version with a `v` in front, or the workflow stops before building.

3. Wait for the **Release** workflow in the Actions tab (about 15 to 25 minutes). It creates a
   draft release named "Simple VS Manager v3.1.0" with the files below.

4. Open the draft under Releases, check the files, edit the notes, and click **Publish release**.

To try a build without tagging, run the workflow by hand: Actions, Release, Run workflow. It
makes a draft for the current version in the files; delete the draft afterwards if you don't
want it.

## What gets built

| System | Files | Built on |
| --- | --- | --- |
| Windows 10/11 (x64) | `_x64-setup.exe` (NSIS installer, installs for the current user, no admin needed) | `windows-latest` |
| macOS 11+ (Apple Silicon and Intel) | `_universal.dmg`, plus `.app.tar.gz` | `macos-latest` |
| Linux (x64) | `.AppImage` (runs on most distros), `.deb` (Debian, Ubuntu, Mint), `.rpm` (Fedora, openSUSE) | `ubuntu-22.04` |

Linux builds run on the oldest supported Ubuntu runner on purpose: the app then works on any
distro with the same or newer glibc (Ubuntu 22.04, Debian 12, Fedora 36 and later). Linux users
also need WebKitGTK 4.1, which the `.deb`/`.rpm` pull in and most desktops already have.

There is no `.msi`: the MSI format (WiX) rejects versions like `3.0.0-alpha.0`, and the NSIS
installer covers the same users.

## Code signing (optional)

Builds work without any signing secrets. Each platform signs only when its secrets are set in
the repository's Settings, Secrets and variables, Actions.

### What unsigned builds mean for users

- **Windows**: SmartScreen shows "Windows protected your PC". Users click **More info**, then
  **Run anyway**. The warning fades once a signed app builds up reputation; unsigned ones keep it.
- **macOS**: the app is ad-hoc signed so it runs on Apple Silicon, but Gatekeeper says Apple
  can't check it. Users open it once, then go to System Settings, Privacy & Security, and click
  **Open Anyway** (on older macOS: right-click the app, Open). If macOS says the app "is
  damaged", run `xattr -cr "/Applications/Simple VS Manager.app"`.
- **Linux**: no signing is expected. AppImage users mark the file as executable first.

### macOS secrets (Apple Developer Program, 99 USD a year)

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | A "Developer ID Application" certificate exported from Keychain as `.p12`, base64 encoded (`base64 -i cert.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | The password you gave the `.p12` export |
| `APPLE_SIGNING_IDENTITY` | The certificate name, e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | Your Apple ID email, for notarization |
| `APPLE_PASSWORD` | An app-specific password from appleid.apple.com (not your Apple ID password) |
| `APPLE_TEAM_ID` | Your 10-character team ID from developer.apple.com |

With the first three the app is signed; with all six it is also notarized, and Gatekeeper opens
it without any warning.

### Windows secrets

| Secret | Value |
| --- | --- |
| `WINDOWS_CERTIFICATE` | A code signing certificate as `.pfx`, base64 encoded (`[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))`) |
| `WINDOWS_CERTIFICATE_PASSWORD` | The `.pfx` password |

Most certificate authorities now issue code signing keys only on hardware tokens or in the
cloud, so a `.pfx` may not be available. Azure Trusted Signing is the usual cheap alternative;
it would need a `signCommand` in `tauri.conf.json` instead of these secrets.

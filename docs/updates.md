# Updates

## Checking for updates

Reqly looks for a newer release when it starts and then once a day, and
shows a notification with a **Details** button when it finds one. **Check for
updates** (Settings → About, command palette) asks right away.

The check is a single anonymous request for the
[release list on GitHub](https://github.com/hightemp/reqly/releases), sent
through the proxy and the CA certificates of Settings → HTTP. Nothing about
the installation is sent; a failed automatic check is silent.

Settings → About:

| Setting | |
|---|---|
| Check for updates automatically | On by default; checks at most once per `intervalHours` (24) |
| Install updates automatically | Desktop: download and install in the background, then offer a restart |
| Update channel | **Stable**, or **Pre-release** to also get `-beta` / `-rc` versions |

**Skip this version** in the update dialog hides that version from automatic
checks; newer versions and manual checks still show.

### Desktop

**Update** downloads the new version, checks its signature and installs it;
**Restart now** finishes. On Windows the installer closes Reqly by itself.
This works for the AppImage, the macOS app and the Windows installers; deb
and rpm installs (and builds without an updater key) get a **Download**
button for the release page instead.

### Android

The dialog links to the APK of the new release; Android installs it.

### Server

In the web UI of a server, administrators (everyone on a single-user server)
are told about a newer release, with the command to update the image:

```sh
docker pull ghcr.io/hightemp/reqly-server:<version>
```

then recreate the container (`docker compose up -d`). For the binary,
download `reqly-server-<version>-linux-<arch>.tar.gz` from the release and
restart it. Other users of a multi-user server see nothing.

### CLI

`reqly-cli version --check` prints the version and whether a newer stable
release exists; it exits with 2 when the check fails.

### Turning checks off

`REQLY_UPDATE_CHECK=false` in the environment turns update checks off (closed
networks); `REQLY_UPDATE_URL` points them at another release list in the
GitHub API format (a mirror).

## Releasing signed updates

The desktop app updates itself with the [Tauri updater](https://v2.tauri.app/plugin/updater/):
it downloads a bundle of the new release and installs it after checking its
signature against the public key built into the app.

Releases carry signed updater bundles when the repository has the secrets
`TAURI_SIGNING_PRIVATE_KEY` (the private key) and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The desktop jobs then build them with
[`tauri.updater.conf.json`](../apps/desktop/tauri.updater.conf.json), and the
`Updater manifest` job writes `latest.json` from them
([`scripts/updater-manifest.mjs`](../scripts/updater-manifest.mjs)):

| Platform key | Bundle |
|---|---|
| `linux-x86_64`, `linux-x86_64-appimage` | `Reqly_<version>_amd64.AppImage` |
| `darwin-aarch64`, `darwin-x86_64` (`-app`) | `Reqly_universal.app.tar.gz` |
| `windows-x86_64`, `windows-x86_64-nsis` | `Reqly_<version>_x64-setup.exe` |
| `windows-x86_64-msi` | `Reqly_<version>_x64_en-US.msi` |

The deb and rpm packages are not updated in place.

Create the key once and keep it safe: an app can only install updates signed
with the key it was built with.

```sh
pnpm -C apps/desktop tauri signer generate -w ~/.tauri/reqly-updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/reqly-updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

The public key (`~/.tauri/reqly-updater.key.pub`) goes into
`plugins.updater.pubkey` of `apps/desktop/tauri.conf.json`.

`node scripts/updater-manifest.mjs v0.10.0 --dry-run` prints the manifest of
a published release.

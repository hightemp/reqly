# Updates

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

# Android

The Android app is the desktop app (`apps/desktop`) built with Tauri 2
mobile. It runs the same core with the SQLite database in the app's data
directory, so it works offline and has the same features as the desktop app,
with the exceptions below. It needs Android 10 (API 29) or later.

## Layout and touch

On narrow screens (below 768 px) the UI switches to a compact layout:

- the sidebar opens as a drawer from the menu button or with a swipe from
  the left edge; a swipe left closes it;
- the request and the response are switched with the **Request / Response**
  control at the top; sending a request shows the response;
- open requests are listed in a sheet (the title button with the count)
  instead of the tab strip.

On touch screens controls and rows are finger-sized, holding a finger opens
the context menu (as a right click does), and a tree row held for a moment
can be dragged to another folder (a quick move scrolls the list).

## Platform differences

| Feature | Android |
|---|---|
| Unix domain socket, network interface (request settings) | Not available: the fields are disabled; requests that set them fail with “not available on Android”. |
| `os` template tag | Not available, and not offered in the tag list. |
| Files (multipart fields, binary bodies, client certificates, imports) | Picked with the Android file chooser. Request files are copied into the app's blob store, so they keep working after the original moves. |
| Saving response bodies and exports | Through the Android save dialog, one file at a time. |
| OAuth 2.0 sign-in (authorization code, implicit) | The sign-in page opens in the system browser. When it lands on the redirect URI, copy the page address and paste it into Reqly. |
| Window frame | Not drawn; the app follows the status and navigation bars and the keyboard. |

## Building

Requirements: the Android SDK with NDK 28 (`ANDROID_HOME`, `NDK_HOME`),
Java 17+, the Rust targets (`rustup target add aarch64-linux-android
armv7-linux-androideabi x86_64-linux-android`) and the usual Node and pnpm
setup. The Android project lives in `apps/desktop/gen/android`.

```sh
# Debug APK for an x86_64 emulator; without debug info it is ~30 MB.
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_STRIP=symbols \
  pnpm -C apps/desktop tauri android build --debug --target x86_64 --apk

# Release APK and AAB for phones.
pnpm -C apps/desktop tauri android build --apk --aab --target aarch64 --target armv7 --target x86_64
```

A global `RUSTFLAGS` overrides the link flags the Tauri CLI passes for
Android targets; unset it when building.

### Signing

Release builds are signed when these variables are set (otherwise the APK
and AAB are unsigned):

| Variable | Value |
|---|---|
| `REQLY_ANDROID_KEYSTORE` | Path to the keystore (`.jks`) |
| `REQLY_ANDROID_KEYSTORE_PASSWORD` | Keystore password |
| `REQLY_ANDROID_KEY_ALIAS` | Key alias |
| `REQLY_ANDROID_KEY_PASSWORD` | Key password |

The release workflow takes them from the repository secrets
`ANDROID_KEYSTORE_BASE64` (the keystore, base64-encoded),
`ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD`.
A keystore can be created with:

```sh
keytool -genkeypair -keystore reqly.jks -alias reqly -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 reqly.jks   # value of ANDROID_KEYSTORE_BASE64
```

## Smoke test

`pnpm -C ui e2e:android` drives the installed debug app on the first device
`adb` lists (Playwright's Android support connects to the app's WebView):
it creates a request, sends it to a local test server (the emulator reaches
the host as `10.0.2.2`) and checks the Android-only parts of the UI.

```sh
adb install -r apps/desktop/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
pnpm -C ui e2e:android
```

CI builds the debug APK and runs this test on an Android 15 emulator.

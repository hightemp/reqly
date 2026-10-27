# ADR-003: Android build (Tauri 2 mobile)

- Status: accepted
- Date: 2026-10-17

## Context

Android is planned for stage 8. We need to make sure early that the chosen
stack (Tauri 2, sqlx/SQLite, tokio, rustls, libcurl) does not block it.

## What was checked

- `cargo tauri android init` creates the project in `apps/desktop/gen/android`
  (committed; `build/` is ignored).
- `cargo tauri android build --debug --target x86_64 --apk` builds an APK. On
  an Android 15 emulator (API 35, x86_64) the app starts:
  - the core opens SQLite in `/data/user/0/dev.reqly.app` (visible in logcat);
  - commands over Tauri IPC work and the UI loads its settings.
- The HTTP engines (libcurl with static OpenSSL, and reqwest) build for
  `aarch64-linux-android` (NDK 28, API 29), see ADR-002.

## Findings for stage 8

- Mobile does not need the custom title bar (already disabled: `isMobile()` in the UI).
- The layout is not responsive: the sidebar takes half the screen and there
  are no safe-area insets for the status bar. A drawer and stacked panels are
  needed (T8.2).
- A debug APK with symbols is about 180 MB. To install it on an emulator,
  build with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_STRIP=symbols`
  (about 27 MB).
- Entry point: `#[cfg_attr(mobile, tauri::mobile_entry_point)]` on `run()`.

## Decision

The stack is confirmed for Android. Android builds are added to CI in stage 8.

## Update (stage 8)

- `minSdk` is 29 (Android 10), as the PRD requires; the NDK stays at 28.
- The Gradle build calls the Tauri CLI through `pnpm` (`BuildTask.kt`), so it
  runs the version in `apps/desktop/package.json`; a globally installed
  `cargo tauri` of another version breaks the build.
- `MainActivity` applies the status bar, navigation bar, display cutout and
  keyboard insets as padding: in edge-to-edge mode the WebView reports only
  the cutout to CSS (`env(safe-area-inset-*)`).
- One webview only: the OAuth 2.0 sign-in window of the desktop app is
  replaced by the system browser and a pasted redirect URL.
- Saving files goes through the dialog plugin (a content URI) and the fs
  plugin; the core cannot open content URIs.
- The smoke test drives the app with Playwright's Android support
  (`_android.devices()` → `webView()`); `connectOverCDP` does not work with
  Android WebView. Details: [docs/android.md](../android.md).

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

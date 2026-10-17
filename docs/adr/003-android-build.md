# ADR-003: Сборка под Android (Tauri 2 mobile)

- Статус: принято
- Дата: 2026-10-17

## Контекст

Android запланирован на этап 8. Нужно заранее убедиться, что выбранный стек
(Tauri 2, sqlx/SQLite, tokio, rustls, libcurl) его не блокирует.

## Что проверено

- `cargo tauri android init` создаёт проект в `apps/desktop/gen/android`
  (коммитится, `build/` игнорируется).
- `cargo tauri android build --debug --target x86_64 --apk` собирает APK. На
  эмуляторе Android 15 (API 35, x86_64) приложение запускается:
  - ядро открывает SQLite в `/data/user/0/dev.reqly.app` (видно в logcat);
  - команды через Tauri IPC работают, UI загружает настройки.
- HTTP-движки (libcurl со статическим OpenSSL и reqwest) собираются под
  `aarch64-linux-android` (NDK 28, API 29), см. ADR-002.

## Найдено для этапа 8

- На мобильных не нужен кастомный titlebar (уже отключён: `isMobile()` в UI).
- Раскладка не адаптивна: сайдбар занимает пол-экрана, нет safe-area-отступов
  под статус-бар. Нужны drawer и стек панелей (T8.2).
- Debug-APK с символами весит около 180 МБ. Для установки на эмулятор собирать с
  `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_STRIP=symbols` (около 27 МБ).
- Точка входа: `#[cfg_attr(mobile, tauri::mobile_entry_point)]` на `run()`.

## Решение

Стек подтверждён для Android. Сборка под Android добавляется в CI на этапе 8.

# Changelog

Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),
версии следуют [SemVer](https://semver.org/lang/ru/).

## [Unreleased]

### Added

- Монорепозиторий: Cargo workspace (`reqly-core`, `reqly-storage`, `reqly-api`,
  `reqly-testserver`, `reqly-server`, `reqly-desktop`) и Svelte 5 UI.
- Контракт команд UI ↔ ядро с генерацией TypeScript-типов; транспорты Tauri IPC
  и HTTP + WebSocket; общий contract-набор тестов.
- SQLite-хранилище с миграциями и бэкапом перед миграцией.
- Каркас UI: темы (Light, Dark, System), i18n (en, ru), хоткеи, SplitPane,
  кастомный titlebar, сохранение настроек на сервере и синхронизация между вкладками.

# ADR-001: Структура crates и границы модулей

- Статус: принято
- Дата: 2026-10-17

## Контекст

Reqly поставляется как desktop-приложение (Tauri 2) и как сервер (axum), которые
открывают в браузере один и тот же Svelte UI. Логика должна существовать в
одном экземпляре, а каждая фича — работать в обоих вариантах без дублирования.

## Решение

Cargo workspace:

| Crate | Назначение | Зависит от |
|---|---|---|
| `reqly-core` | Доменные примитивы (Id, SortKey, Meta, Clock), модель ошибок, позже — доменные сервисы | — |
| `reqly-storage` | SQLite (sqlx), миграции, бэкап перед миграцией, репозитории | core |
| `reqly-api` | Контракт UI ↔ ядро: команды, реестр, события, генерация TS-типов, contract-набор | core, storage |
| `reqly-testserver` | Тестовый HTTP(S)/mTLS-сервер для интеграционных тестов | — |
| `apps/server` | axum: `POST /api/v1/cmd/{name}`, WS `/api/v1/ws`, встроенная статика UI | api |
| `apps/desktop` | Tauri 2: команда `dispatch`, проброс событий в webview | api |

Будущие crates (`reqly-http`, `reqly-template`, `reqly-interop`, `reqly-protocols`,
`reqly-script`, `reqly-testing`, `reqly-sync`) добавляются по этапам и подключаются
к `reqly-api`.

Правила:

- Оболочки (`apps/*`) не содержат бизнес-логики, только транспорт.
- Каждая операция UI — команда `Command` в `reqly-api` с serde- и TS-типами.
  TS-типы генерируются `ts-rs` в `ui/src/lib/api/generated/`, генерация
  выполняется при `cargo test`; CI проверяет, что файлы актуальны.
- Ошибки — `CoreError { code, message, params }`, локализуются только в UI.
- Contract-набор (`reqly-api::contract`, feature `contract`) прогоняется через
  HTTP (server) и через Tauri IPC (desktop, mock runtime).

## Последствия

- Новая фича = команда в `reqly-api` и сервис в доменном crate. Обе оболочки
  получают её автоматически.
- `reqly-api` становится точкой сборки, поэтому его зависимости растут. Это допустимо,
  пока доменные crates не зависят от `reqly-api`.

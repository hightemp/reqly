# Reqly

Fast, themeable API client — desktop (Tauri 2) and self-hosted web (Rust server).

> Status: early development (stage 0). See [PRD.md](PRD.md) and [TASKS.md](TASKS.md) (in Russian).

## Layout

```
crates/reqly-core        domain primitives (ids, sort keys, errors)
crates/reqly-storage     SQLite storage and migrations
crates/reqly-api         typed command/event contract shared by both shells
crates/reqly-testserver  local HTTP(S)/mTLS fixtures for integration tests
apps/server              axum server: web UI + API (HTTP + WebSocket)
apps/desktop             Tauri 2 desktop app
ui/                      Svelte 5 + Vite SPA used by both
docs/adr                 architecture decision records
```

## Requirements

- Rust (stable, see `rust-toolchain.toml`)
- Node.js 22+, pnpm 10
- Tauri 2 system dependencies for desktop builds:
  <https://v2.tauri.app/start/prerequisites/>

## Development

```sh
pnpm install

# Web: run the server and the Vite dev server (proxies /api to :8484)
cargo run -p reqly-server
pnpm dev                       # http://localhost:5173

# Desktop
pnpm -C apps/desktop dev

# Tests
cargo test --workspace         # also regenerates ui/src/lib/api/generated
pnpm test                      # unit tests (Vitest)
pnpm -C ui e2e                 # end-to-end (Playwright, builds UI + server)
pnpm lint
```

### Server configuration

`reqly.toml` in the working directory (or `REQLY_CONFIG=path`), overridden by env:

| Key | Env | Default |
|---|---|---|
| `bind` | `REQLY_BIND` | `127.0.0.1:8484` |
| `data_dir` | `REQLY_DATA_DIR` | `.reqly-data` |
| `log` | `REQLY_LOG` | `info` |

## License

[MIT](LICENSE)

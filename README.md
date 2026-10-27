# Reqly

[![CI](https://github.com/hightemp/reqly/actions/workflows/ci.yml/badge.svg)](https://github.com/hightemp/reqly/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/hightemp/reqly?include_prereleases&sort=semver)](https://github.com/hightemp/reqly/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg?logo=rust)](https://www.rust-lang.org)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB.svg?logo=tauri)](https://v2.tauri.app)
[![Svelte](https://img.shields.io/badge/Svelte-5-FF3E00.svg?logo=svelte)](https://svelte.dev)
[![Docker](https://img.shields.io/badge/docker-ghcr.io%2Fhightemp%2Freqly--server-2496ED.svg?logo=docker)](https://github.com/hightemp/reqly/pkgs/container/reqly-server)

Fast, themeable API client — desktop (Tauri 2: Linux, Windows, macOS),
Android and a self-hosted web version served by a single Rust binary.

> Status: v0.3 (stage 3 of the [roadmap](PRD.md)). HTTP requests,
> environments and templates, import/export and code generation are
> supported; more auth types and protocols come next. Planning docs are in Russian: [PRD.md](PRD.md), [TASKS.md](TASKS.md).

## Features

- Workspaces → collections → nested folders and requests; tree view or
  "folders + request list" view; drag & drop (also between views),
  multi-select, cut/copy/paste, duplicate, trash with undo, per-folder sorting
- Postman-style tabs (preview tab, pinning, reopen closed)
- Request editor: URL bar synced with query params, path params, every body
  type (JSON/XML/YAML/text/HTML, form, multipart with files, binary), Basic /
  Bearer / API key auth inherited through folders, folder-level headers,
  per-request network settings, Markdown docs, paste a cURL command to import
- Response viewer: status, timing breakdown (DNS/connect/TLS/TTFB), sizes,
  pretty/raw/preview (sandboxed HTML, images, PDF, media), JSONPath / jq /
  XPath filters, headers, cookies, a raw timeline like Insomnia's, history
- libcurl engine: HTTP/2, proxies (HTTP/HTTPS/SOCKS), custom CAs, client
  certificates (PEM/PKCS#12), cookies per workspace, cancel in flight
- 10 built-in themes plus a theme editor (JSON import/export), density,
  fonts, zoom; custom window frame; remappable shortcuts; English and Russian
- Variables and environments like Insomnia: global, base and switchable
  environments, folder and request variables, encrypted secrets, highlighted
  `{{ variables }}` with hover preview and completion, template tags
  (`{% uuid %}`, `{% hash %}`, `{% jwt %}`, response chaining, prompts…) and
  fake data from faker-js in all its locales — see [docs/templates.md](docs/templates.md)
- Import from Insomnia (v4/v5), Postman (collections, environments, globals),
  OpenAPI 3.x / Swagger 2.0, HAR, cURL and Bruno with a report of everything
  that could not be imported exactly; export to Reqly (JSON/YAML), Insomnia,
  Postman, HAR and cURL; code generation for 26 languages and libraries —
  see [docs/import-export.md](docs/import-export.md)
- Command palette (`Ctrl+K`), request log, cookie manager, copy as cURL
- Server mode: token login or a team server with user accounts, workspace
  roles (owner, editor, viewer), invitation links, API tokens and an admin
  panel — see [docs/server.md](docs/server.md); SSRF protection for
  requests sent by the server, Docker image
- Collaboration on a team server: live updates, field-level merging of
  concurrent edits, real-time co-editing of bodies and scripts with
  others' cursors, presence avatars, personal or shared response history
- Android app with a phone layout (drawer, request / response switch) and
  touch gestures — see [docs/android.md](docs/android.md)

## Run with Docker

```sh
export REQLY_AUTH_TOKEN="change-me-to-a-long-secret"
docker compose -f docker/compose.yaml up -d
# open http://localhost:8484
```

## Server configuration

`reqly.toml` in the working directory (or `REQLY_CONFIG=path`), overridden by
environment variables:

| Key | Env | Default | |
|---|---|---|---|
| `bind` | `REQLY_BIND` | `127.0.0.1:8484` | Listen address |
| `data_dir` | `REQLY_DATA_DIR` | `.reqly-data` | Database and files |
| `log` | `REQLY_LOG` | `info` | `tracing` filter |
| `auth.mode` | `REQLY_AUTH_MODE` | `none` | `none`, `token` or `multi` |
| `auth.token` | `REQLY_AUTH_TOKEN` | — | Required for `token` (12+ chars) |
| `auth.registration` | `REQLY_REGISTRATION` | `admin` | `multi`: `open`, `invite` or `admin` |
| `auth.admin_username` | `REQLY_ADMIN_USERNAME` | — | `multi`: first administrator |
| `auth.admin_password` | `REQLY_ADMIN_PASSWORD` | — | Its password (8+ chars) |
| `auth.secure_cookie` | `REQLY_SECURE_COOKIE` | `false` | Set behind HTTPS |
| `network.block_private` | `REQLY_BLOCK_PRIVATE` | `false` | Deny RFC 1918 targets |
| `network.block_loopback` | `REQLY_BLOCK_LOOPBACK` | `false` | Deny 127.0.0.0/8 |
| `network.allow_hosts` | `REQLY_ALLOW_HOSTS` | — | Hosts that skip checks |
| `network.deny` | — | — | Extra CIDRs to deny |

Cloud metadata and link-local addresses are always blocked for requests sent
by the server. Scripts can call the API with `Authorization: Bearer <token>`.

## Development

Requirements: Rust (stable), Node.js 22+, pnpm 10, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for desktop builds.

Common tasks are in the `Makefile` (`make help` lists them):

```sh
make install        # dependencies, git hooks, Playwright browser
make dev            # server + UI dev server (http://localhost:5173)
make dev-desktop    # desktop app
make test           # Rust + UI tests
make e2e            # Playwright against the server
make lint           # rustfmt, clippy, eslint, prettier, svelte-check
make build          # release server with the UI embedded
make docker         # server image
make release V=x.y.z  # bump version, commit, tag (notes come from CHANGELOG.md)
```

Or directly:

```sh
pnpm install

# Web: API server + Vite dev server (proxies /api to :8484)
cargo run -p reqly-server
pnpm dev                       # http://localhost:5173

# Desktop
pnpm -C apps/desktop dev

# Tests
cargo test --workspace         # also regenerates ui/src/lib/api/generated
pnpm test                      # UI unit tests (Vitest)
pnpm -C ui e2e                 # end-to-end (Playwright; builds UI + server)
pnpm lint
```

On Linux with NVIDIA GPUs WebKitGTK may need `WEBKIT_DISABLE_DMABUF_RENDERER=1`;
the desktop app sets it automatically.

### Releases

Add a `## [x.y.z]` section to `CHANGELOG.md`, run `make release V=x.y.z` and
`git push --follow-tags`. The release workflow publishes a GitHub release with
that section as its description and attaches desktop installers, server
binaries and the Docker image (`ghcr.io/hightemp/reqly-server`).

To publish the image to Docker Hub as well, add a repository variable
`DOCKERHUB_USERNAME` and a secret `DOCKERHUB_TOKEN` (a Docker Hub access token
with read & write access) under **Settings → Secrets and variables →
Actions**. To rebuild only the image of an existing release, run the
*Release* workflow by hand with the tag and `only: docker`.

### Layout

```
crates/reqly-core        domain model, ids, sort keys, errors
crates/reqly-storage     SQLite (migrations, tree, history, cookies, search, sessions), blob store
crates/reqly-http        request preparation, libcurl engine, filters, cURL, network policy
crates/reqly-api         typed commands/events shared by both shells (+ TS bindings)
crates/reqly-testserver  local HTTP(S)/mTLS fixtures for tests
apps/server              axum server: embedded web UI, API, auth
apps/desktop             Tauri 2 app
ui/                      Svelte 5 + Vite SPA used by both
docs/adr                 architecture decision records
```

## License

[MIT](LICENSE)

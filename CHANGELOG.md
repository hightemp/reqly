# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.5.0] — 2026-10-17

Stage 5: GraphQL, WebSocket, server-sent events and gRPC.

### Added

- GraphQL requests: query editor with schema-aware completion and
  validation, variables, operation selection, prettify, schema by
  introspection or from a file (cached per request) and a docs explorer.
  Subscriptions run over WebSocket (`graphql-transport-ws` and the legacy
  `graphql-ws`).
- WebSocket requests: connect with headers and auth, send text, JSON or
  binary messages, saved messages, subprotocols and automatic reconnection.
- Server-sent events: event streams on the HTTP engine, a WHATWG parser and
  reconnection with `Last-Event-ID`.
- gRPC requests: `.proto` files (compiled in-process, no `protoc`) or server
  reflection, unary, server, client and bidirectional streaming, metadata,
  TLS, deadlines and example messages.
- A connection log for WebSocket, SSE, subscriptions and gRPC with filters,
  message details and handshake headers; protocol badges in the tree and
  tabs. See `docs/protocols.md`.
- Insomnia WebSocket and gRPC requests import and export; GraphQL bodies from
  Insomnia, Postman and Bruno import as GraphQL.

### Fixed

- Ctrl+Enter in the WebSocket composer no longer also toggles the
  connection.

## [0.4.0] — 2026-10-17

Stage 4: advanced authentication and network settings.

### Added

- Authentication: Digest (MD5, SHA-256, `auth-int`), NTLMv2, OAuth 1.0
  (HMAC, RSA, PLAINTEXT), OAuth 2.0, AWS Signature v4, Hawk, Atlassian ASAP,
  generated JWTs, Akamai EdgeGrid and netrc files (desktop). Signatures are
  checked against the published test vectors of each scheme. See
  `docs/auth.md`.
- OAuth 2.0: authorization code with PKCE, client credentials, password,
  device code and implicit grants; tokens per request or folder, encrypted
  at rest, refreshed automatically and shown in the Auth tab. Sign-in opens
  a window on desktop and a pop-up in the browser.
- Imports from Insomnia, Postman, Bruno and OpenAPI keep these auth schemes
  instead of reporting them as unsupported; exports to Insomnia and Postman
  write them back.
- Network settings (globally, per collection, folder or request): HTTP
  version (auto, 1.1, 2, h2c prior knowledge, 3 when the build supports it),
  DNS overrides (`--resolve`), connect-to, minimum and maximum TLS version,
  compression, keep-alive, a response size limit, a delay before sending,
  and on desktop a Unix domain socket and a network interface. The server
  checks overrides against its network policy and refuses sockets and
  interfaces. cURL import and "Copy as cURL" understand the matching flags.

### Changed

- A settings button in the sidebar toolbar (the one in the status bar was
  easy to miss).

### Fixed

- Linux desktop: with `HTTP_PROXY` set, the webview sent `localhost` (the dev
  server) through the proxy and showed a connection error. Local hosts are
  now added to `NO_PROXY`; `make dev` and `make dev-desktop` do the same.
- Desktop builds on Windows and macOS (an unused Linux-only helper failed
  the build).

## [0.3.0] — 2026-10-17

Stage 3: import, export and code generation.

### Added

- Import wizard: files, a folder, drag and drop, pasted text or a URL; format
  detection with manual override, preview tree, a report of everything that
  could not be imported exactly (copy or save it), import into a new
  collection or into an existing one (merge, add alongside, replace).
- Import from Insomnia v3/v4/v5, Postman Collection v2.0/v2.1 with
  environments and globals, OpenAPI 3.0/3.1 and Swagger 2.0, HAR 1.2, cURL
  (several commands at once) and Bruno (a collection folder or a zip).
- The Reqly exchange format (JSON or YAML) with `formatVersion`, migrations
  and a JSON Schema (`docs/format/reqly.schema.json`); export a workspace, a
  collection or selected items, optionally with secret values, cookies and
  response history.
- Export to Insomnia v4/v5, Postman v2.1 (environments as separate files),
  HAR and cURL.
- Code generation for 26 languages and libraries, plus a "Copy as" submenu
  for requests.
- Request and folder scripts are kept on import (they run from stage 6).

### Changed

- Escape in the tree clears the selection.
- `{% base64 %}` takes its arguments in Insomnia's order
  (`action, kind, value`); the two-argument form still works.
- `export.run` returns a list of files.

## [0.2.0] — 2026-10-17

Stage 2: variables, environments, template tags and fake data.

### Added

- Template language: `{{ var }}`, paths such as `{{ a.b[0] }}`, Insomnia's
  `_.` prefix, tags `{% tag args %}`, calls `{{ $fn() }}` and comments;
  templates inside variable values with cycle detection. See
  `docs/templates.md`.
- Environments: global (workspace), base and switchable (collection) ones
  with colors, a table and JSON editor (`Ctrl+E`) and a switcher in the
  sidebar; folder and request variables.
- Secret environment values: masked in the UI and encrypted at rest
  (XChaCha20-Poly1305, key in `secret.key` next to the database).
- Template highlighting (defined / missing), value preview on hover,
  completion after `{{` and `{%`, a tag editor with live preview, and a
  single-line template input for the URL, tables and auth fields.
- Functions: uuid, now, random (including regex), base64, urlencode, hash,
  hmac, jsonpath, jwt, cookie, request, prompt (asked before sending), file,
  os and response chaining with resend triggers.
- Fake data with faker-js naming in English and Russian (grammatical gender,
  patronymics, `+7` phone numbers), set globally or per collection; Postman
  dynamic variables (`$guid`, `$randomEmail`…).

### Fixed

- Desktop builds on Windows and macOS: module names that differed only in
  case clashed on case-insensitive file systems.

## [0.1.0] — 2026-10-17

The first version: a complete HTTP client (stage 1).

### Added

- Workspaces, collections and a tree of folders and requests: "tree" and
  "folders + list" views, drag and drop (also between views), multi-select,
  cut/copy/paste, duplicate, trash with undo, per-folder sorting, search with
  `method:` and `url:` filters.
- Postman-style tabs: preview tab, pinning, close others / to the right /
  all, reopen closed tabs, restored after restart.
- HTTP request editor: URL and query parameters kept in sync, path
  parameters, every body type (including multipart with files and binary),
  Basic / Bearer / API key auth inherited from folders, folder headers,
  request settings, Markdown descriptions, cURL import by pasting and
  "Copy as cURL".
- libcurl engine: timeline, timing breakdown by phase, HTTP/2, proxies,
  custom CAs, client certificates, cookies per workspace, cancellation.
- Response viewer: Pretty / Raw / Preview, large responses, JSONPath / jq /
  XPath filters, headers, cookies, timeline, response history and request
  log.
- Settings: 10 built-in themes and a theme editor, density, fonts, zoom,
  native window frame, remappable shortcuts, request defaults, proxy,
  certificates, history limits; command palette.
- Server: token sign-in (sessions, CSRF protection, rate limiting), SSRF
  protection, Docker and docker compose, release workflow (installers,
  binaries, GHCR image).
- A typed UI ↔ core command contract with generated TypeScript types; Tauri
  IPC and HTTP + WebSocket transports; a shared contract test suite.
- SQLite storage with migrations and a backup before migrating.
- UI foundation: themes (Light, Dark, System), i18n (English, Russian),
  shortcuts, split panes, a custom title bar, settings stored on the server
  and synced between tabs.

[Unreleased]: https://github.com/hightemp/reqly/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/hightemp/reqly/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/hightemp/reqly/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/hightemp/reqly/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/hightemp/reqly/releases/tag/v0.2.0

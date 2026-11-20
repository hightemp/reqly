# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- AI chat messages have **Copy message** and, on answers, **Branch off from
  here**: a new chat with the messages up to that answer (`ai.forkChat`).
- A context menu for text (Cut, Copy, Paste, Select all) in text fields and
  code editors, and Copy / Select all over read-only text: the desktop app
  showed no menu there. Phones keep the system's text toolbar.
- Section tabs (request, response, settings and others) scroll with the mouse
  wheel and get scroll arrows when the panel is too narrow for them.

### Fixed

- The text of AI chat messages and tool results could not be selected.
- The current line highlight in code editors hid the selection.

## [1.3.0] — 2026-11-19

AI chat with OpenAI-compatible providers.

### Added

- AI chat in a collapsible panel on the right (status bar button,
  `Mod+Shift+L`, an **AI** pane on phones), on desktop, server and Android;
  off by default (ADR-015, [docs/ai-chat.md](docs/ai-chat.md)).
  - Providers with an OpenAI-compatible API, with presets for OpenAI,
    OpenRouter, Ollama Cloud and a local Ollama; calls go through the core
    and the proxy and certificate settings; keys are write-only.
  - Streamed answers with Markdown and highlighted code, stop, chats per
    workspace (per user on a multi-user server), the current request and
    its response as attachments.
  - The MCP tools, with their own settings: reading runs at once, changes
    wait for **Allow** / **Deny** / **Always in this chat**, or run without
    asking per group.

### Fixed

- Agent tools accept `{}` for request fields a model fills by default, and
  an `auth` without `type` names what is missing.

## [1.2.0] — 2026-11-18

MCP server for AI agents, and tab bar fixes.

### Added

- MCP server for AI agents on the desktop (`127.0.0.1`) and the server
  (`/mcp`): 24 tools to browse, edit, send and run requests, manage
  environments, import and export; off by default, tools chosen per group
  or one by one in **Settings → MCP**, MCP token or personal API tokens,
  secrets masked, call log and a status bar indicator (ADR-014,
  [docs/mcp.md](docs/mcp.md)).
- `environment.get` command.

### Fixed

- Many open tabs can be scrolled: the mouse wheel scrolls the tab bar sideways,
  and arrow buttons appear when the tabs do not fit.
- Deleting a request in the tree closes its open tab, and deleting a folder
  closes the tabs of the requests inside it.

## [1.1.0] — 2026-11-17

Import from `.http` files and the clipboard, and the in-app help.

### Added

- Import `.http` / `.rest` files (JetBrains HTTP Client, VS Code REST Client)
  and raw HTTP request text: requests separated by `###`, file variables,
  JSON / form / multipart bodies and `< file` references.
- **Import into this folder…** and **Import from clipboard** (curl commands
  or HTTP requests) in the tree context menu; the import target can be a
  folder (`folderId` of `import.apply`). The desktop and Android apps read
  the clipboard through the Tauri clipboard plugin; where it cannot be read,
  the wizard opens on the paste tab.
- Palette command **Import requests from cURL or HTTP text…** opens the
  import wizard on the paste tab with the active collection as the target.
- Pasting HTTP request text into the URL bar replaces the request, like a
  pasted curl command.
- **Supported formats** window in the import wizard: every format with where
  it comes from, what is imported and examples.
- In-app help (`F1`, the **?** in the status bar, the command palette): pages
  on getting started, requests, variables, auth, protocols, import, scripts,
  tests, collaboration, sync and settings in English and Russian, with
  search, links between pages and the current keyboard shortcuts. Import,
  environments, the runner and collection scripts have a **?** that opens
  their page.
- Help pages **Script API** (every object and method with its signature,
  description and an example) and **Libraries and recipes** (chai, lodash,
  crypto-js, moment, uuid, faker-js; token caching, HMAC signing, list
  checks, data-driven runs…). Every example is run by a test.
- Scripts: `crypto.getRandomValues()` and `crypto.randomUUID()`.

### Changed

- Pasted curl commands and HTTP requests are added to the collection
  directly, not wrapped in a `clipboard` folder.
- The folder icon in the tree sits next to the expand arrow.

### Fixed

- Variable completion and value previews were cut off inside single-line
  fields in the desktop app (WebKit): editor tooltips now open above the
  page.
- Scripts: `CryptoJS.AES.encrypt(text, passphrase)` failed with "Native
  crypto module could not be used"; `require('uuid').v7()` returned a v4
  UUID.

## [1.0.0] — 2026-11-04

Stage 11: desktop ↔ server sync.

### Added

- Desktop ↔ server sync: connect the desktop or Android app to a server
  (user name and password, or a token), pick workspaces to upload or
  download, work offline; changes sync every 30 seconds, after local edits
  and on **Sync now** (`docs/sync.md`, ADR-013).
- Changes made on both sides merge field by field, text fields edit by
  edit; deleted on one side and changed on the other asks which version to
  keep. A status bar indicator shows the sync state.
- Secret environment values stay on the device unless secret sync is on.
- Commands `sync.pull` and `sync.push` on servers (within workspace roles)
  and `sync.*` on the desktop.

## [0.10.0] — 2026-11-04

Stage 10: updates.

### Added

- Update checks on every platform: at start and once a day (configurable),
  one anonymous request for the GitHub release list through the proxy and
  CA certificates of the HTTP settings; stable and pre-release channels,
  **Skip this version**, **Check for updates** in Settings → About and the
  command palette (`docs/updates.md`).
- The desktop app updates itself (AppImage, macOS, Windows): release notes,
  download with progress, signature check, install and restart, or
  automatic installs. deb and rpm installs link to the release instead.
- Android links to the APK of the new release.
- Server admins (everyone on a single-user server) are told about a newer
  release with the Docker command to update; `REQLY_UPDATE_CHECK=false`
  turns checks off, `REQLY_UPDATE_URL` points them at a mirror.
- `reqly-cli version --check`.
- Releases carry signed updater bundles and `latest.json`.
- Command `update.check`.

## [0.9.0] — 2026-10-28

Stage 9: tests, the collection runner and the CLI.

### Added

- No-code assertions in a request's **Tests** tab: status, response time,
  a header, the body, or a JSONPath / jq value with equals, contains, regex,
  number comparisons, exists and JSON Schema; templates in them render after
  the scripts (`docs/testing.md`).
- The response's **Tests** tab lists assertion results with the value
  received, before script test results; results are kept in the history.
- Collection runner for a collection or a folder: choose and order the
  requests, the environment, iterations, a CSV or JSON data file, a delay
  and stop on failure; results stream in per iteration and failed requests
  can be rerun. Scripts see `iterationData`, the iteration number and can
  call `setNextRequest`.
- Run reports as HTML, JUnit XML or JSON.
- `reqly-cli run` runs a collection from an export file or from a Reqly
  server (API token) in CI, with pretty, JSON or JUnit output, report files
  and exit codes; binaries for Linux, macOS and Windows are attached to
  releases (`docs/cli.md`).
- Commands `runner.run`, `runner.cancel`, `runner.report` and events
  `runner.progress`, `runner.finished`.

### Fixed

- `setNextRequest(null)` in scripts now ends the iteration instead of being
  ignored.
- Filter errors (JSONPath, jq, XPath) have readable messages.

## [0.8.0] — 2026-10-27

Stage 8: Android.

### Added

- Android app (Android 10 and later): a universal APK and an AAB are
  attached to releases, signed when the keystore secrets are configured
  (`docs/android.md`).
- Phone layout below 768 px: the sidebar opens as a drawer, the request and
  the response are switched with a segmented control (sending shows the
  response), and open requests are listed in a sheet instead of the tab
  strip.
- Touch: holding a finger opens context menus, a held tree row can be
  dragged, a swipe from the left edge opens the drawer and a swipe left
  closes it; controls and rows are finger-sized on touch screens.
- `app.info` lists the features the platform cannot offer.

### Changed

- On Android, Unix socket and network interface settings are disabled with
  an explanation (requests that set them fail with `http.unavailable`), and
  the `os` tag is unavailable (`template.unavailable`).
- On Android, files are picked with the system chooser and kept in the blob
  store, saved through the system save dialog, and OAuth 2.0 signs in
  through the system browser with the redirected address pasted back.

### Fixed

- Tooltips no longer pop up after a tap on touch screens.
- Tabs opened or closed right before a reload or quit are no longer lost.

## [0.7.0] — 2026-10-18

Stage 7: multi-user server and collaboration; faker-js.

### Added

- Multi-user server (`REQLY_AUTH_MODE=multi`): user accounts with
  argon2id passwords, open or administrator-only registration, the first
  administrator from the environment or `reqly-server admin create`,
  `reqly-server admin password` to reset a password, lockout after failed
  sign-ins, personal API tokens (`Authorization: Bearer rq_…`)
  (`docs/server.md`, ADR-010).
- Workspace members with the roles owner, editor and viewer; every command
  is authorized by the caller's role. Viewers get a view-only workspace.
- Settings → Account: display name, password, API tokens; Workspace
  actions → Members….
- Invitation links: one-time, expiring links that add a user to a
  workspace with a role, creating the account when needed;
  `REQLY_REGISTRATION=invite` allows registration only with a link.
- Settings → Administration for administrators: users (create, promote,
  disable, reset password, delete), all workspaces, sign-up links and the
  server configuration.
- Per-user settings and active environments on a multi-user server.
- Live updates between users: changes to a shared workspace reach its
  other members at once; private events (responses, streams, settings)
  reach only their user.
- Field-level merging of request and folder edits: concurrent changes to
  different fields both survive, and whoever loses a change to the same
  field is notified (`node.update` with `fields` and `baseVersion`).
- Real-time co-editing of the request body, GraphQL query and variables and
  scripts with other people's cursors (Yjs and `yrs`, ADR-012).
- Presence: avatars of the people in the workspace, and of those who have a
  request open in the tree and on the tabs.
- Personal or shared response history per workspace (personal by default).
- Fake data from faker-js 10.6.0 with all of its functions and 76 locales,
  bundled and run in QuickJS (ADR-009, `docs/templates.md`).
- Template arguments can be arrays and objects:
  `{{ $faker.number.int({ min: 1, max: 9 }) }}`,
  `{{ $faker.helpers.arrayElement(['a', b]) }}`.
- All Postman dynamic variables (`$randomCatsImage`, `$randomLoremParagraphs`,
  `$randomSemver`…); the editor completes them.
- Scripts: `faker`, `require('@faker-js/faker')` with every locale export,
  `require('@faker-js/faker/locale/<code>')`, `TextEncoder`;
  `replaceIn()` replaces dynamic variables such as `{{$randomFirstName}}`.
- Script completion lists faker-js modules and methods with parameters.

### Changed

- SQLite: write transactions start with `BEGIN IMMEDIATE` and wait in a
  queue, and WAL commits use `synchronous=NORMAL`; concurrent edits no
  longer fail with "database is locked" (ADR-011).
- The faker locale setting accepts any faker-js locale; the data follows
  faker-js (for `ru`, phone numbers use national formats; ask for
  `phone.number({ style: 'international' })` to get `+7`).
- `rq.faker` is the faker-js instance (`rq.faker.person.firstName()`);
  `rq.faker('module.method', ...args)` still works and now returns faker-js
  values (numbers stay numbers).

### Removed

- The built-in Rust faker (ADR-005).

### Fixed

- `{% response %}` tags could read and send requests of another workspace
  on a multi-user server.
- The server counts only failed sign-ins against the per-address limit;
  successful sign-ins of many people behind one address are no longer
  refused.

## [0.6.0] — 2026-10-18

Stage 6: scripts.

### Added

- Scripts: pre-request and after-response JavaScript on collections, folders
  and requests, run in that order in a QuickJS sandbox (5 s and 64 MB by
  default, no files or network except `sendRequest`). See `docs/scripts.md`
  and ADR-006.
- The `rq` API with `pm` (Postman) and `insomnia` (Insomnia) compatible
  names: request editing, response access and assertions, variables of every
  level, tests with chai, `sendRequest`, console, timers, `skipRequest`, and
  the modules crypto-js, lodash, chai, moment and uuid.
- Runtime variables set by scripts (highest priority, per collection, until
  restart), listed and cleared in the environment manager.
- Scripts tab with API completion and snippets; Console and Tests tabs in
  the response.
- Imported Postman and Insomnia scripts run, collection-level scripts
  included; the import report lists unsupported APIs. Bruno scripts are kept
  commented out.

### Fixed

- Pressing Enter in the URL field right after typing could send the request
  instead of accepting the highlighted completion.

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

[Unreleased]: https://github.com/hightemp/reqly/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/hightemp/reqly/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/hightemp/reqly/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/hightemp/reqly/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/hightemp/reqly/compare/v0.10.0...v1.0.0
[0.10.0]: https://github.com/hightemp/reqly/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/hightemp/reqly/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/hightemp/reqly/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/hightemp/reqly/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/hightemp/reqly/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/hightemp/reqly/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/hightemp/reqly/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/hightemp/reqly/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/hightemp/reqly/releases/tag/v0.2.0

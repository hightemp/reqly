# ADR-014: MCP server for agents

- Status: accepted
- Date: 2026-11-18

## Context

PRD §7.22: AI agents (Claude Code, Cursor, VS Code and other MCP clients)
should be able to do what a user does in Reqly: browse and edit requests,
folders and environments, send requests, run collections, import and
export. The feature is off by default; the settings turn it on and choose
which functions an agent gets.

Every user action already goes through the command registry of
`reqly-api` (about 150 typed commands): the desktop app calls it over Tauri
IPC, the server over `POST /api/v1/cmd/{name}`, `reqly-cli` in process.
Commands publish events (`tree.changed`…), so open windows update; deletes
are soft (trash, undo); in the `multi` server mode every call runs as a
user and is checked against their role (ADR-010).

The desktop app owns its SQLite database, so a second process must not
write to it.

## Options

**Transport**

1. stdio: the client starts `reqly-cli mcp`. A separate process cannot
   share the desktop database safely; it would need a bridge to the
   running app anyway.
2. Streamable HTTP served by the running app (desktop) and by the server.
   The agent works on the same data as the user and the UI updates live.

**Protocol implementation**

1. The `rmcp` SDK: complete, but a large dependency tree (schemars, its
   own transport stack), and tool schemas would have to come from
   schemars while our types use ts-rs. Builds already run short of RAM.
2. Own implementation: tools only, JSON responses, no sessions. That is a
   handful of JSON-RPC methods.

**Tools**

1. Expose every registry command as a tool.
2. A curated set of tools, written for agents, implemented on top of the
   registry commands.

## Decision

### Transport: Streamable HTTP only

- **Desktop**: when MCP is enabled, the app listens on
  `127.0.0.1:<port>/mcp` (default port `7655`, configurable). Loopback
  only; requests with an `Origin` header that is not a loopback origin are
  rejected (DNS rebinding). Turning MCP off or changing the port stops or
  restarts the listener without restarting the app. Android has no MCP.
- **Server**: the route `/mcp` next to `/api/v1`. It is served only while
  MCP is enabled.
- No stdio transport (decided with the product owner): current MCP clients
  speak HTTP, and HTTP keeps the agent on the running app's data.

### Authentication

`Authorization: Bearer <token>` on every request.

- Desktop and the server in modes `none` and `token`: an **MCP token**
  generated in the settings (random, 32 bytes, stored as a SHA-256 digest,
  shown once with a copy button, can be regenerated). The server's shared
  secret of the `token` mode is not reused: an agent token can be revoked
  on its own. Calls run as `Caller::Local`.
- Server in mode `multi`: the user's personal API token (existing
  `account.createToken`). Calls run as that user, so roles and workspace
  membership apply unchanged.

### Protocol

Own implementation in a new crate `reqly-mcp` (an axum router that apps
mount), MCP revision `2025-06-18` (also accepting `2025-03-26` clients):

- `POST /mcp` with one JSON-RPC message; requests get an
  `application/json` response, notifications `202 Accepted`. No SSE
  stream and no `Mcp-Session-Id`: tools are request/response, and
  long operations (collection runs) return when they end. `GET` → `405`.
- Methods: `initialize`, `ping`, `tools/list`, `tools/call`; the
  `notifications/initialized` notification. Capabilities: `tools` only.
- Results carry `structuredContent` and the same JSON as text content.
  Failures of a tool (validation, not found, request error) are tool
  results with `isError: true` and the localized message, so the agent can
  react; protocol errors are JSON-RPC errors.
- Tool annotations: `readOnlyHint`, `destructiveHint`, `idempotentHint`,
  `openWorldHint` (for tools that send requests).
- Input schemas are hand-written JSON Schema next to each tool and checked
  by a test against the registry input types (a valid example input of
  every tool must deserialize).

### Tools

Curated tools in groups; each one calls registry commands (through
`access::scope` with the caller), never storage directly:

| Group          | Tools                                                                                                                                   |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `read`         | `list_workspaces`, `list_collections`, `get_tree`, `get_request`, `search_requests`, `list_environments`, `get_history`, `get_response` |
| `edit`         | `create_request`, `update_request`, `create_folder`, `rename_item`, `move_item`, `duplicate_item`, `delete_item`                        |
| `run`          | `send_request`, `render_request`, `run_collection`                                                                                      |
| `environments` | `get_variables`, `set_variables`, `create_environment`, `set_active_environment`                                                        |
| `interop`      | `import_text`, `export_collection`                                                                                                      |

- Requests and folders are addressed by id; `get_tree` and
  `search_requests` return ids with names and paths. `update_request`
  takes a partial request (only the given fields change) and saves through
  the same path as the editor, so open editors merge it.
- `send_request` sends a saved request (optionally with an environment and
  per-call overrides), stores the response in the history like the UI and
  returns status, headers, timing, test results and the body cut to a
  limit (default 16 KB, `maxBodyBytes` up to 1 MB); the full body stays
  available through `get_response`.
- `delete_item` moves to the trash; the response says how to restore.
- Secret variables are returned masked (`••••`); an agent can set them but
  never read them. Request auth fields marked secret are masked the same
  way.

### Settings and enforcement

The settings key `mcp` holds `{ enabled, port, tools: { [name]: bool } }`
(defaults: disabled; when enabled, the `read` and `run` groups on, the
rest off). The settings UI shows groups with a checkbox each that expands
to the tools, presets _Read only_ and _Everything_, the endpoint, the token
and copyable client configs.

The core checks the settings on every message: a disabled tool is missing
from `tools/list` and rejected by `tools/call`; with MCP disabled the
endpoint answers `404`. On the server only an administrator changes MCP
settings (in `multi`); in `none`/`token` mode whoever can open the
settings.

### Activity

Every `tools/call` is recorded in an in-memory log (last 200 calls: time,
tool, short arguments, outcome, duration) and published as an `mcp.call`
event. The settings section shows the log; the status bar shows an _MCP_
indicator while enabled. Changes an agent makes reach the UI through the
normal events.

## Consequences

- One implementation serves desktop and server; tools reuse the commands,
  so access checks, trash, history and live updates come for free, and a
  new command does not become an agent tool by accident.
- The desktop app opens a local port while MCP is enabled; it is
  protected by loopback binding, the Origin check and the token.
- No streaming: a long collection run blocks one HTTP request (clients
  allow long tool calls; the runner keeps its own limits). Progress
  notifications can be added later with an SSE response.
- Hand-written schemas can drift from the commands; the example-input test
  guards that.
- No confirmation dialogs: an allowed tool runs (decided with the product
  owner). The trash and the activity log are the safety net.

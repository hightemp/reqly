# MCP server for agents

Reqly serves the [Model Context Protocol](https://modelcontextprotocol.io)
so AI agents (Claude Code, Cursor, VS Code and other MCP clients) can do
what a user does: browse and edit requests, folders and environments, send
requests, run collections, import and export. Design: ADR-014.

MCP is **off by default**. It is turned on, and its tools chosen, in
**Settings → MCP**; changes apply without a restart.

## Endpoint

| App | Address | Token |
|---|---|---|
| Desktop (Linux, Windows, macOS) | `http://127.0.0.1:7655/mcp` (port configurable) | MCP token from the settings |
| Server, `auth.mode = "none"` or `"token"` | `https://<server>/mcp` | MCP token from the settings |
| Server, `auth.mode = "multi"` | `https://<server>/mcp` | a personal API token of a user |
| Android | — | — |

- Transport: **Streamable HTTP**, MCP revision `2025-06-18` (clients on
  `2025-03-26` work too). One JSON-RPC message per `POST /mcp`; responses
  are `application/json`, notifications get `202 Accepted`. There are no
  sessions and no SSE stream; `GET /mcp` answers `405`. There is no stdio
  transport.
- Methods: `initialize`, `ping`, `tools/list`, `tools/call`. Capabilities:
  `tools` only.
- While MCP is disabled the endpoint answers `404`.
- Every request needs `Authorization: Bearer <token>`; otherwise `401`.
- Browser requests with an `Origin` of another site are refused (`403`):
  the desktop accepts only `localhost`, `127.0.0.1` and `[::1]` origins,
  the server only its own. Agents send no `Origin`.
- The desktop binds `127.0.0.1` only. A busy port is reported in the
  settings.

### Tokens

- **MCP token** (desktop, single-user server): 32 random bytes, created
  when MCP is first enabled and shown once; only its SHA-256 digest is
  stored. **New token** replaces it. Tools run with full access.
- **Personal API tokens** (multi-user server): created in **Settings →
  Account → API tokens**. Tools run as that user, so workspace roles apply
  (ADR-010): a viewer's agent can read but not change, and only the user's
  workspaces are visible. On a multi-user server only administrators
  change the MCP settings.

## Connecting clients

Claude Code:

```sh
claude mcp add --transport http reqly http://127.0.0.1:7655/mcp \
  --header "Authorization: Bearer <token>"
```

Cursor (`~/.cursor/mcp.json`):

```json
{ "mcpServers": { "reqly": { "url": "http://127.0.0.1:7655/mcp", "headers": { "Authorization": "Bearer <token>" } } } }
```

VS Code (`.vscode/mcp.json`):

```json
{ "servers": { "reqly": { "type": "http", "url": "http://127.0.0.1:7655/mcp", "headers": { "Authorization": "Bearer <token>" } } } }
```

The settings show these snippets with the real address and token.

## Tools

Each tool calls the same commands as the UI, so changes reach open windows
at once, deletions go to the trash and sent requests are stored in the
history. Requests and folders are addressed by id; `get_tree` and
`search_requests` give ids with names and paths. A tool that fails (a
missing id, a refused role, a network error) returns a result with
`isError: true` and the error code and message.

| Group | Tool | What it does |
|---|---|---|
| Read | `list_workspaces` | workspaces with ids |
| | `list_collections` | collections, of one workspace or all |
| | `get_tree` | folders and requests of a collection in display order, with paths |
| | `get_request` | all data of a request or folder (credentials masked) |
| | `search_requests` | full-text search by name, URL, method, description |
| | `list_environments` | environments of a collection and the global ones, variable names, the active ones |
| | `get_history` | recent responses of a request or a workspace |
| | `get_response` | a stored response: status, headers, timing, body (text or base64, cut to `maxBodyBytes`), tests |
| Edit requests | `create_request` | a new HTTP request; only `collectionId` and `name` are required |
| | `update_request` | changes only the given fields, merged like an edit in the app |
| | `create_folder`, `rename_item`, `move_item`, `duplicate_item` | |
| | `delete_item` | moves to the trash |
| Send and run | `send_request` | sends and returns the response; `environmentId` and `variables` apply to this send only |
| | `render_request` | the request with templates replaced, without sending |
| | `run_collection` | Collection Runner over a collection, folder or list, with iterations and data rows |
| Environments | `get_variables`, `set_variables`, `create_environment`, `set_active_environment` | |
| Import and export | `import_text` | cURL, `.http`, raw HTTP, OpenAPI, Postman, Insomnia, HAR, Bruno, Reqly — into a collection or folder, or as a new collection |
| | `export_collection` | Reqly JSON/YAML, Insomnia, Postman, HAR or cURL, without secrets |

The settings group the tools; a disabled tool is missing from `tools/list`
and refused by `tools/call`. Defaults when MCP is first enabled: **Read**
and **Send and run** on, the rest off.

Response bodies are cut to 16 KB by default (`maxBodyBytes`, up to 1 MB);
`bodyTruncated` tells, and the full body stays in the history.

## Secrets

An agent never reads secret values:

- secret environment variables read as `••••` (`get_variables`);
- auth credentials and credential-like headers and parameters
  (`Authorization`, `Cookie`, `*token*`, `*secret*`, `*api-key*`…) read as
  `••••` in `get_request`, unless they are template references such as
  `{{token}}`;
- the values of secret variables and literal credentials of the request are
  replaced by `••••` in `send_request`, `get_response` and
  `render_request` results, so an echoing endpoint does not reveal them.

Writing works: `set_variables` with `secret` stores secrets, and a `••••`
sent back by `update_request` or `set_variables` keeps the stored value.

## Activity

The last 200 tool calls are listed in **Settings → MCP → Agent calls**
(time, tool, arguments with secrets removed, result, duration) and kept in
memory until a restart. The status bar shows **MCP** while it is served.

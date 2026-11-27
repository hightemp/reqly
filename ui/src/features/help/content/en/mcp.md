# MCP for agents

Reqly has a built-in [MCP](https://modelcontextprotocol.io) server. AI
agents such as Claude Code, Cursor or VS Code can then work with your
requests the way you do: browse collections, change requests, send them,
run collections, read responses and manage environments.

MCP is **off by default** and only the tools you allow are available.

## Turning it on

**Settings → MCP → MCP server**. Changes apply at once, no restart.

- **Desktop**: the app listens on `http://127.0.0.1:7655/mcp`, on this
  computer only. Change the **Port** if it is taken; a busy port is shown
  in red. The server stops when the app closes.
- **Server**: the endpoint is `/mcp` on the server's address. On a
  multi-user server only administrators turn MCP on.
- The Android app has no MCP server.

While MCP is on, **MCP** appears in the status bar; click it to open the
settings.

## The token

Every agent request carries `Authorization: Bearer <token>`.

- **Desktop and single-user servers**: the token is created when you first
  turn MCP on and shown **once**: copy it then. **New token** replaces it;
  agents with the old one lose access.
- **Multi-user servers**: agents use a personal API token (**Settings →
  Account → API tokens**) and act with that user's rights: they see only
  that user's workspaces, and a viewer's agent cannot change anything.

## Connecting a client

**Connect a client** shows what to paste, with your address and token:

```text
claude mcp add --transport http reqly http://127.0.0.1:7655/mcp --header "Authorization: Bearer <token>"
```

Cursor (`~/.cursor/mcp.json`) and VS Code (`.vscode/mcp.json`) take the
same address and header as JSON.

Codex (`~/.codex/config.toml`) reads the token from an environment variable:

```toml
[mcp_servers.reqly]
url = "http://127.0.0.1:7655/mcp"
bearer_token_env_var = "REQLY_MCP_TOKEN"
```

or `codex mcp add reqly --url http://127.0.0.1:7655/mcp --bearer-token-env-var REQLY_MCP_TOKEN`.
Start Codex with `REQLY_MCP_TOKEN=<token>` in its environment.

To point an agent at a request, use **Copy path / ID → Copy for AI** in the
context menu of the request (in the tree or on its tab): it copies the path,
the ID, the method and URL, and the `get_request` call that reads it.

## Tools

Tools are grouped; tick a group or open it to choose single tools.
**Read only** and **Everything** set all of them at once. When MCP is
first turned on, **Read** and **Send and run** are allowed.

| Group             | What an agent can do                                                                        |
| ----------------- | ------------------------------------------------------------------------------------------- |
| Read              | list workspaces and collections, see the tree, read requests, search, history, Reqly's docs |
| Edit requests     | create requests and folders, change, rename, move, duplicate, delete                        |
| Send and run      | send a request, see it rendered, run a collection or folder                                 |
| Environments      | read and set variables, create environments, choose the active one                          |
| Import and export | import cURL, `.http`, OpenAPI and other text, export a collection                           |

Agents get a short guide to Reqly when they connect, and the `get_docs` tool
returns any page of this help, so they do not need to look for Reqly's code.

- Changes show up in open windows right away, like changes of a
  teammate; open editors merge them.
- Deleted items go to the **trash** and can be restored.
- Sent requests are kept in the **history** like requests you send.
- An agent can send a request with another environment or extra
  variables without changing your active environment.

## Secrets

Agents never read secret values: secret variables, passwords and tokens in
auth and credential headers are shown to them as `••••`, also inside
responses (an echo of a header) and rendered requests. Values written as
`{{variable}}` are shown as they are. An agent can still set secrets; a
`••••` it sends back keeps the stored value.

## Agent calls

**Agent calls** lists the last 200 calls: time, tool, arguments (without
secrets), result and duration. The list is kept until the app restarts.

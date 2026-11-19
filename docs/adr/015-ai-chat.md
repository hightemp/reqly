# ADR-015: AI chat

- Status: accepted
- Date: 2026-11-19

## Context

PRD §7.23: a chat with a language model inside Reqly, in a collapsible
panel on the right. The model can answer questions about requests and
responses and act in the app like the user does (the same things an MCP
agent can do, ADR-014). The feature is off by default; the settings turn
it on and configure providers.

Providers speak the OpenAI Chat Completions API: OpenAI itself and the
many compatible services. The settings offer presets for OpenRouter,
Ollama Cloud and a local Ollama, plus a custom OpenAI-compatible
provider.

The chat must work on every platform: desktop, server (web UI) and
Android.

What already exists:

- The curated agent tools of `reqly-mcp`: 24 tools in 5 groups. They are
  implemented on top of registry commands through `dispatch_as`, so the
  caller's access checks apply. Secrets are masked by `Scrub`.
- The libcurl HTTP engine of `reqly-http`. It honours the network
  settings (proxy, CA, client certificates) and streams the body through
  `body_sink`.
- The incremental SSE parser of `reqly-protocols`.
- Events delivered to the windows: Tauri events on desktop, WebSocket on
  the server.

## Options

**Where the provider is called**

1. From the UI (fetch in the browser or WebView). This is simple, but
   API keys end up in the frontend. Some providers reject browser origins
   (CORS). A local Ollama is unreachable from a remote web UI.
2. From the core. Keys never leave the backend, there is no CORS, and the
   network settings apply. The server calls providers on behalf of its
   users.

**HTTP client**

1. `reqwest` (or an OpenAI SDK crate): a second HTTP stack, with its own
   proxy and TLS configuration.
2. The existing `reqly-http` engine with `body_sink`, plus `SseParser`.

**Tools**

1. Chat-only tools.
2. The MCP tool catalog, shared by both features.

## Decision

### Core: provider calls in Rust

A new crate, `reqly-ai`, holds the provider client and nothing that
depends on the app:

- `ChatRequest`: model, messages (`system`, `user`, `assistant` with
  `tool_calls`, `tool`), tools as JSON Schema functions, `stream: true`,
  optional `temperature`.
- Streaming parse of `chat/completions` SSE chunks: content deltas, tool
  call deltas assembled by index, `finish_reason`, `usage` when sent, and
  `[DONE]`.
- `GET {baseUrl}/models` gives the model list.
- Errors map to `CoreError` codes: `ai.auth` (401/403), `ai.rate_limit`
  (429), `ai.model` (404 or unknown model), `ai.provider` (other
  statuses, with the provider's message), `ai.network`.
- Transport is a trait. The app implements it with `reqly-http`
  (`execute` with a `body_sink`, the network settings and a
  `CancelToken`); tests use a fake.

### Tool catalog moves into `reqly-api`

The `Tool`, `ToolContext` and `catalog` modules move from `reqly-mcp` to
`reqly-api::agent`. They depend only on the registry and `dispatch_as`.
`reqly-mcp` keeps the protocol, the HTTP router and the gate, and uses
the moved catalog. MCP behaviour does not change. The catalog test keeps
`CATALOG` and the tools in step.

The chat needs the registry to run tools. `AppContext` gets a slot for
it, set once at startup.

### Providers and settings

The settings key `ai` (reserved: `settings.get`/`settings.set` refuse
it) holds:

```
{
  enabled: false,
  providers: [{ id, name, preset, baseUrl, headers: [{name, value}] }],
  defaultModel: { providerId, model } | null,
  tools: { [name]: bool },
  autoApprove: { [group]: bool }
}
```

- **Presets** fill the name and the base URL:

  | Preset        | Base URL                       | Key      |
  | ------------- | ------------------------------ | -------- |
  | `openai`      | `https://api.openai.com/v1`    | required |
  | `openrouter`  | `https://openrouter.ai/api/v1` | required |
  | `ollamaCloud` | `https://ollama.com/v1`        | required |
  | `ollama`      | `http://localhost:11434/v1`    | none     |
  | `custom`      | entered by the user            | optional |

  OpenRouter also gets the `HTTP-Referer` and `X-Title` headers that
  identify the app.

- **API keys** are stored apart from the config, in the reserved key
  `ai.keys` (`{ [providerId]: key }`). No command returns them. The
  config reports `hasKey` per provider; `ai.setKey` writes or clears a
  key. They are stored like environment secrets, in the database. OS
  keychain storage is a separate open question (Q6).
- **Models:** `ai.models` lists a provider's models through `/models`.
  A model can also be typed in, since some services do not list
  everything.
- **Who configures:** on desktop and in the server modes `none`/`token`,
  whoever can open the settings. In `multi`, only an administrator
  changes providers, keys and the tool settings. Users see the provider
  names and models and pick one.

### Chats

New tables (migration):

- `ai_chats`: `id`, `workspace_id`, `owner` (user id in `multi`, null
  otherwise), `title`, `provider_id`, `model`, `auto_approve` (per-chat
  "don't ask again"), timestamps, `deleted_at`.
- `ai_messages`: `id`, `chat_id`, `seq`, `role`, `content`,
  `tool_calls` (JSON), `tool_call_id`, `tool_name`, `status`
  (`done`/`error`/`cancelled`/`pending`), `error`, `usage` (JSON),
  `created_at`.

Chats belong to a workspace. In `multi` each user sees only their own
chats: the commands filter by owner and the access rules check
membership of the workspace. Chats are not part of desktop ↔ server
sync and are not exported. The title is the first words of the first
message, and it can be renamed.

Commands:

- `ai.config`, `ai.setConfig`, `ai.setKey`, `ai.models`
- `ai.chats`, `ai.chat`, `ai.createChat`, `ai.renameChat`,
  `ai.deleteChat`, `ai.setChatModel`
- `ai.send`, `ai.cancel`, `ai.confirm`

### A turn

`ai.send { chatId, text, attachments }` stores the user message,
returns at once and runs the turn in a task:

1. **Prompt.** The system prompt describes Reqly and the tools, the
   current workspace and its collections (names and ids), and asks the
   model to answer in the user's language.
   - Attachments add masked context to the user message: the current
     request (`get_request`) and its last response (`get_response`,
     body cut to 16 KB).
   - Older tool results are sent shortened (2 KB each) to keep the
     context small. The full results stay in the database.
2. **Stream.** Deltas are published as `ai.delta { chatId, messageId,
text }`.
3. **Tool calls.** For each call, an `ai.toolCall` event carries the
   name, the arguments and the state.
   - Tools of the `read` group and the read-only tools (`render_request`,
     `export_collection`) run at once.
   - Other tools wait for `ai.confirm { chatId, callId, allow, always }`,
     unless their group is auto-approved in the settings or the chat
     has "don't ask again".
   - A denied call returns "the user declined" to the model.
   - Tools run as the chat's caller (`Caller::User` in `multi`), so
     roles apply. Results are masked like MCP results.
4. **Loop.** The loop repeats until the model answers without tool
   calls, up to 20 steps per turn. Past that, the turn ends with a
   notice.
5. **End.** `ai.done { chatId, status, usage }`. `ai.cancel` stops the
   stream (`CancelToken`) and any pending confirmation, and keeps what
   was received with the status `cancelled`.

`ai.*` events are visible only to the chat's owner (`event_visible`).
On restart, turns still `pending` become `cancelled`.

Only enabled tools are offered to the model. The AI tool settings are
separate from MCP: same catalog, own choices. All groups are on by
default, and confirmation protects the changing ones. `autoApprove` is
off for every group.

### UI

- **Right panel.** A collapsible `SplitPane` on the right of the main
  area. Its width and its state are saved in the UI prefs. A status-bar
  button and the hotkey `Mod+Shift+L` open it. The panel and the button
  exist only while AI is enabled.
  - Compact layout (Android, narrow windows): a fourth pane, "AI", in
    `CompactBar`.
- **Panel contents:**
  - the chat list (new, rename, delete) and the model picker;
  - the messages, rendered as markdown with highlighted code blocks and
    a copy button;
  - the input with an "attach current request/response" chip;
  - "Stop" while a turn runs;
  - tool calls as collapsible cards with the arguments and the result,
    and "Allow" / "Deny" / "Always in this chat" when they wait.
- **Settings section "AI":**
  - the switch;
  - providers (add from a preset, base URL, key with "set"/"clear",
    extra headers, "Check connection" through `/models`);
  - the default model;
  - tools by group, with auto-approve per group.
- Help page `ai` (en, ru).

## Consequences

- One implementation serves all platforms. Keys stay in the backend,
  and provider calls use the network settings (proxy, CA) like requests
  do.
- MCP and the chat share tools, masking and access checks. A new tool
  appears in both settings lists.
- On a server, provider traffic and its cost go through the server and
  its admin-configured keys. Per-user keys can come later.
- Context management is simple (shortened old tool results, no
  summarisation). Very long chats may exceed small context windows; the
  provider error is shown and a new chat helps.
- The model sees masked secrets only, but request bodies, URLs and
  response bodies it reads are sent to the provider. The settings
  section and the help page say so.
- Only the Chat Completions API is supported. Responses API, vision and
  file inputs are out of scope.

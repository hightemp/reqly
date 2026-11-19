# AI chat

A chat with a language model in a panel on the right of the window. The
model answers questions about requests and responses and can act in Reqly
with the same tools as the [MCP server](mcp.md). Design: ADR-015.

The chat is **off by default**: **Settings → AI → AI chat**.

## Providers

Any service with the OpenAI Chat Completions API (streaming and tool
calling). Presets fill the name and the base URL:

| Preset            | Base URL                                   | API key  |
| ----------------- | ------------------------------------------ | -------- |
| OpenAI            | `https://api.openai.com/v1`                | required |
| OpenRouter        | `https://openrouter.ai/api/v1`             | required |
| Ollama Cloud      | `https://ollama.com/v1`                    | required |
| Ollama (local)    | `http://localhost:11434/v1`                | —        |
| OpenAI-compatible | any (LM Studio, vLLM, LiteLLM, a gateway…) | optional |

- **Keys** are write-only: no command returns them, the settings show only
  whether one is set. They are stored in the database (settings key
  `ai.keys`), like secret environment variables.
- **Extra headers** are sent with every call (a gateway's own key, …).
  OpenRouter also gets `HTTP-Referer` and `X-Title`.
- **Check connection** lists the models (`GET /models`); a model can also be
  typed in.
- Calls are made by the core, never by the browser: the **proxy, TLS and CA
  certificate** settings of _Requests & proxy_ apply, and on a server its
  network policy too. Cookies are never sent to a provider.

On a multi-user server only administrators change providers, keys and the
tool settings; users see the providers (without their headers) and pick a
model. Provider traffic and its cost go through the server's keys.

## Chats

- Chats belong to a workspace; on a multi-user server each user sees only
  their own (administrators included). They are stored in the database
  (`ai_chats`, `ai_messages`), are not synced between desktop and server
  and are not exported.
- The first message titles the chat; chats can be renamed and deleted.
- Each chat has its model; new chats start with the default model.
- The current request and its last response can be attached: the model
  gets them as JSON (masked, bodies cut to 16 KB).

## A turn

`ai.send` stores the message and runs the turn; it returns when the turn
ends (`done`, `cancelled`, `error`). Meanwhile:

- the answer streams as `ai.delta` events (text, reasoning, the tool being
  called) and `ai.message` events (each stored message as it changes);
- the model may call tools, up to **20 model calls** per turn;
- **reading** tools (and `render_request`, `export_collection`) run at once;
  others wait: an `ai.confirm` event, answered with `ai.confirm`
  (`allow`, `allowAlways` — for the rest of the chat — or `deny`), unless
  their group runs **without asking** (_Settings → AI → Tools_);
- `ai.cancel` stops the stream and any waiting call and keeps what arrived;
- `ai.done` ends the turn.

Tools run as the user who chats, so workspace roles apply; results are
masked like MCP results (secret variables and credentials appear as
`••••`). Failed answers stay in the chat with the provider's message and
are not sent back to the model. Tool results of earlier turns are sent
shortened (2 KB each) to keep the context small.

All `ai.*` events except `ai.configChanged` go only to the chat's owner.

## Commands

| Command                                                                                     | Who                      |                                                |
| ------------------------------------------------------------------------------------------- | ------------------------ | ---------------------------------------------- |
| `ai.config`                                                                                 | everyone                 | providers (no keys), tools, presets, `canEdit` |
| `ai.setConfig`, `ai.setKey`                                                                 | administrators           | settings, keys                                 |
| `ai.models`                                                                                 | everyone                 | models of a provider                           |
| `ai.chats`, `ai.chat`, `ai.createChat`, `ai.renameChat`, `ai.deleteChat`, `ai.setChatModel` | viewers of the workspace | own chats                                      |
| `ai.send`, `ai.cancel`, `ai.confirm`                                                        | viewers of the workspace | turns                                          |

Errors: `ai.auth` (401/403), `ai.rate_limit` (429), `ai.model` (404),
`ai.provider` (other statuses), `ai.network`, `ai.cancelled`,
`ai.disabled`, `ai.no_model`, `ai.busy`, `ai.step_limit`,
`ai.tool_unavailable`; provider errors carry `status` and the provider's
`message`.

## Privacy

Messages, and the requests and responses the model reads through tools or
attachments, are sent to the provider. Secret values are masked. A local
model (Ollama) keeps everything on the machine.

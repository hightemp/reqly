# AI chat

A chat with a language model in a panel on the right. Ask about your
requests and responses — why a call fails, what a header means, how to
write a test — or tell it what to do: the model can read and change your
requests, folders and environments, send requests and run collections.

The chat is **off by default**.

## Turning it on

1. **Settings → AI → AI chat**.
2. **Add provider** and pick a preset (see below).
3. **Check connection** loads the provider's models.
4. Choose the **Default model**: pick one from the list or type its name.

Presets:

- **OpenAI**, **OpenRouter**, **Ollama Cloud**: paste the API key.
- **Ollama (local)**: no key; Ollama must be running
  (`http://localhost:11434`).
- **OpenAI-compatible**: any other service with an OpenAI-style
  `chat/completions` API (LM Studio, vLLM, LiteLLM, a company gateway…);
  enter its base URL, a key if it needs one and extra headers.

Keys are stored by Reqly and never shown again: the settings only say
whether a key is set. Provider calls use the proxy and certificate
settings of **Requests & proxy**.

On a multi-user server only administrators set up providers and tools;
everyone else picks a model and chats.

## The panel

Open it with the **AI** button in the status bar or `Mod+Shift+L`. On a
phone it is the **AI** tab at the top.

- **Chats** belong to the workspace; start a new one with **+**, rename or
  delete them from the list. On a multi-user server each user sees only
  their own chats.
- The **model** of the chat is chosen above the input.
- Under a message: **Copy message** (its Markdown text) and, under an
  answer, **Branch off from here** — a new chat with the messages up to
  that answer, to try another direction without losing this one.
- **Enter** sends, **Shift+Enter** adds a line. **Stop** ends the answer
  and keeps what arrived.
- **Attach the current request** (and its last response) to ask about it
  without copying anything.

## Tools and approvals

When the model uses a tool, the chat shows a card with the arguments and
the result.

- **Reading** (the tree, a request, history, responses) runs at once.
- **Changing** (creating, editing, moving, deleting requests), **sending
  requests**, **environments** and **import** wait for you: **Allow**,
  **Deny**, or **Always in this chat**.
- In **Settings → AI → Tools** you choose which tools the chat may use and
  which groups run **without asking**.

The model sees only what tools return: secret variables and credentials
are masked as `••••`. Deleted items go to the trash. On a multi-user
server tools run with your role: a viewer cannot change anything.

## Privacy

Your messages, and the requests and responses the model reads, are sent
to the provider you chose. Use a local model (Ollama) to keep everything
on your computer.

# Getting started

Reqly is an API client: build HTTP, GraphQL, WebSocket, SSE and gRPC
requests, send them and inspect the responses.

## How things are organized

- A **workspace** holds collections, global environments and cookies. Switch
  workspaces in the sidebar header.
- A **collection** holds folders and requests, its own environments and
  settings. Pick it in the sidebar.
- **Folders** can be nested. Headers, auth, variables, scripts and network
  settings of a folder apply to everything inside it.

## Your first request

1. Press **Create → New request** (or `Ctrl+N`).
2. Type the URL, e.g. `https://httpbin.org/get`, and pick the method.
3. Press **Send** (or `Ctrl+Enter`).

The response shows the status, time and size, the body (pretty, raw or a
preview), headers, cookies and the timeline. See [Requests](help:requests).

Pasting a `curl` command or HTTP request text into the URL bar turns it into
the request. To bring in many requests at once, see
[Import and export](help:import-export).

## Finding things

- **Command palette** (`Ctrl+K`): search requests by name or URL, or type `>`
  for commands.
- **Search requests** (`Ctrl+Shift+F`) filters the tree.
- The sidebar switches between the **tree** and **folders + request list**
  views.

## Organizing requests

Drag and drop requests and folders, select several with `Ctrl` or `Shift`,
cut, copy, paste and duplicate them. The context menu (right click) has all
actions. Deleted items go to the **Trash** (command palette) and can be
restored; **Undo** in the notification brings them back right away.

Tabs work like in a browser: a single click opens a preview tab, editing or
a double click keeps it. Tabs can be pinned; `Ctrl+Shift+T` reopens a closed
tab.

## Saving

Changes are saved automatically. Turn this off in **Settings → General** to
save with `Ctrl+S`.

## Where to go next

- [Variables and environments](help:variables)
- [Scripts](help:scripts) and [Tests](help:testing)
- [Keyboard shortcuts](help:shortcuts)

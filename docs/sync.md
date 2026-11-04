# Sync

The desktop and Android apps can keep workspaces in sync with a
[Reqly server](server.md): edit offline, and changes go to the server when
it is reachable. Other apps connected to the same server, and the server's
web UI, see them.

## Connecting

Settings → **Sync**:

- **Server URL**: the address of the server, e.g. `https://reqly.example.com`.
- A server in `multi` mode: your **user name** and **password**. Reqly signs
  in once and keeps a personal API token named "Reqly sync (<computer>)";
  the password is not stored. You can revoke the token in the web UI
  (Settings → Account → API tokens).
- A server in `token` mode: its **token**. A personal API token works too,
  instead of a user name and password.
- A server without authentication: just the URL.

Connecting to another server starts over: the workspaces stop syncing with
the old one and stay in the app.

## Workspaces

The list shows the workspaces of this device and those of the server (the
ones you are a member of). Tick a workspace to sync it:

- a workspace only on this device is **uploaded** and belongs to you on the
  server (you can then invite people to it);
- a workspace only on the server is **downloaded**.

Unticking stops syncing; the workspace stays on this device and on the
server. **Disconnect** stops syncing everything.

## When it syncs

At connect, every 30 seconds, a couple of seconds after a change, and on
**Sync now** (settings, command palette). The cloud in the status bar shows
the state: synced, changes waiting, syncing, failed (hover for the reason) or
conflicts. Settings → Sync lists the recent syncs that did something.

Without a connection everything keeps working; changes wait and go out on
the next sync.

## What syncs

Workspaces, collections (with their settings and scripts), folders,
requests and environments. History, cookies, OAuth 2.0 tokens, GraphQL
schemas and app settings stay on each device. A file chosen as a request
body stays on the device it was chosen on.

**Secrets**: values of secret environment variables stay on the device
unless **Sync secret values** is on; then they travel to the server (use
HTTPS) and are encrypted at rest on each side.

## Changes on both sides

Changes merge field by field: a request renamed on one device and given a
new header on another keeps both. When the same field changed on both
sides, the URL, the body, the GraphQL query and variables, the description
and scripts merge both edits (as in live collaboration); for other fields
the change that reaches the server last wins.

An item **deleted on one side and changed on the other** is a conflict: the
status bar shows it, and **Resolve…** lets you keep your version (it is
restored, or deleted on the server) or take the server's.

In a workspace where you are a viewer, local changes cannot be sent: they
are replaced by the server's version.

## How it works

See [ADR-013](adr/013-desktop-server-sync.md): the server numbers every
change; apps pull what changed since their last sync, then push their own
changes with the version they were based on, and the server merges them.

# ADR-010: Multi-user server and command authorization

- Status: accepted
- Date: 2026-10-18

## Context

Stage 7 adds the `multi` auth mode (PRD §7.17): users, sessions, API
tokens, workspace membership with the roles `owner`, `editor` and `viewer`.
The UI talks to the core through ~100 typed commands (`POST /api/v1/cmd/{name}`)
that take `&AppContext` and know nothing about users. Desktop and the
`none`/`token` server modes must keep working unchanged.

Every command has to be authorized by role, and the rule must hold for
commands added later without anyone remembering to add a check.

## Options

1. Pass a per-request context with the user to every command. Touches every
   command signature; desktop has to invent a user.
2. Annotate each command with its access rule (`const ACCESS`) and resolve
   the workspace in the command itself. Easy to forget; the rule lives in
   ~100 places.
3. Authorize in front of the registry: derive the workspaces a call touches
   from the ids in its input, look up the caller's role, and compare with
   the permission the command needs. Commands that list across workspaces
   read the caller from a task-local.

## Decision

Option 3, in `reqly-api` (`access.rs`), so it is tested without HTTP:

- `Caller` is `Local` (desktop, `none` and `token` modes: full access) or
  `User { id, admin }`. `commands::dispatch_as` runs a command under
  `access::scope`, a `tokio::task_local`.
- **Workspaces of a call.** Every top-level input field named `id`, `ids`,
  `*Id` or `*Ids` (and those of an export `scope`) is an entity id. Nested
  request data is not looked at: header rows carry ids of their own. Ids are random UUIDs, so one query
  over the tables that belong to a workspace (workspaces, collections,
  nodes, environments, response records, cookies) maps each to its
  workspace. An id that maps to nothing fails with `not_found`, the same
  answer as for a workspace the caller cannot see. Fields that are handles
  rather than entities (`connectionId`, `flowId`) are skipped; those
  objects remember the user that created them.
- **Permission of a command** comes from a table: `read` (viewer),
  `write` (editor), `manage` (owner: members, workspace delete/rename),
  `global` (no workspace: settings, parsing, codegen targets),
  `admin`. A command missing from the table needs `admin`, so a new command
  is closed until classified; a test fails when the table and the registry
  differ.
- Sending requests, running scripts and reading history are `read`:
  viewers can use a collection without changing it.
- Administrators act as owners of every workspace; `workspace.list` still
  shows only their own, the admin panel lists all.
- Listing commands (`workspace.list`, `workspace.ensureDefault`) and
  `workspace.create` (which makes the caller the owner) read the caller.
  Settings of a user are stored under `user:<id>:<key>` and fall back to
  the server-wide value; the active environment is such a setting, so
  switching it affects only the caller. Stream connections belong to the
  user who opened them.
- Passwords are hashed with argon2id (`reqly-storage::users`). API tokens
  and session tokens are random and stored as SHA-256. After 5 failed
  logins an account is locked for 15 minutes, in addition to the per-IP
  limit.
- The first administrator comes from `REQLY_ADMIN_USERNAME` and
  `REQLY_ADMIN_PASSWORD` when there are no users, or from
  `reqly-server admin create`.

## Consequences

- New commands are safe by default (admin only) and the access test makes
  the author classify them.
- Ids from input are trusted only after the lookup, so a crafted input
  that mixes ids of two workspaces needs the role in both.
- Push events are filtered the same way (`access::event_visible`): events
  about shared content (tree, collections, environments, cookies, runtime
  variables) go to the members of the workspace their ids belong to;
  `workspaces.changed` carries nothing and goes to everyone; every other
  event goes only to the user who caused it (`Event::actor`, taken from
  the task-local caller). An event without an actor reaches no user.
- PostgreSQL stays open (Q5, T7.1.6).

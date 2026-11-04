# ADR-013: Desktop ↔ server sync

- Status: accepted
- Date: 2026-11-04

## Context

PRD §7.18: the desktop app connects to a server, picks workspaces and keeps
them in sync, offline first, with the merge rules of collaboration (field
level last-writer-wins, text merges) and a dialog for conflicts that cannot
be merged (delete against change). Secrets stay local unless asked.

Entities already carry UUIDv7 ids, versions, soft delete and fractional
order (ADR-004), so a record made offline on any client is unique.

## Decision

### Records

Workspaces, collections, environments and folders/requests sync as
**records**: the kind, the id, the workspace, a `deleted` flag and a JSON
object of synced fields (name, parent, order, data…). Local-only columns
(version, timestamps, field versions) are not part of a record. History,
cookies, OAuth 2 tokens, GraphQL schemas and settings do not sync.

### Server: change sequence

Every write to a synced table gets the next value of a global **change
sequence** (`sync_seq`, set by SQLite triggers, so no write path can forget
it). SQLite serializes writers, so sequence order is commit order:

- `sync.pull` takes a cursor per workspace and returns the records of those
  workspaces changed after it (in sequence order, in pages) and the new
  cursors.
- `sync.push` takes changes: the record as the client has it and the
  **base**, the record as the client last got it from the server. The
  server merges each change into its current record (below), writes it and
  returns the result, or a conflict.

The server is the single order of changes, so clients converge on it.

### Client: base copies instead of an operation log

The client keeps the base of every synced record (`sync_base`: the record
and the local `sync_seq` it was written at). A record is changed locally
when its `sync_seq` is newer than the base, new when it has no base, and
deleted when the base has no row any more. This is the operation journal of
the PRD in a coalesced form: it needs no hook in any write path, survives a
crash between a write and a sync, and an offline change made many times is
sent once.

A sync round is **pull, then push**:

1. Pull. For every incoming record: unchanged locally → apply it; changed
   locally → merge the local record onto the incoming one (rebase) and keep
   the result as a local change; then the incoming record becomes the base.
2. Push the local changes with their bases; apply what the server returns
   as the new base. A change that met a newer record on the server (another
   client pushed in between) comes back as `stale`, and the next round
   pulls and rebases it.

### Merge

`merge(base, ours, theirs)` per field, recursively into objects (request
data, body, scripts):

- changed on one side only → that side;
- changed on both sides:
  - the **text fields** of collaboration (`url`, `description`, `body.text`,
    the GraphQL query and variables, scripts) merge both edits as Yjs
    edits of the base text, as the server does for live documents (ADR-012),
    so edits in different places both survive;
  - anything else → **ours** (the change being applied: the push on the
    server, the local change on the client) — last writer wins.

Deleted on one side and changed on the other is a **conflict**. It is not
merged: the client keeps it aside (`sync_conflicts`), stops sending that
record and asks the user: keep the local version (it is pushed with
`force`, restoring or overwriting the server record) or take the server's.

### Secrets

The values of secret environment keys are not sent and incoming records
never replace them, unless the user turns on secret sync. Then they travel
in plain text over the connection (HTTPS) and are encrypted at rest by each
side with its own key.

### Connection

The desktop app stores the server URL and a personal API token (made by
signing in with a user name and password, or pasted). Sync runs on connect,
every 30 seconds, shortly after a local change, and on "Sync now". Picking
a local workspace uploads it; picking a server workspace downloads it.

## Consequences

- Sync works with any server mode; in multi-user mode pull and push follow
  workspace roles (ADR-010), and a pushed new workspace belongs to its
  author.
- A record changed on both sides converges after one more round.
- Two clients inserting at the same place may get equal sort keys; order is
  then decided by id (ADR-004).
- File bodies are references to local blobs: the reference syncs, the file
  does not.

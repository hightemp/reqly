# ADR-012: Collaborative editing of text fields

- Status: accepted
- Date: 2026-10-18

## Context

PRD §7.17 asks for real-time co-editing of the text fields of a request
(URL, body, scripts, description) with the cursors of other people, using
Yjs on the client (`y-codemirror.next`) and `yrs` on the server. Structural
fields already merge per field (T7.2.2). Requests are sent from the node
data in the database, so the text must end up there too.

## Decision

- **One Yjs document per text field** of a node, addressed by the node id
  and a dotted path into its data (`url`, `body.text`, `description`,
  `scripts.preRequest`…). The document holds one `Y.Text` named `t`.
- **The server owns the documents** (`collab` in `AppContext`, `yrs`). The
  first `collab.join` creates the document from the current field value
  under a fresh `docId`; later joins get the full state. Clients never
  insert the initial text themselves, so two clients cannot duplicate it.
- **Transport**: commands carry client updates (`collab.update`, base64 v1
  updates) and awareness (`collab.awareness`, relayed as is); the server
  pushes them to the other members as `collab.update` and
  `collab.awareness` events over the existing WebSocket. An update for an
  unknown `docId` (server restart, evicted document) fails with
  `collab.stale`, and the client joins again.
- **The node stays the source of truth for sending**: after each update the
  server writes the text into the node (field-level, as T7.2.2). When a
  field changes another way (API, import, a client without collaboration),
  the server applies the difference to the live document as a Yjs edit, so
  everybody converges on it. A `node.update` that sends a whole object
  containing a live field (the body, to change its kind) keeps the live
  text; a field sent on its own (`url`) is an explicit change and wins.
  Text written from a document has no author, so it never triggers
  overwrite notifications (T7.2.2).
- **Access**: joining and awareness need `read`, updates need `write`;
  events go to the members of the workspace (ADR-010).
- Documents are dropped after 10 minutes without joins or updates.
- Only on a multi-user server; desktop and single-user servers keep plain
  editors.
- The UI binds the body, the GraphQL query and variables and the scripts.
  The URL bar shows a text built from the URL and the query parameters
  rather than the stored URL, and the description is a plain text area, so
  both stay with field-level merging (T7.2.2) for now.

## Consequences

- Concurrent typing in the same field merges character by character, with
  remote cursors and per-user undo (`Y.UndoManager`).
- The editor store must not send these fields itself while they are bound
  to a document.
- Yjs history lives only in memory: after a restart or eviction the
  document starts again from the stored text.
- Updates of one client must arrive in the order it made them: `yrs` 0.28
  can lose an edit when updates of the same client arrive out of order (a
  property test found it). Clients send updates one at a time, and HTTP and
  the WebSocket keep the order; the property test checks convergence under
  any interleaving of different senders.

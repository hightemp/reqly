# ADR-004: Ids, versions, soft delete and item order

- Status: accepted
- Date: 2026-10-17

## Context

Stages 7 (collaboration) and 10 (sync) need entities that can be created
offline on different clients, compared by version and reordered without
conflicts. Changing the data model later is expensive.

## Decision

Every syncable entity has `Meta` (`reqly-core::meta`):

- `id`: **UUIDv7**. Generated on the client, globally unique and time-ordered
  (good for SQLite indexes).
- `created_at` / `updated_at`: Unix milliseconds (UTC) from a `Clock`
  (tests use `FixedClock`).
- `version`: a monotonic change counter of the entity. It grows on every
  change, including deletion and restore.
- `deleted_at`: soft delete (trash, syncing deletions).
- `sort_key`: a **fractional index** (`reqly-core::sort_key`). A base-62
  string without a trailing zero, compared byte by byte. A new key always
  exists between any two keys, so moving an item only changes its own row.
  - Inserting at the end or start increments or decrements the last digit
    (the length grows by about one character per 60 inserts).
  - Inserting between two keys takes the midpoint.
  - `n_between` spreads a group insert evenly (used by group drag and drop).

## Consequences

- Moving an item in the tree writes `O(1)` rows, without renumbering siblings.
- Concurrent inserts at the same place on two clients can produce the same
  key. Order is then decided by `id`, and sync regenerates the key.
- Keys may slowly grow longer. If needed, a collection can be "compacted"
  (keys regenerated); that is a separate operation that bumps versions.

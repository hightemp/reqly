# ADR-011: SQLite for the multi-user server

- Status: accepted
- Date: 2026-10-18
- Answers: PRD Q5

## Context

PRD Q5 asks whether the multi-user server needs PostgreSQL. The storage
layer is sqlx on SQLite with SQLite-specific SQL (STRICT tables, FTS5
search, `ON CONFLICT` upserts). T7.1.6 asked for a load test of SQLite in
`multi` mode before deciding.

## Load test

`crates/reqly-api/tests/load.rs` (ignored by default). Every simulated user
works without pauses in their own workspace and in one workspace shared by
all: 70% reads (`tree.list`, `node.get`), 25% `node.update`, 5%
`node.create`, all through `dispatch_as`, so authorization lookups are
included. The database is a file in WAL mode, as on the server.

The first run found a real defect: with 20 users most writes failed with
`database is locked` (`SQLITE_BUSY`, `SQLITE_BUSY_SNAPSHOT`). Deferred
transactions read first and then fail at once when they try to write after
another connection committed; the busy timeout does not apply to that case.
Fixed by:

- `BEGIN IMMEDIATE` for write transactions (`Storage::begin_write`);
- an async write lock taken before a pooled connection, so waiting writers
  do not hold the connections readers need;
- `synchronous=NORMAL` (safe with WAL: a power loss may drop the last
  commits, never corrupts the database).

Results after the fix, release build, 10 s per run, on a developer
machine shared with other work:

| Users | ops/s | Reads p50 / p99 | Writes/s | Writes p50 / p99 | Errors |
|---|---|---|---|---|---|
| 20 | 1588 | 3.0 / 46 ms | 471 | 20 / 58 ms | 0 |
| 50 | 1636 | 2.6 / 48 ms | 497 | 78 / 155 ms | 0 |
| 100 | 1690 | 2.6 / 50 ms | 512 | 178 / 297 ms | 0 |

Writes are serialized at about 500 per second, so write latency grows with
the number of writers that act at the same moment; reads stay flat. The
simulated users write continuously; a person editing requests saves at most
a few times a second (autosave is debounced by 500 ms), so ~500 writes/s
covers hundreds of people editing at the same time.

## Options

1. Stay on SQLite, tuned as above.
2. Add PostgreSQL as an option: a second SQL dialect for every repository,
   FTS5 replaced by `tsvector`, migrations twice, CI for both.
3. Move to PostgreSQL only: loses the zero-setup single binary and the
   shared code path with the desktop app.

## Decision

Option 1. SQLite stays the only database of the server. PostgreSQL is
reconsidered when one of these becomes a requirement: several server
instances behind a load balancer, high availability with failover, or
measured write p99 above 200 ms in production.

## Consequences

- One storage implementation for desktop and server; backups are a file
  copy (`VACUUM INTO`).
- All multi-statement writes must go through `Storage::begin_write`; a
  regression test (`crates/reqly-storage/tests/concurrency.rs`) runs
  concurrent writers on a database file.
- A single server instance is the scaling limit; the load test is the tool
  to check it again when the workload changes.

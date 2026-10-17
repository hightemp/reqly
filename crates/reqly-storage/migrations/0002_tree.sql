-- Workspaces → collections → tree of folders/requests (see ADR-004 for meta columns).

CREATE TABLE workspaces (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL,
    sort_key   TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    version    INTEGER NOT NULL,
    deleted_at INTEGER
) STRICT;

CREATE TABLE collections (
    id           TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    settings     TEXT NOT NULL,
    sort_key     TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    version      INTEGER NOT NULL,
    deleted_at   INTEGER
) STRICT;

CREATE INDEX collections_by_workspace ON collections (workspace_id, sort_key);

CREATE TABLE nodes (
    id            TEXT PRIMARY KEY NOT NULL,
    collection_id TEXT NOT NULL REFERENCES collections (id) ON DELETE CASCADE,
    parent_id     TEXT REFERENCES nodes (id) ON DELETE CASCADE,
    kind          TEXT NOT NULL CHECK (kind IN ('folder', 'request')),
    name          TEXT NOT NULL,
    -- Denormalized from `data` for listing, sorting and search.
    method        TEXT,
    url           TEXT,
    data          TEXT NOT NULL,
    sort_key      TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    version       INTEGER NOT NULL,
    deleted_at    INTEGER
) STRICT;

CREATE INDEX nodes_tree ON nodes (collection_id, parent_id, sort_key);
CREATE INDEX nodes_parent ON nodes (parent_id);

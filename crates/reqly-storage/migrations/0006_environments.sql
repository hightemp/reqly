CREATE TABLE environments (
    id            TEXT PRIMARY KEY NOT NULL,
    workspace_id  TEXT NOT NULL,
    collection_id TEXT REFERENCES collections (id) ON DELETE CASCADE,
    kind          TEXT NOT NULL CHECK (kind IN ('global', 'base', 'sub')),
    name          TEXT NOT NULL,
    color         TEXT,
    -- JSON object; values of secret keys are encrypted ("enc:v1:…").
    data          TEXT NOT NULL,
    secrets       TEXT NOT NULL,
    sort_key      TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    version       INTEGER NOT NULL,
    deleted_at    INTEGER
) STRICT;

CREATE INDEX environments_by_collection ON environments (collection_id, sort_key);
CREATE INDEX environments_by_workspace ON environments (workspace_id, kind);

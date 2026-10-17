CREATE TABLE response_records (
    id               TEXT PRIMARY KEY NOT NULL,
    request_id       TEXT NOT NULL,
    workspace_id     TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    status           INTEGER,
    status_text      TEXT NOT NULL,
    http_version     TEXT NOT NULL,
    headers          TEXT NOT NULL,
    body_hash        TEXT,
    body_size        INTEGER NOT NULL,
    headers_size     INTEGER NOT NULL,
    content_type     TEXT,
    timings          TEXT NOT NULL,
    timeline         TEXT NOT NULL,
    request_snapshot TEXT NOT NULL,
    redirects        INTEGER NOT NULL,
    final_url        TEXT NOT NULL,
    remote_addr      TEXT,
    error            TEXT,
    -- Denormalized for the history list and workspace log search.
    method           TEXT NOT NULL,
    url              TEXT NOT NULL
) STRICT;

CREATE INDEX response_records_by_request ON response_records (request_id, created_at DESC);
CREATE INDEX response_records_by_workspace ON response_records (workspace_id, created_at DESC);
CREATE INDEX response_records_by_time ON response_records (created_at);

CREATE TABLE cookies (
    id                 TEXT PRIMARY KEY NOT NULL,
    workspace_id       TEXT NOT NULL,
    domain             TEXT NOT NULL,
    include_subdomains INTEGER NOT NULL,
    path               TEXT NOT NULL,
    secure             INTEGER NOT NULL,
    http_only          INTEGER NOT NULL,
    expires            INTEGER,
    name               TEXT NOT NULL,
    value              TEXT NOT NULL,
    updated_at         INTEGER NOT NULL,
    UNIQUE (workspace_id, domain, path, name)
) STRICT;

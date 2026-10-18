-- Users of the multi-user server (ADR-010).
CREATE TABLE users (
    id            TEXT PRIMARY KEY NOT NULL,
    username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name  TEXT NOT NULL,
    -- argon2id PHC string.
    password_hash TEXT NOT NULL,
    is_admin      INTEGER NOT NULL DEFAULT 0,
    disabled      INTEGER NOT NULL DEFAULT 0,
    failed_logins INTEGER NOT NULL DEFAULT 0,
    locked_until  INTEGER,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
) STRICT;

-- Personal API tokens (CLI, sync). Only a SHA-256 of the token is stored.
CREATE TABLE api_tokens (
    id           TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER,
    expires_at   INTEGER
) STRICT;

CREATE INDEX api_tokens_by_user ON api_tokens (user_id);

CREATE TABLE memberships (
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role         TEXT NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (workspace_id, user_id)
) STRICT;

CREATE INDEX memberships_by_user ON memberships (user_id);

-- Sessions end when their user is deleted.
CREATE INDEX sessions_by_user ON sessions (user_id);

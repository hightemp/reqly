-- Invitation links of the multi-user server (ADR-010). Only a SHA-256 of
-- the token is stored. A workspace invitation also grants a role there.
CREATE TABLE invitations (
    id           TEXT PRIMARY KEY NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,
    workspace_id TEXT REFERENCES workspaces (id) ON DELETE CASCADE,
    role         TEXT CHECK (role IN ('owner', 'editor', 'viewer')),
    created_by   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL,
    used_by      TEXT REFERENCES users (id) ON DELETE SET NULL,
    used_at      INTEGER
) STRICT;

CREATE INDEX invitations_by_workspace ON invitations (workspace_id);

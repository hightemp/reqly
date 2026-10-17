-- Login sessions of the server build. Only a hash of the session token is stored.
CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY NOT NULL,
    user_id    TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
) STRICT;

CREATE INDEX sessions_by_expiry ON sessions (expires_at);

-- OAuth 2.0 tokens per node (request or folder) that defines the auth.
CREATE TABLE oauth2_tokens (
    node_id       TEXT PRIMARY KEY NOT NULL,
    -- Hash of the settings the token was obtained with; a mismatch means stale.
    config_hash   TEXT NOT NULL,
    -- Encrypted JSON: access, refresh and id tokens and the raw response.
    secret        TEXT NOT NULL,
    token_type    TEXT NOT NULL,
    scope         TEXT NOT NULL,
    expires_at    INTEGER,
    created_at    INTEGER NOT NULL
) STRICT;

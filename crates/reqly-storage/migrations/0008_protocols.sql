-- Request protocol for tree badges (http, graphql, sse, webSocket, grpc).
ALTER TABLE nodes ADD COLUMN protocol TEXT;
UPDATE nodes SET protocol = 'http' WHERE kind = 'request';

-- GraphQL schema per request: an introspection result (JSON) or SDL from a file.
CREATE TABLE graphql_schemas (
    node_id     TEXT PRIMARY KEY NOT NULL,
    -- 'introspection' or 'sdl'
    format      TEXT NOT NULL,
    source      TEXT NOT NULL,
    schema      TEXT NOT NULL,
    fetched_at  INTEGER NOT NULL
) STRICT;

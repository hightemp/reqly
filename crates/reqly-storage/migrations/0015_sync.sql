-- Desktop <-> server sync (ADR-013).

-- The change sequence: every write to a synced table takes the next value.
CREATE TABLE sync_counter (
    id  INTEGER PRIMARY KEY CHECK (id = 1),
    seq INTEGER NOT NULL
) STRICT;
INSERT INTO sync_counter (id, seq) VALUES (1, 1);

ALTER TABLE workspaces ADD COLUMN sync_seq INTEGER NOT NULL DEFAULT 1;
CREATE INDEX workspaces_by_sync_seq ON workspaces (sync_seq);

CREATE TRIGGER workspaces_sync_insert AFTER INSERT ON workspaces BEGIN
    UPDATE sync_counter SET seq = seq + 1 WHERE id = 1;
    UPDATE workspaces SET sync_seq = (SELECT seq FROM sync_counter WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER workspaces_sync_update AFTER UPDATE ON workspaces WHEN NEW.sync_seq IS OLD.sync_seq BEGIN
    UPDATE sync_counter SET seq = seq + 1 WHERE id = 1;
    UPDATE workspaces SET sync_seq = (SELECT seq FROM sync_counter WHERE id = 1) WHERE id = NEW.id;
END;

ALTER TABLE collections ADD COLUMN sync_seq INTEGER NOT NULL DEFAULT 1;
CREATE INDEX collections_by_sync_seq ON collections (sync_seq);

CREATE TRIGGER collections_sync_insert AFTER INSERT ON collections BEGIN
    UPDATE sync_counter SET seq = seq + 1 WHERE id = 1;
    UPDATE collections SET sync_seq = (SELECT seq FROM sync_counter WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER collections_sync_update AFTER UPDATE ON collections WHEN NEW.sync_seq IS OLD.sync_seq BEGIN
    UPDATE sync_counter SET seq = seq + 1 WHERE id = 1;
    UPDATE collections SET sync_seq = (SELECT seq FROM sync_counter WHERE id = 1) WHERE id = NEW.id;
END;

ALTER TABLE environments ADD COLUMN sync_seq INTEGER NOT NULL DEFAULT 1;
CREATE INDEX environments_by_sync_seq ON environments (sync_seq);

CREATE TRIGGER environments_sync_insert AFTER INSERT ON environments BEGIN
    UPDATE sync_counter SET seq = seq + 1 WHERE id = 1;
    UPDATE environments SET sync_seq = (SELECT seq FROM sync_counter WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER environments_sync_update AFTER UPDATE ON environments WHEN NEW.sync_seq IS OLD.sync_seq BEGIN
    UPDATE sync_counter SET seq = seq + 1 WHERE id = 1;
    UPDATE environments SET sync_seq = (SELECT seq FROM sync_counter WHERE id = 1) WHERE id = NEW.id;
END;

ALTER TABLE nodes ADD COLUMN sync_seq INTEGER NOT NULL DEFAULT 1;
CREATE INDEX nodes_by_sync_seq ON nodes (sync_seq);

CREATE TRIGGER nodes_sync_insert AFTER INSERT ON nodes BEGIN
    UPDATE sync_counter SET seq = seq + 1 WHERE id = 1;
    UPDATE nodes SET sync_seq = (SELECT seq FROM sync_counter WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER nodes_sync_update AFTER UPDATE ON nodes WHEN NEW.sync_seq IS OLD.sync_seq BEGIN
    UPDATE sync_counter SET seq = seq + 1 WHERE id = 1;
    UPDATE nodes SET sync_seq = (SELECT seq FROM sync_counter WHERE id = 1) WHERE id = NEW.id;
END;

-- Client side: the synced workspaces and how far they are pulled.
CREATE TABLE sync_cursors (
    workspace_id TEXT PRIMARY KEY NOT NULL,
    seq          INTEGER NOT NULL
) STRICT;

-- The record as last got from the server, and the local `sync_seq` it was
-- written at: a row with a newer `sync_seq` may have a local change.
CREATE TABLE sync_base (
    kind         TEXT NOT NULL,
    id           TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    record       TEXT NOT NULL,
    local_seq    INTEGER NOT NULL,
    PRIMARY KEY (kind, id)
) STRICT;

CREATE INDEX sync_base_by_workspace ON sync_base (workspace_id);

-- Deleted against changed, waiting for the user.
CREATE TABLE sync_conflicts (
    kind         TEXT NOT NULL,
    id           TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    conflict     TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (kind, id)
) STRICT;

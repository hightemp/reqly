-- Full-text index over node name, URL, method and description.
-- Rows share rowid with `nodes`; kept in sync by triggers.
CREATE VIRTUAL TABLE nodes_fts USING fts5 (
    name,
    url,
    method,
    description,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER nodes_fts_insert AFTER INSERT ON nodes BEGIN
    INSERT INTO nodes_fts (rowid, name, url, method, description)
    VALUES (new.rowid, new.name, coalesce(new.url, ''), coalesce(new.method, ''),
            coalesce(json_extract(new.data, '$.description'), ''));
END;

CREATE TRIGGER nodes_fts_delete AFTER DELETE ON nodes BEGIN
    DELETE FROM nodes_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER nodes_fts_update AFTER UPDATE OF name, url, method, data ON nodes BEGIN
    DELETE FROM nodes_fts WHERE rowid = old.rowid;
    INSERT INTO nodes_fts (rowid, name, url, method, description)
    VALUES (new.rowid, new.name, coalesce(new.url, ''), coalesce(new.method, ''),
            coalesce(json_extract(new.data, '$.description'), ''));
END;

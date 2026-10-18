-- Who last changed each field of a node and in which version, for
-- field-level last-writer-wins merges (T7.2.2):
-- {"url": {"v": 7, "by": "<user id>"}, "name": {...}}.
ALTER TABLE nodes ADD COLUMN field_versions TEXT NOT NULL DEFAULT '{}';

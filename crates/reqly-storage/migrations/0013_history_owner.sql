-- Who sent the request (multi-user server); NULL for older records and
-- single-user setups. Private history shows each user their own records.
ALTER TABLE response_records ADD COLUMN user_id TEXT;

CREATE INDEX response_records_by_user ON response_records (user_id, request_id);

-- Collection-level pre-request and after-response scripts (stage 6).
ALTER TABLE collections ADD COLUMN scripts TEXT NOT NULL DEFAULT '{}';
-- Script console output and test results of a response.
ALTER TABLE response_records ADD COLUMN script_logs TEXT NOT NULL DEFAULT '[]';
ALTER TABLE response_records ADD COLUMN tests TEXT NOT NULL DEFAULT '[]';

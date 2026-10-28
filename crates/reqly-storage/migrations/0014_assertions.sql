-- Results of a response's no-code assertions (stage 9).
ALTER TABLE response_records ADD COLUMN assertions TEXT NOT NULL DEFAULT '[]';

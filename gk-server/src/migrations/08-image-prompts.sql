BEGIN;
ALTER TABLE Image ADD COLUMN prompt TEXT;
UPDATE Metadata SET value = 8 WHERE key = 'schema_version';
COMMIT;

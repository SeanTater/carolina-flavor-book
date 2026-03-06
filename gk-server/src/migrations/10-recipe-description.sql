ALTER TABLE Recipe ADD COLUMN description TEXT;
UPDATE metadata SET value = '10' WHERE key = 'schema_version';

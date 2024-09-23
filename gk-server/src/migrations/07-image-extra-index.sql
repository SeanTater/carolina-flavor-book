BEGIN;
CREATE INDEX IF NOT EXISTS image_everything_but_content_bytes
    ON Image(recipe_id, created_on, format, category);
DROP INDEX IF EXISTS image_by_recipe_id;

UPDATE Metadata SET value = '7' WHERE key = 'schema_version';
COMMIT;
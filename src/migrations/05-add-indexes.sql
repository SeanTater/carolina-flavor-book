BEGIN;
-- The image index is the only one important for performance because the other tables are small
CREATE INDEX IF NOT EXISTS image_by_recipe_id ON Image(recipe_id);
CREATE INDEX IF NOT EXISTS revision_by_recipe_id ON Revision(recipe_id);
CREATE INDEX IF NOT EXISTS tag_by_recipe_id ON Tag(recipe_id);
CREATE INDEX IF NOT EXISTS attempt_by_auto_task_id_item_id ON Attempt(auto_task_id, item_id);
UPDATE Metadata SET value = 5 WHERE key = 'schema_version';
COMMIT;
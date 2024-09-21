BEGIN;

-- To make embedding search more effective, we are counting the name as a revision of a recipe too.
INSERT INTO Revision (recipe_id, source_name, content_text)
    SELECT recipe_id, 'name', name
    FROM Recipe;

UPDATE Metadata SET value = '6' WHERE key = 'schema_version';
COMMIT;
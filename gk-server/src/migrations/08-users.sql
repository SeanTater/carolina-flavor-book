BEGIN;
-- This tag is useless because it's a prefix of the primary key
DROP TABLE IF EXISTS tag_by_recipe_id;

-- Creating users primarily so that we can track who created what
-- even though "who" is mostly models and not actual people
CREATE TABLE IF NOT EXISTS User (
    user_id INTEGER PRIMARY KEY,
    is_agent INTEGER NOT NULL DEFAULT 0,
    username TEXT NOT NULL,
    created_on TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    email TEXT, -- Optional, null for agents
    invited_by INTEGER REFERENCES User(user_id) ON DELETE SET NULL
);
ALTER TABLE Recipe
    ADD COLUMN created_by
    INTEGER DEFAULT NULL
    REFERENCES User(user_id) ON DELETE SET NULL;
ALTER TABLE Image
    ADD COLUMN created_by
    INTEGER DEFAULT NULL
    REFERENCES User(user_id) ON DELETE SET NULL;
ALTER TABLE Revision
    ADD COLUMN created_by
    INTEGER DEFAULT NULL
    REFERENCES User(user_id) ON DELETE SET NULL;
ALTER TABLE Tag
    ADD COLUMN created_by
    INTEGER DEFAULT NULL
    REFERENCES User(user_id) ON DELETE SET NULL;

-- We have to allow nulls because we're adding a column to an existing table
-- and we can't have a not null column with no default value
-- Now, we're going to fill them in so that they are not null anyway
-- We're going to set the created_by column to the user_id of the first user
INSERT INTO User(user_id, is_agent, username) VALUES (1, 0, 'founder');
INSERT INTO User(user_id, is_agent, username) VALUES (2, 1, 'flux.1 schnell');
INSERT INTO User(user_id, is_agent, username) VALUES (4, 1, 'llama3.1 8b');
INSERT INTO User(user_id, is_agent, username) VALUES (5, 1, 'gpt-4o-mini');

-- All the recipes are created by the founder at this point
UPDATE Recipe SET created_by = 1;
-- All the images with scan in the category are created by the founder too
UPDATE Image SET created_by = 1 WHERE category like '%scan%';
-- All the images with the category ai% are created by flux.1 schnell
UPDATE Image SET created_by = 2 WHERE category like 'ai%';
-- All the revisions with the source_name llm and details["model"] == "llama3.1", are created by llama3.1 8b
UPDATE Revision SET created_by = 4 WHERE source_name = 'llm' AND details -> '$.model' = 'llama3.1';
-- All the revisions with the source_name llm and details["model"] == "gpt-4o-mini", are created by gpt-4o-mini
UPDATE Revision SET created_by = 5 WHERE source_name in ('ocr', 'llm') AND details -> '$.model' = 'gpt-4o-mini';
-- All the revisions with the source_name llm and details["model"] == "gpt-4o", are created by gpt-4o
COMMIT;
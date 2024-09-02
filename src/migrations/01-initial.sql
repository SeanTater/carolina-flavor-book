PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

BEGIN;
-- Data such as the schema version is stored in the Metadata table.
CREATE TABLE IF NOT EXISTS Metadata (
    key TEXT PRIMARY KEY,
    value TEXT
);
CREATE TABLE IF NOT EXISTS Recipe (
    recipe_id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    created_on TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS Image (
    image_id INTEGER PRIMARY KEY,
    recipe_id INTEGER NOT NULL REFERENCES Recipe(recipe_id) ON DELETE CASCADE,
    created_on TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    format TEXT,
    category TEXT NOT NULL default 'scan',
    content_bytes BLOB
);
CREATE TABLE IF NOT EXISTS Revision (
    recipe_id INTEGER NOT NULL REFERENCES Recipe(recipe_id) ON DELETE CASCADE,
    created_on TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source_name TEXT,
    details TEXT,
    format TEXT,
    content_text TEXT
);
CREATE TABLE IF NOT EXISTS Tag (
    recipe_id INTEGER NOT NULL REFERENCES Recipe(recipe_id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    PRIMARY KEY (recipe_id, tag)
);
COMMIT;
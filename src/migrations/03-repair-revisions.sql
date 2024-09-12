BEGIN;
PRAGMA foreign_keys=off;
ALTER TABLE Revision RENAME TO Revision_old;
CREATE TABLE Revision (
    revision_id INTEGER NOT NULL PRIMARY KEY,
    recipe_id INTEGER NOT NULL REFERENCES Recipe(recipe_id) ON DELETE CASCADE,
    created_on TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source_name TEXT NOT NULL,
    details TEXT NOT NULL DEFAULT '{}',
    format TEXT NOT NULL DEFAULT 'text',
    content_text TEXT NOT NULL
);
INSERT INTO Revision SELECT rowid, recipe_id, created_on, source_name, coalesce(details, '{}'), coalesce(format, 'text'), content_text FROM Revision_old;
DROP TABLE Revision_old;

-- Embedding table needs to be fixed because it references the old Revision table
ALTER TABLE Embedding RENAME TO Embedding_old;
CREATE TABLE IF NOT EXISTS Embedding (
    embedding_id INTEGER PRIMARY KEY,
    recipe_id INTEGER REFERENCES Recipe(recipe_id) ON DELETE CASCADE,
    revision_id INTEGER REFERENCES Revision(revision_id) ON DELETE CASCADE,
    span_start INTEGER,
    span_end INTEGER,
    created_on TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    model_name TEXT NOT NULL,
    embedding BLOB NOT NULL
);

-- Allow the indexing process to be idempotent by deduplicating embeddings
CREATE UNIQUE INDEX IF NOT EXISTS idx_embedding_model_span ON Embedding(revision_id, model_name, span_start, span_end);

INSERT INTO Embedding SELECT * FROM Embedding_old;
DROP TABLE Embedding_old;

UPDATE Metadata SET value = 3 WHERE key = 'schema_version';
PRAGMA foreign_keys=on;
COMMIT;
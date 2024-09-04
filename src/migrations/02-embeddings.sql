-- Add an id column to the Revision table
ALTER TABLE Revision ADD COLUMN revision_id INTEGER;
UPDATE Revision SET revision_id = rowid;
CREATE UNIQUE INDEX IF NOT EXISTS idx_revision_id ON Revision(revision_id);

-- Embedding table
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

UPDATE Metadata SET value = 2 WHERE key = 'schema_version';
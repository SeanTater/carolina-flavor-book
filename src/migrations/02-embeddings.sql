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
    embedding BLOB NOT NULL
);
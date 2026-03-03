BEGIN;

CREATE TABLE IF NOT EXISTS FrontPageSection (
    date TEXT NOT NULL,        -- 'MM-DD' for annual calendar mapping
    section TEXT NOT NULL,     -- 'featured', 'spotlight', 'seasonal', 'healthy'
    title TEXT NOT NULL,       -- 'Food Spotlight: Northern China'
    blurb TEXT,                -- 'Hearty, warming dishes from Dongbei...'
    query_tags TEXT NOT NULL,  -- JSON array: ["dongbei", "comfort-food"]
    PRIMARY KEY (date, section)
);

UPDATE Metadata SET value = '9' WHERE key = 'schema_version';
COMMIT;

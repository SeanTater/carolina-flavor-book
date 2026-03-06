CREATE TABLE Author (
    author_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    bio TEXT NOT NULL
);

CREATE TABLE Article (
    article_id INTEGER PRIMARY KEY,
    author_id TEXT NOT NULL REFERENCES Author(author_id),
    title TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    summary TEXT,
    content_text TEXT NOT NULL,
    publish_date TEXT NOT NULL,
    created_on TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    thumbnail_image_id INTEGER REFERENCES Image(image_id)
);

CREATE TABLE ArticleRecipeLink (
    article_id INTEGER NOT NULL REFERENCES Article(article_id),
    recipe_id INTEGER NOT NULL REFERENCES Recipe(recipe_id),
    sort_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (article_id, recipe_id)
);

UPDATE metadata SET value = '11' WHERE key = 'schema_version';

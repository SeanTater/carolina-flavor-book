use std::fmt::Debug;

use crate::database::{Database, FromRow};
use anyhow::Result;
use gk::basic_models;
use half::f16;
use rusqlite::params;
use serde::{Deserialize, Serialize, Serializer};
use strum::{EnumString, IntoStaticStr};

pub fn sqlite_current_timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Recipe {
    pub recipe_id: i64,
    pub name: String,
    pub created_on: String,
    pub thumbnail: Option<Vec<u8>>,
}

impl FromRow for Recipe {
    /// Create a new recipe from an sql row, provided by rusqlite, using named columns.
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            recipe_id: row.get("recipe_id")?,
            name: row.get("name")?,
            created_on: row.get("created_on")?,
            thumbnail: row.get("thumbnail").ok(),
        })
    }
}

impl Recipe {
    /// List all the recipes in the database.
    pub fn list_some(db: &Database) -> Result<Vec<Recipe>> {
        let recipes: Vec<Recipe> = db.collect_rows("
            SELECT *,
                (SELECT content_bytes FROM Image WHERE recipe_id = Recipe.recipe_id ORDER BY category <> 'ai-01' LIMIT 1) AS thumbnail,
                (SELECT group_concat(tag, ', ') FROM Tag WHERE recipe_id = Recipe.recipe_id) AS tags
            FROM Recipe
            ORDER BY random()
            LIMIT 20
        ", params![])?;
        Ok(recipes)
    }

    /// List all the tags for a recipe
    pub fn get_tags(&self, db: &Database) -> Result<Vec<Tag>> {
        db.collect_rows(
            "SELECT * FROM Tag WHERE recipe_id = ?",
            params![self.recipe_id],
        )
    }

    /// Get all the images for a recipe
    pub fn get_images(&self, db: &Database) -> Result<Vec<Image>> {
        db.collect_rows(
            "SELECT * FROM Image WHERE recipe_id = ?",
            params![self.recipe_id],
        )
    }

    /// Get all the revisions of a recipe
    pub fn get_revisions(&self, db: &Database) -> Result<Vec<Revision>> {
        db.collect_rows(
            "SELECT * FROM Revision WHERE recipe_id = ?",
            params![self.recipe_id],
        )
    }

    /// Get a recipe by ID
    pub fn get_by_id(db: &Database, recipe_id: i64) -> Result<Option<Self>> {
        Ok(db
            .collect_rows(
                "SELECT *,
                    (SELECT content_bytes FROM Image WHERE recipe_id = Recipe.recipe_id ORDER BY category LIMIT 1) AS thumbnail
                    FROM Recipe
                    WHERE Recipe.recipe_id = ?",
                params![recipe_id],
            )?
            .pop())
    }

    /// Get a recipe that doesn't have enough images for a given category.
    ///
    /// This is used for a loop that generates images for recipes that don't have enough.
    pub fn get_any_recipe_without_enough_images(
        db: &Database,
        category: &str,
    ) -> Result<Option<FullRecipe>> {
        // We want to get a recipe that has less than 3 images.
        let at_least = 3;
        db.collect_rows(
            "SELECT
                        -- Sqliteism: this will get every column, even besides the group by
                        Recipe.*
                    FROM Recipe
                    LEFT JOIN Image
                        ON Recipe.recipe_id = Image.recipe_id
                        AND Image.category = ?
                    GROUP BY Recipe.recipe_id
                    HAVING COUNT(Image.image_id) < ?
                    ORDER BY RANDOM()
                    LIMIT 1",
            params![category, at_least],
        )?
        .pop()
        .and_then(|recipe: Recipe| Self::get_full_recipe(db, recipe.recipe_id).transpose())
        .transpose()
    }

    /// Get all the details about a recipe
    pub fn get_full_recipe(db: &Database, recipe_id: i64) -> Result<Option<FullRecipe>> {
        let recipe = Self::get_by_id(db, recipe_id)?;
        let revision_source_worst_to_best = ["name", "ocr", "llm", "manual"];
        if let Some(recipe) = recipe {
            let tags = recipe.get_tags(db)?;
            let images = recipe.get_images(db)?;
            let revisions = recipe.get_revisions(db)?;
            let best_revision = revisions
                .iter()
                .max_by_key(|r| {
                    // We want to pick one of these in order, picking anything else only as a last resort
                    revision_source_worst_to_best
                        .iter()
                        .position(|x| r.source_name == *x)
                        .unwrap_or_default()
                })
                .cloned();
            Ok(Some(FullRecipe {
                recipe,
                tags,
                images,
                revisions,
                best_revision,
            }))
        } else {
            Ok(None)
        }
    }

    /// Add a new recipe to the database
    pub fn push(db: &Database, upload: basic_models::RecipeForUpload) -> Result<i64> {
        let conn = db.pool.get()?;
        conn.execute(
            "INSERT INTO Recipe (name, created_on) VALUES (?, ?)",
            params![upload.name, sqlite_current_timestamp()],
        )?;
        let recipe_id = conn.last_insert_rowid();
        for tag in upload.tags {
            Tag::push(db, recipe_id, &tag)?;
        }
        for revision in upload.revisions {
            Revision::push(db, revision, recipe_id)?;
        }
        for image in upload.images {
            Image::push(db, recipe_id, image)?;
        }
        Ok(recipe_id)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tag {
    pub recipe_id: i64,
    pub tag: String,
}

impl FromRow for Tag {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            recipe_id: row.get("recipe_id")?,
            tag: row.get("tag")?,
        })
    }
}

impl Tag {
    /// Add a tag to a recipe
    pub fn push(db: &Database, recipe_id: i64, tag: &str) -> Result<()> {
        let conn = db.pool.get()?;
        conn.execute(
            "INSERT OR IGNORE INTO Tag (recipe_id, tag) VALUES (?, ?)",
            params![recipe_id, tag],
        )?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct Image {
    pub image_id: i64,
    pub recipe_id: i64,
    pub category: String,
    pub format: String,
    pub content_bytes: Vec<u8>,
}

impl FromRow for Image {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            image_id: row.get("image_id")?,
            recipe_id: row.get("recipe_id")?,
            category: row.get("category")?,
            format: row.get("format")?,
            content_bytes: row.get("content_bytes")?,
        })
    }
}

impl Image {
    pub fn get_image(db: &Database, image_id: i64) -> Result<Option<Image>> {
        Ok(db
            .collect_rows("SELECT * FROM Image WHERE image_id = ?", params![image_id])?
            .pop())
    }

    pub fn push(db: &Database, recipe_id: i64, upload: basic_models::ImageForUpload) -> Result<()> {
        let conn = db.pool.get()?;
        // Do some rudimentary validation
        anyhow::ensure!(
            upload.content_bytes.len() < 20_000_000,
            "Image is too large"
        );
        // Check that it decodes as webp
        let mut img =
            image::load_from_memory_with_format(&upload.content_bytes, image::ImageFormat::WebP)?;
        // If it's larger than 2048x2048, resize it
        img = if img.width() > 2048 || img.height() > 2048 {
            img.resize_to_fill(2048, 2048, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };
        // Encode it back to webp. image::DynamicImage doesn't offer lossy webp, but "webp" does.
        let content_bytes = webp::Encoder::from_image(&img)
            .map_err(|e| anyhow::anyhow!("WebP encoding error: {:?}", e))?
            .encode(75.0);
        conn.execute(
            "INSERT INTO Image (recipe_id, category, format, content_bytes)
            VALUES (?, ?, 'webp', ?)",
            params![recipe_id, upload.category, &content_bytes[..]],
        )?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Revision {
    pub revision_id: i64,
    pub recipe_id: i64,
    pub source_name: String,
    pub created_on: String,
    pub content_text: String,
    pub details: String,
    pub format: Option<String>,
    pub rendered: Option<String>,
}

impl FromRow for Revision {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let content_text: String = row.get("content_text")?;
        Ok(Self {
            revision_id: row.get("revision_id")?,
            recipe_id: row.get("recipe_id")?,
            source_name: row.get("source_name")?,
            created_on: row.get("created_on")?,

            // Render the markdown content
            // Normally I would use pulldown_cmark but it doesn't protect against XSS
            // So instead we're using markdown-rs, which is less featureful but safer
            // But only the post-1.0 version
            rendered: Some(markdown::to_html(&content_text)),
            content_text,
            details: row.get("details")?,
            format: row.get("format")?,
        })
    }
}

impl Revision {
    /// Get all the embeddings for a revision
    pub fn get_embeddings(&self, db: &Database) -> Result<Vec<Embedding>> {
        db.collect_rows(
            "SELECT * FROM Embedding WHERE recipe_id = ? AND revision_id = ?",
            params![self.recipe_id, self.created_on],
        )
    }

    /// Get some revisions without embeddings for a given model name.
    ///
    /// There's no guarantee that by the time the embeddings are computed,
    /// the revisions will still be without embeddings.
    pub fn get_revisions_without_embeddings(
        db: &Database,
        model_name: &str,
        limit: i64,
    ) -> Result<Vec<Revision>> {
        db.collect_rows(
            "SELECT *
            FROM Revision
            LEFT JOIN Embedding
                ON Revision.revision_id = Embedding.revision_id
                AND Embedding.model_name = ?
            WHERE Embedding.embedding_id IS NULL
            LIMIT ?",
            params![model_name, limit],
        )
    }

    /// Insert a new revision into the database
    pub fn push(
        db: &Database,
        upload: basic_models::RevisionForUpload,
        recipe_id: i64,
    ) -> Result<()> {
        let conn = db.pool.get()?;
        conn.execute(
            "INSERT INTO Revision (recipe_id, source_name, content_text, format, details)
            VALUES (?, ?, ?, ?, ?)",
            params![
                recipe_id,
                upload.source_name,
                upload.content_text,
                upload.format,
                upload.details.unwrap_or("{}".into())
            ],
        )?;
        Ok(())
    }
}

/// A full recipe, including all the details. This is used for rendering the recipe page.
#[derive(Debug, Serialize)]
pub struct FullRecipe {
    pub recipe: Recipe,
    pub tags: Vec<Tag>,
    pub images: Vec<Image>,
    pub revisions: Vec<Revision>,
    pub best_revision: Option<Revision>,
}

/// Convert a slice of bytes into a vector of f32
fn bytes_to_f16(bytes: &[u8]) -> Vec<f16> {
    bytes
        .chunks_exact(2)
        .map(|chunk| f16::from_le_bytes([chunk[0], chunk[1]]))
        .collect()
}

/// An embedding, associated with a span inside a revision
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Embedding {
    pub embedding_id: i64,
    pub recipe_id: i64,
    pub revision_id: i64,
    pub span_start: u32,
    pub span_end: u32,
    pub created_on: String,
    pub model_name: String,
    pub embedding: Vec<f16>,
}

impl FromRow for Embedding {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            embedding_id: row.get("embedding_id")?,
            recipe_id: row.get("recipe_id")?,
            revision_id: row.get("revision_id")?,
            span_start: row.get("span_start")?,
            span_end: row.get("span_end")?,
            created_on: row.get("created_on")?,
            model_name: row.get("model_name")?,
            embedding: bytes_to_f16(&row.get::<_, Vec<u8>>("embedding")?),
        })
    }
}

impl Embedding {
    /// Find the embedding count, to determine whether a revision has been indexed
    pub fn count_embeddings(db: &Database) -> Result<usize> {
        let conn = db.pool.get()?;
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM Embedding")?;
        let count: i64 = stmt.query_row(params![], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// List all embeddings in the database
    pub fn list_all(db: &Database, model_name: &str) -> Result<Vec<Embedding>> {
        db.collect_rows(
            "SELECT * FROM Embedding WHERE model_name = ?",
            params![model_name],
        )
    }

    /// Push a batch of embeddings into the database
    pub fn push(db: &Database, embeddings: &[Embedding]) -> Result<()> {
        let conn = db.pool.get()?;
        let mut stmt = conn.prepare(
            "INSERT OR IGNORE INTO Embedding (recipe_id, revision_id, span_start, span_end, model_name, embedding)
            VALUES (?, ?, ?, ?, ?, ?)")?;
        for embedding in embeddings {
            let embedding_bytes = embedding
                .embedding
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect::<Vec<u8>>();
            stmt.execute(params![
                embedding.recipe_id,
                embedding.revision_id,
                embedding.span_start,
                embedding.span_end,
                embedding.model_name,
                embedding_bytes
            ])?;
        }
        Ok(())
    }
}

#[derive(Debug, EnumString, IntoStaticStr, Serialize, Deserialize, Clone, Copy)]
pub enum ClaimType {
    GenerateImage,
    Unknown,
}

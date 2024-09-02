use anyhow::Result;
use base64::Engine;
use rusqlite::params;
use serde::{Deserialize, Serialize, Serializer};

use crate::database::{Database, FromRow};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Recipe {
    recipe_id: i64,
    name: String,
    created_on: String,
    thumbnail: Option<String>,
}

impl FromRow for Recipe {
    /// Create a new recipe from an sql row, provided by rusqlite, using named columns.
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            recipe_id: row.get("recipe_id")?,
            name: row.get("name")?,
            created_on: row.get("created_on")?,
            thumbnail: row
                .get("thumbnail")
                .map(|b: Vec<u8>| webp_to_data_url(&b))
                .ok(),
        })
    }
}

impl Recipe {
    /// List all the recipes in the database.
    pub fn list_all(db: &Database) -> Result<Vec<Recipe>> {
        let recipes: Vec<Recipe> = db.collect_rows("
            SELECT *,
                (SELECT content_bytes FROM Image WHERE recipe_id = Recipe.recipe_id LIMIT 1) AS thumbnail
            FROM Recipe
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
                "SELECT * FROM Recipe WHERE recipe_id = ?",
                params![recipe_id],
            )?
            .pop())
    }

    /// Get all the details about a recipe
    pub fn get_full_recipe(db: &Database, recipe_id: i64) -> Result<Option<FullRecipe>> {
        let recipe = Self::get_by_id(db, recipe_id)?;
        if let Some(recipe) = recipe {
            let tags = recipe.get_tags(db)?;
            let images = recipe.get_images(db)?;
            let revisions = recipe.get_revisions(db)?;
            let best_revision = revisions.iter().max_by_key(|r| &r.created_on).cloned();
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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tag {
    recipe_id: i64,
    tag: String,
}

impl FromRow for Tag {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Self {
            recipe_id: row.get("recipe_id")?,
            tag: row.get("tag")?,
        })
    }
}

/// Convert a webp image to a data URL
fn webp_to_data_url(bytes: &[u8]) -> String {
    format!(
        "data:image/webp;base64,{}",
        // For the purpose of data urls, you do NOT need to use the URL_SAFE variant
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// Serialize a webp image to a data URL
fn serialize_webp_to_data_url<S: Serializer>(bytes: &[u8], ser: S) -> Result<S::Ok, S::Error> {
    ser.serialize_str(&webp_to_data_url(bytes))
}

#[derive(Debug, Serialize, Clone)]
pub struct Image {
    image_id: i64,
    recipe_id: i64,
    category: String,
    format: String,
    // This is a webp encoded image
    // Store it as a data URL when rendering
    #[serde(serialize_with = "serialize_webp_to_data_url")]
    content_bytes: Vec<u8>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Revision {
    recipe_id: i64,
    source_name: String,
    created_on: String,
    content_text: String,
    details: String,
    format: Option<String>,
    rendered: Option<String>,
}

impl FromRow for Revision {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let content_text: String = row.get("content_text")?;
        Ok(Self {
            recipe_id: row.get("recipe_id")?,
            source_name: row.get("source_name")?,
            created_on: row.get("created_on")?,
            rendered: Some(Self::render_markdown(&content_text)),
            content_text,
            details: row.get("details")?,
            format: row.get("format")?,
        })
    }
}

impl Revision {
    fn render_markdown(markdown: &str) -> String {
        // Render the markdown content
        let mut buffer = String::new();
        let parser = pulldown_cmark::Parser::new(markdown);
        pulldown_cmark::html::push_html(&mut buffer, parser);
        buffer
    }
}

#[derive(Debug, Serialize)]
pub struct FullRecipe {
    recipe: Recipe,
    tags: Vec<Tag>,
    images: Vec<Image>,
    revisions: Vec<Revision>,
    best_revision: Option<Revision>,
}

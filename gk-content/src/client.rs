use anyhow::{ensure, Result};
use gk::basic_models::{RecipeForUpload, RevisionForUpload};
use serde::{Deserialize, Serialize};

/// HTTP client for the gk-server API.
pub struct ContentClient {
    http: reqwest::Client,
    server: String,
    token: String,
}

/// Minimal recipe info returned by /api/recipes/basic.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BasicRecipe {
    pub recipe_id: i64,
    pub name: String,
}

/// Tag pair returned by /api/tags.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TagEntry {
    pub recipe_id: i64,
    pub tag: String,
}

/// Recipe with full text content from /api/recipes/text.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RecipeWithText {
    pub recipe_id: i64,
    pub name: String,
    pub content_text: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MissingImageRecipe {
    pub recipe_id: i64,
    pub name: String,
    pub image_count: i64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SearchResult {
    pub recipe_id: i64,
    pub name: String,
    pub relevance: i32,
}

impl ContentClient {
    pub fn new(server: &str, token: &str) -> Self {
        Self {
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("Failed to build HTTP client"),
            server: server.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    /// Push a recipe and return its ID.
    pub async fn push_recipe(&self, name: &str, content: &str, tags: &[String]) -> Result<i64> {
        let upload = RecipeForUpload {
            name: name.to_string(),
            description: None,
            tags: tags.to_vec(),
            revisions: vec![RevisionForUpload {
                source_name: "generated".into(),
                content_text: content.to_string(),
                format: "markdown".into(),
                details: None,
            }],
            images: vec![],
        };

        let resp = self.http
            .post(format!("{}/api/recipe", self.server))
            .body(bincode::serialize(&upload)?)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await?;

        // The server returns a redirect to /recipe/{id}
        let status = resp.status();
        if status.is_redirection() {
            let location = resp.headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow::anyhow!("No Location header in redirect"))?;
            let id = location
                .rsplit('/')
                .next()
                .and_then(|s| s.parse::<i64>().ok())
                .ok_or_else(|| anyhow::anyhow!("Could not parse recipe_id from {location}"))?;
            Ok(id)
        } else {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Expected redirect, got {status}: {body}");
        }
    }

    /// Push tags for a recipe.
    pub async fn push_tags(&self, recipe_id: i64, tags: &[String]) -> Result<()> {
        let resp = self.http
            .post(format!("{}/api/tags/{recipe_id}", self.server))
            .json(tags)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await?;
        ensure!(resp.status().is_success(), "Failed to push tags: {}", resp.status());
        Ok(())
    }

    /// Upload an image for a recipe.
    pub async fn push_image(&self, recipe_id: i64, category: &str, image_bytes: Vec<u8>) -> Result<()> {
        let resp = self.http
            .post(format!("{}/api/image/{recipe_id}/{category}", self.server))
            .body(image_bytes)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await?;
        ensure!(resp.status().is_success(), "Failed to push image: {}", resp.status());
        Ok(())
    }

    /// Get recipes with missing images.
    pub async fn get_missing_images(&self, max_images: i64) -> Result<Vec<MissingImageRecipe>> {
        let resp = self.http
            .get(format!("{}/api/recipes/missing-images?max_images={max_images}", self.server))
            .send()
            .await?;
        ensure!(resp.status().is_success(), "Failed to get missing images: {}", resp.status());
        Ok(resp.json().await?)
    }

    /// Get all tags from the server.
    pub async fn get_all_tags(&self) -> Result<Vec<TagEntry>> {
        let resp = self.http
            .get(format!("{}/api/tags", self.server))
            .send()
            .await?;
        ensure!(resp.status().is_success(), "Failed to get tags: {}", resp.status());
        Ok(resp.json().await?)
    }

    /// Get basic recipe list.
    pub async fn get_all_basics(&self) -> Result<Vec<BasicRecipe>> {
        let resp = self.http
            .get(format!("{}/api/recipes/basic", self.server))
            .send()
            .await?;
        ensure!(resp.status().is_success(), "Failed to get recipes: {}", resp.status());
        Ok(resp.json().await?)
    }

    /// Get all recipes with their full text content.
    pub async fn get_all_recipes_with_text(&self) -> Result<Vec<RecipeWithText>> {
        let resp = self.http
            .get(format!("{}/api/recipes/text", self.server))
            .send()
            .await?;
        ensure!(resp.status().is_success(), "Failed to get recipes text: {}", resp.status());
        Ok(resp.json().await?)
    }

    /// Semantic search for recipes.
    pub async fn search_semantic(&self, query: &str) -> Result<Vec<SearchResult>> {
        let resp = self.http
            .get(format!("{}/api/search?query={}", self.server, urlencoding::encode(query)))
            .send()
            .await?;
        ensure!(resp.status().is_success(), "Failed to search: {}", resp.status());
        Ok(resp.json().await?)
    }

    /// Patch a recipe (rename, update content, modify tags).
    /// All fields are optional — only provided fields are applied.
    pub async fn patch_recipe(&self, recipe_id: i64, patch: &serde_json::Value) -> Result<()> {
        let resp = self.http
            .patch(format!("{}/api/recipe/{recipe_id}", self.server))
            .json(patch)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await?;
        ensure!(resp.status().is_success(), "Failed to patch recipe {recipe_id}: {}", resp.status());
        Ok(())
    }

    /// Rename a recipe (convenience wrapper around patch_recipe).
    pub async fn rename_recipe(&self, recipe_id: i64, new_name: &str) -> Result<()> {
        self.patch_recipe(recipe_id, &serde_json::json!({"name": new_name})).await
    }

    /// Upsert an author.
    pub async fn upsert_author(&self, author: &serde_json::Value) -> Result<()> {
        let resp = self.http
            .post(format!("{}/api/author", self.server))
            .json(author)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await?;
        ensure!(resp.status().is_success(), "Failed to upsert author: {}", resp.status());
        Ok(())
    }

    /// Publish an article, returns article_id.
    pub async fn publish_article(&self, article: &serde_json::Value) -> Result<i64> {
        let resp = self.http
            .post(format!("{}/api/article", self.server))
            .json(article)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await?;
        ensure!(resp.status().is_success(), "Failed to publish article: {}", resp.status());
        let body: serde_json::Value = resp.json().await?;
        Ok(body["article_id"].as_i64().unwrap_or(0))
    }

    /// Upsert front page schedule sections.
    pub async fn upsert_schedule(&self, sections: &[serde_json::Value]) -> Result<()> {
        let resp = self.http
            .post(format!("{}/api/schedule", self.server))
            .json(sections)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await?;
        ensure!(resp.status().is_success(), "Failed to upsert schedule: {}", resp.status());
        Ok(())
    }
}

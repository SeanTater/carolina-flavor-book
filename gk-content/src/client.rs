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

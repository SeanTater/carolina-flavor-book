use super::llm::generate_recipe_scene;
use gk::basic_models::ImageForUpload;
use anyhow::anyhow;
use anyhow::Result;
use reqwest::Client;
use serde_json::json;
lazy_static::lazy_static! {
    pub(crate) static ref reqwest_client: Client = Client::new();
}

/// Call an LLM to generate a text scene of the finished product of a recipe,
/// then calls another API to generate an image of it
pub async fn illustrate_recipe(best_recipe_text: &str) -> Result<Vec<ImageForUpload>> {
    let api_key = dotenvy::var("REPLICATE_API_KEY")?;
    for scene_id in 0..5 {
        // Each scene is independent
        let scene = generate_recipe_scene(best_recipe_text).await?;
        let image_generation_response: serde_json::Value = reqwest_client
            .post("https://api.replicate.com/v1/models/black-forest-labs/flux-schnell/predictions")
            .bearer_auth(&api_key)
            .json(&json!({
                "input": {
                    "prompt": scene,
                    "go_fast": true,
                    "num_outputs": 1,
                    "aspect_ratio": "16:9",
                    "output_format": "webp",
                    "output_quality": 85,
                }
            }))
            .send()
            .await?
            .json()
            .await?;
        // We only care about the ["urls"]["get"] field
        let get_image_url = image_generation_response
            .pointer("/urls/get")
            .ok_or_else(|| anyhow!("No image URL in response"))?
            .as_str()
            .ok_or_else(|| anyhow!("Image URL is not a string"))?;
        // Now go fetch that image
        tracing::info!("Fetching image from {}", get_image_url);
        let image_bytes = reqwest_client
            .get(get_image_url)
            .bearer_auth(&api_key)
            .send()
            .await?
            .bytes()
            .await?;
        // Save to a local file for debugging
        std::fs::write(format!("scene-{}.webp", scene_id), &image_bytes)?;
    }
    // todo
    Ok(vec![])
}

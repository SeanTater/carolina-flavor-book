use super::{
    convert_to_webp,
    llm::{generate_recipe_scene, LlmConfig},
};
use gk::basic_models::ImageForUpload;
use anyhow::Result;
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct DiffusionRequest {
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    negative_prompt: Option<String>,
    width: u32,
    height: u32,
    steps: u32,
    cfg_scale: f32,
    seed: u32,
    include_base64: bool,
}

#[derive(Debug, Deserialize)]
struct DiffusionResponse {
    #[allow(dead_code)]
    job_id: String,
    base64_png: String,
}

pub async fn illustrate_recipe(llm_config: &LlmConfig, diffusion_base_url: &str, best_recipe_text: &str) -> Result<Vec<ImageForUpload>> {
    let client = Client::new();
    let mut images = Vec::new();

    for scene_id in 0..5 {
        // Each scene is independent
        let prompt = generate_recipe_scene(llm_config, best_recipe_text).await?;
        tracing::info!("Generating image {scene_id} for scene: {prompt}");

        let request = DiffusionRequest {
            prompt: prompt.clone(),
            negative_prompt: Some("text, watermark, blurry, low quality".to_string()),
            width: 1664,
            height: 928,
            steps: 20,
            cfg_scale: 4.0,
            seed: 0, // random
            include_base64: true,
        };

        let response: DiffusionResponse = client
            .post(format!("{diffusion_base_url}/api/generate"))
            .json(&request)
            .send()
            .await?
            .json()
            .await?;

        tracing::info!("Generated image {scene_id}");

        // Decode base64 to bytes
        let png_bytes = base64::engine::general_purpose::STANDARD
            .decode(&response.base64_png)?;
        let img = image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png)?;
        let content_bytes = convert_to_webp(&img, 75.0)?;

        images.push(ImageForUpload {
            category: "generated".to_string(),
            content_bytes,
            prompt: Some(prompt),
        });
    }

    Ok(images)
}

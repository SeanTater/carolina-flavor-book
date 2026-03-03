use anyhow::Result;
use crate::client::ContentClient;
use std::collections::BTreeMap;

/// Apply tags from a JSON map of {recipe_id: [tags]}.
pub async fn ingest_tags(client: &ContentClient, tags_map: &BTreeMap<i64, Vec<String>>) -> Result<IngestTagsReport> {
    let mut added = 0u64;

    for (recipe_id, tags) in tags_map {
        client.push_tags(*recipe_id, tags).await?;
        added += tags.len() as u64;
    }

    Ok(IngestTagsReport { added, recipes: tags_map.len() as u64 })
}

#[derive(Debug, serde::Serialize)]
pub struct IngestTagsReport {
    pub added: u64,
    pub recipes: u64,
}

#[derive(Debug, serde::Deserialize)]
pub struct RecipeIngest {
    pub name: String,
    pub content: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub image_prompt: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct IngestReport {
    pub created: u64,
    pub failed: u64,
    pub images_generated: u64,
}

pub async fn ingest_recipes(
    client: &ContentClient,
    recipes: &[RecipeIngest],
    generate_images: bool,
    image_gen_args: &[String],
) -> Result<IngestReport> {
    let mut created = 0u64;
    let mut failed = 0u64;
    let mut images_generated = 0u64;

    for recipe in recipes {
        match client.push_recipe(&recipe.name, &recipe.content, &recipe.tags).await {
            Ok(recipe_id) => {
                created += 1;
                if generate_images {
                    if let Some(prompt) = &recipe.image_prompt {
                        match generate_all_styles(client, recipe_id, prompt, image_gen_args).await {
                            Ok(n) => images_generated += n,
                            Err(e) => eprintln!("Image generation failed for {}: {e}", recipe.name),
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to ingest {}: {e}", recipe.name);
                failed += 1;
            }
        }
    }

    Ok(IngestReport { created, failed, images_generated })
}

/// The image styles we generate for each recipe.
/// Each style has a category name and a prompt template.
/// The `{}` placeholder is replaced with the base image prompt.
const IMAGE_STYLES: &[(&str, &str)] = &[
    ("hero", "{}, overhead angle, dramatic lighting, dark moody background, editorial food photography"),
    ("closeup", "{}, extreme close-up macro shot, shallow depth of field, showing texture and detail of the dish"),
    ("in-context", "{}, served at a warm dinner table with family or friends, ambient candlelight, lifestyle food photography"),
    ("plating", "{}, elegant restaurant plating on a clean white plate, minimalist garnish, fine dining presentation"),
];

/// Generate one image in a specific style and push it.
async fn generate_one(
    client: &ContentClient,
    recipe_id: i64,
    category: &str,
    full_prompt: &str,
    image_gen_args: &[String],
) -> Result<()> {
    let tmp = tempfile::NamedTempFile::new()?.into_temp_path();
    let output_path = format!("{}.webp", tmp.display());

    let status = tokio::process::Command::new("image-gen")
        .arg(full_prompt)
        .arg("-o")
        .arg(&output_path)
        .arg("--quality")
        .arg("75")
        .args(image_gen_args)
        .status()
        .await?;

    anyhow::ensure!(status.success(), "image-gen exited with {status}");

    let image_bytes = tokio::fs::read(&output_path).await?;
    client.push_image(recipe_id, category, image_bytes).await?;

    let _ = tokio::fs::remove_file(&output_path).await;

    Ok(())
}

/// Generate all image styles for a recipe. Returns the number successfully generated.
pub async fn generate_all_styles(
    client: &ContentClient,
    recipe_id: i64,
    base_prompt: &str,
    image_gen_args: &[String],
) -> Result<u64> {
    let mut count = 0u64;
    for (category, template) in IMAGE_STYLES {
        let full_prompt = template.replace("{}", base_prompt);
        match generate_one(client, recipe_id, category, &full_prompt, image_gen_args).await {
            Ok(()) => {
                count += 1;
                eprintln!("    ✓ {category}");
            }
            Err(e) => {
                eprintln!("    ✗ {category}: {e}");
            }
        }
    }
    Ok(count)
}

/// Generate images for a single style only. Returns Ok(()) on success.
pub async fn generate_single_style(
    client: &ContentClient,
    recipe_id: i64,
    base_prompt: &str,
    style: &str,
    image_gen_args: &[String],
) -> Result<()> {
    let (category, template) = IMAGE_STYLES
        .iter()
        .find(|(cat, _)| *cat == style)
        .ok_or_else(|| anyhow::anyhow!(
            "Unknown style '{}'. Valid styles: {}",
            style,
            IMAGE_STYLES.iter().map(|(c, _)| *c).collect::<Vec<_>>().join(", ")
        ))?;
    let full_prompt = template.replace("{}", base_prompt);
    generate_one(client, recipe_id, category, &full_prompt, image_gen_args).await
}

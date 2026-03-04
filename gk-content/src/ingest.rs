use anyhow::Result;
use crate::client::ContentClient;
use rand::prelude::*;
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

/// Image style definition: a category name and a pool of prompt templates.
/// `{}` is replaced with the base image prompt. One template is chosen at random per generation.
struct ImageStyle {
    category: &'static str,
    templates: &'static [&'static str],
}

const IMAGE_STYLES: &[ImageStyle] = &[
    ImageStyle {
        category: "hero",
        templates: &[
            "Overhead angle, dramatic lighting, dark moody background, editorial food photography, {}",
            "45-degree angle, warm directional side light, shallow depth of field, editorial food photography, {}",
            "Straight-on eye-level shot, rich saturated colors, dark slate background, professional food photography, {}",
        ],
    },
    ImageStyle {
        category: "closeup",
        templates: &[
            "Extreme close-up macro shot, shallow depth of field, showing texture and detail, {}",
            "Tight crop on a single serving, bokeh background, natural window light, food detail photography, {}",
            "Macro lens detail shot, steam rising, glistening surface texture, dramatic side lighting, {}",
        ],
    },
    ImageStyle {
        category: "in-context",
        templates: &[
            "Warm family dinner table, mismatched plates, ambient candlelight, lifestyle food photography, {}",
            "Outdoor summer table setting, dappled sunlight through trees, linen napkins, garden party atmosphere, {}",
            "Bustling street food stall, neon signs in background, steam rising, night market energy, {}",
            "Cozy winter kitchen scene, wooden table, frost on the window, warm tungsten lighting, {}",
            "Rustic farmhouse table, wildflowers in a jar, morning light streaming in, countryside brunch, {}",
            "Packed lunch spread on a park bench, sunny day, casual picnic blanket, natural daylight, {}",
            "Seaside restaurant terrace, ocean in the background, white tablecloth, Mediterranean golden hour, {}",
            "Dimly lit wine bar, exposed brick walls, candlelight, intimate date night atmosphere, {}",
            "Busy weekend brunch café, marble countertop, coffee cups nearby, urban lifestyle photography, {}",
            "Grandmother's kitchen, worn wooden table, handmade ceramics, nostalgic warm film grain, {}",
            "Camping scene, enamel plate, campfire glow in the background, outdoor adventure cooking, {}",
            "Rooftop dinner party, city skyline at dusk, string lights overhead, social gathering, {}",
        ],
    },
    ImageStyle {
        category: "plating",
        templates: &[
            "Elegant restaurant plating on a clean white plate, minimalist garnish, fine dining presentation, {}",
            "Modern Nordic plating, matte ceramic plate, negative space, single edible flower garnish, {}",
            "Rustic earthenware plating, rough clay bowl, drizzle of oil, artisan presentation, {}",
            "Japanese-inspired plating, asymmetric arrangement, small portions, lacquerware, wabi-sabi aesthetic, {}",
            "Deconstructed presentation, components arranged separately on a slate board, chef's tasting style, {}",
        ],
    },
    ImageStyle {
        category: "ingredient-prep",
        templates: &[
            "Mise en place on a wooden cutting board, knife, bowls of prepped ingredients, overhead shot, fresh raw ingredients laid out for making {}",
            "Small bowls and ramekins on a marble countertop, prep stage, bright natural light, overhead, ingredients for {}",
            "Chopped vegetables, measured spices in pinch bowls, herbs, raw proteins, kitchen prep scene, mise en place for {}",
            "Market-fresh ingredients just unpacked from a shopping bag, scattered on a butcher block, rustic and abundant, for making {}",
        ],
    },
];

/// Pick a random template from a style's pool and fill in the base prompt.
fn build_prompt(style: &ImageStyle, base_prompt: &str) -> String {
    let mut rng = rand::thread_rng();
    let template = style.templates.choose(&mut rng).expect("style has no templates");
    template.replace("{}", base_prompt)
}

const MAX_IMAGE_RETRIES: u32 = 5;

/// Generate one image in a specific style and push it, with retries.
async fn generate_one(
    client: &ContentClient,
    recipe_id: i64,
    category: &str,
    full_prompt: &str,
    image_gen_args: &[String],
) -> Result<()> {
    let mut last_err = None;
    for attempt in 1..=MAX_IMAGE_RETRIES {
        let tmp = tempfile::NamedTempFile::new()?.into_temp_path();
        let output_path = format!("{}.webp", tmp.display());

        let result: Result<()> = async {
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
        }.await;

        match result {
            Ok(()) => return Ok(()),
            Err(e) => {
                if attempt < MAX_IMAGE_RETRIES {
                    let delay = 2u64.pow(attempt);
                    eprintln!("      retry {attempt}/{MAX_IMAGE_RETRIES} (wait {delay}s): {e}");
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap())
}

/// Generate all image styles for a recipe. Returns the number successfully generated.
pub async fn generate_all_styles(
    client: &ContentClient,
    recipe_id: i64,
    base_prompt: &str,
    image_gen_args: &[String],
) -> Result<u64> {
    let mut count = 0u64;
    for style in IMAGE_STYLES {
        let full_prompt = build_prompt(style, base_prompt);
        match generate_one(client, recipe_id, style.category, &full_prompt, image_gen_args).await {
            Ok(()) => {
                count += 1;
                eprintln!("    ✓ {}", style.category);
            }
            Err(e) => {
                eprintln!("    ✗ {}: {e}", style.category);
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
    style_name: &str,
    image_gen_args: &[String],
) -> Result<()> {
    let style = IMAGE_STYLES
        .iter()
        .find(|s| s.category == style_name)
        .ok_or_else(|| anyhow::anyhow!(
            "Unknown style '{}'. Valid styles: {}",
            style_name,
            IMAGE_STYLES.iter().map(|s| s.category).collect::<Vec<_>>().join(", ")
        ))?;
    let full_prompt = build_prompt(style, base_prompt);
    generate_one(client, recipe_id, style.category, &full_prompt, image_gen_args).await
}

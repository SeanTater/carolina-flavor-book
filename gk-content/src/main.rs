use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::Deserialize;

use gk_content::{client::ContentClient, gaps, grid, ingest};

#[derive(Parser)]
#[command(name = "gk-content", about = "Recipe content pipeline tools")]
struct Cli {
    /// Path to server config TOML (reads server.address and auth.service_principal_secret)
    #[arg(long, default_value = "config/dev.toml")]
    config: String,

    /// Path to the recipe grid config
    #[arg(long, default_value = "config/recipe-grid.toml")]
    grid: String,

    #[command(subcommand)]
    command: Commands,
}

/// Subset of the server config we need — just the address and auth token.
#[derive(Deserialize)]
struct ContentConfig {
    server: ServerSection,
    auth: AuthSection,
}

#[derive(Deserialize)]
struct ServerSection {
    address: String,
}

#[derive(Deserialize)]
struct AuthSection {
    service_principal_secret: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Show gaps in recipe tag coverage
    Gaps {
        /// Filter to recipes with this cuisine tag
        #[arg(long)]
        cuisine: Option<String>,

        /// Axes to ignore in the report
        #[arg(long, value_delimiter = ',')]
        ignore: Vec<String>,

        /// Output as JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
    /// Apply tags to existing recipes from a JSON file
    IngestTags {
        /// Path to JSON file: {"recipe_id": ["tag1", "tag2"], ...}
        file: String,
    },
    /// Ingest new recipes from a JSON file
    Ingest {
        /// Path to JSON file: [{name, content, tags, image_prompt?}, ...]
        file: String,

        /// Generate images using image-gen for recipes with image_prompt
        #[arg(long)]
        images: bool,

        /// Extra arguments passed to image-gen (e.g. --image-gen-arg=--port --image-gen-arg=9091)
        #[arg(long = "image-gen-arg")]
        image_gen_args: Vec<String>,
    },
    /// Generate and attach images to existing recipes from a JSON file
    AddImages {
        /// Path to JSON file: {"recipe_id": "image_prompt", ...}
        file: String,

        /// Generate only this style (hero, closeup, in-context, plating). Omit for all 4.
        #[arg(long)]
        style: Option<String>,

        /// Extra arguments passed to image-gen (e.g. --image-gen-arg=--port --image-gen-arg=9091)
        #[arg(long = "image-gen-arg")]
        image_gen_args: Vec<String>,
    },
    /// Load front page schedule from a JSON file
    IngestSchedule {
        /// Path to JSON file: [{date, section, title, blurb?, query_tags}, ...]
        file: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let config_text = std::fs::read_to_string(&cli.config)?;
    let config: ContentConfig = toml::from_str(&config_text)?;

    let server_url = format!("http://{}", config.server.address);
    let client = ContentClient::new(&server_url, &config.auth.service_principal_secret);
    let grid = grid::RecipeGrid::load(&cli.grid)?;

    match cli.command {
        Commands::Gaps { cuisine, ignore, json } => {
            let all_tags = client.get_all_tags().await?;
            let basics = client.get_all_basics().await?;
            let report = gaps::analyze(&all_tags, basics.len() as u64, &grid, cuisine.as_deref(), &ignore);
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", gaps::format_text(&report));
            }
        }
        Commands::IngestTags { file } => {
            let content = std::fs::read_to_string(&file)?;
            let tags_map: std::collections::BTreeMap<i64, Vec<String>> = serde_json::from_str(&content)?;
            let report = ingest::ingest_tags(&client, &tags_map).await?;
            println!("Tagged {} recipes, {} tags added", report.recipes, report.added);
        }
        Commands::Ingest { file, images, image_gen_args } => {
            let content = std::fs::read_to_string(&file)?;
            let recipes: Vec<ingest::RecipeIngest> = serde_json::from_str(&content)?;
            println!("Ingesting {} recipes...", recipes.len());
            let report = ingest::ingest_recipes(&client, &recipes, images, &image_gen_args).await?;
            println!("Created: {}, Failed: {}, Images: {}",
                report.created, report.failed, report.images_generated);
        }
        Commands::AddImages { file, style, image_gen_args } => {
            let content = std::fs::read_to_string(&file)?;
            let prompts: std::collections::BTreeMap<i64, String> = serde_json::from_str(&content)?;
            let style_label = style.as_deref().unwrap_or("all 4 styles");
            println!("Generating {style_label} images for {} recipes...", prompts.len());
            let mut generated = 0u64;
            let mut failed = 0u64;
            for (recipe_id, prompt) in &prompts {
                eprintln!("  recipe {recipe_id}:");
                if let Some(s) = &style {
                    match ingest::generate_single_style(&client, *recipe_id, prompt, s, &image_gen_args).await {
                        Ok(()) => {
                            generated += 1;
                            eprintln!("    ✓ {s}");
                        }
                        Err(e) => {
                            eprintln!("    ✗ {s}: {e}");
                            failed += 1;
                        }
                    }
                } else {
                    match ingest::generate_all_styles(&client, *recipe_id, prompt, &image_gen_args).await {
                        Ok(n) => generated += n,
                        Err(e) => {
                            eprintln!("    ✗ all styles failed: {e}");
                            failed += 1;
                        }
                    }
                }
            }
            println!("Generated: {generated}, Failed: {failed}");
        }
        Commands::IngestSchedule { file } => {
            let content = std::fs::read_to_string(&file)?;
            let sections: Vec<serde_json::Value> = serde_json::from_str(&content)?;
            let count = sections.len();
            client.upsert_schedule(&sections).await?;
            println!("Loaded {} front page sections", count);
        }
    }

    Ok(())
}

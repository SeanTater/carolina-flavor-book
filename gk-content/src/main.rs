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
    /// Search existing recipes (regex on names by default, or --semantic for embedding search)
    Search {
        /// Regex pattern or semantic query
        query: String,

        /// Use server-side semantic search instead of regex
        #[arg(long)]
        semantic: bool,
    },
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
    /// List recipes with missing or few images
    MissingImages {
        /// Show recipes with at most this many images (default: 0 = no images)
        #[arg(long, default_value = "0")]
        max_images: i64,

        /// Output as JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
    /// Batch rename recipes from a JSON file
    Rename {
        /// Path to JSON file: {"recipe_id": "new name", ...}
        file: String,
    },
    /// Batch patch recipes from a JSON file
    Patch {
        /// Path to JSON file: {"recipe_id": {name?, content?, tags?}, ...}
        file: String,
    },
    /// Create or update an author from a JSON file
    UpsertAuthor {
        /// Path to JSON file: {author_id, display_name, bio}
        file: String,
    },
    /// Publish an article from a JSON file
    PublishArticle {
        /// Path to JSON file: {author_id, title, slug, summary?, content_text, publish_date, recipe_ids?}
        file: String,
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
        Commands::Search { query, semantic } => {
            if semantic {
                let results = client.search_semantic(&query).await?;
                for r in &results {
                    println!("{}%\t{}\t{}", r.relevance, r.recipe_id, r.name);
                }
                eprintln!("{} results", results.len());
            } else {
                let re = regex::RegexBuilder::new(&query).case_insensitive(true).build()?;
                let basics = client.get_all_basics().await?;
                let mut count = 0;
                for r in &basics {
                    if re.is_match(&r.name) {
                        println!("{}\t{}", r.recipe_id, r.name);
                        count += 1;
                    }
                }
                eprintln!("{count} matches");
            }
        }
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
        Commands::MissingImages { max_images, json } => {
            let recipes = client.get_missing_images(max_images).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&recipes)?);
            } else {
                println!("{} recipes with <= {max_images} images:\n", recipes.len());
                for r in &recipes {
                    println!("  {:>20}  {} ({})", r.recipe_id, r.name, r.image_count);
                }
            }
        }
        Commands::Rename { file } => {
            let content = std::fs::read_to_string(&file)?;
            let renames: std::collections::BTreeMap<i64, String> = serde_json::from_str(&content)?;
            println!("Renaming {} recipes...", renames.len());
            let mut done = 0u64;
            let mut failed = 0u64;
            for (recipe_id, new_name) in &renames {
                match client.rename_recipe(*recipe_id, new_name).await {
                    Ok(()) => done += 1,
                    Err(e) => {
                        eprintln!("  ✗ {recipe_id}: {e}");
                        failed += 1;
                    }
                }
            }
            println!("Renamed {done}, failed {failed}");
        }
        Commands::Patch { file } => {
            let content = std::fs::read_to_string(&file)?;
            let patches: std::collections::BTreeMap<i64, serde_json::Value> = serde_json::from_str(&content)?;
            println!("Patching {} recipes...", patches.len());
            let mut done = 0u64;
            for (recipe_id, patch) in &patches {
                client.patch_recipe(*recipe_id, patch).await?;
                done += 1;
            }
            println!("Patched {done} recipes");
        }
        Commands::UpsertAuthor { file } => {
            let content = std::fs::read_to_string(&file)?;
            let author: serde_json::Value = serde_json::from_str(&content)?;
            client.upsert_author(&author).await?;
            println!("Upserted author: {}", author["author_id"]);
        }
        Commands::PublishArticle { file } => {
            let content = std::fs::read_to_string(&file)?;
            let article: serde_json::Value = serde_json::from_str(&content)?;
            let id = client.publish_article(&article).await?;
            println!("Published article {}: {}", id, article["title"]);
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

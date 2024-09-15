use anyhow::{ensure, Ok, Result};
use clap::{Parser, ValueEnum};
use gk::basic_models;
use gk_client::ingestion;

/// Add a recipe to the cookbook
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Args {
    /// The name of the recipe to add
    name: String,
    /// Enable dictation mode
    #[arg(short, long)]
    dictation: bool,
    /// Enable webcam capture, from device number <webcam>
    #[arg(short, long)]
    webcam: Option<usize>,
    /// Enable Freestyle mode: use an LLM to generate a new recipe by name.
    #[arg(short, long)]
    freestyle: bool,
    /// Rotate the image
    #[arg(short, long)]
    rotate: Option<Rotate>,
    /// Tags to add to the recipe
    tags: Vec<String>,
    /// URL of the server to upload to
    #[arg(long, default_value = "https://gallagher.kitchen")]
    server: String,
    /// Dry run mode: don't actually upload the recipe
    #[arg(long)]
    dry: bool,
    /// LLM API base URL.
    #[arg(long, default_value = "http://localhost:11434/v1")]
    llm_api_base: String,
}
#[derive(Parser, Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum Rotate {
    R0,
    R90,
    R180,
    R270,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let mut revisions = vec![];
    let mut images = vec![];
    let mut tags = args.tags;
    let mut best_input_text = None;

    println!("Recipe name: {}", args.name);

    if let Some(device_number) = args.webcam {
        println!("Webcam mode enabled");
        let mut picture = ingestion::take_picture(device_number)?;
        picture = match args.rotate.unwrap_or(Rotate::R0) {
            Rotate::R0 => picture,
            Rotate::R90 => picture.rotate90(),
            Rotate::R180 => picture.rotate180(),
            Rotate::R270 => picture.rotate270(),
        };

        let content_text = ingestion::read_text_from_image(&picture)?;

        let webp_bytes = ingestion::convert_to_webp(&picture, 75.0)?;
        images.push(basic_models::ImageForUpload {
            category: "raw scan".to_string(),
            content_bytes: webp_bytes,
        });

        best_input_text = Some(content_text.clone());
        revisions.push(basic_models::RevisionForUpload {
            source_name: "ocr".to_string(),
            content_text: content_text.clone(),
            format: "text".to_string(),
            details: None,
        });
    }

    if args.dictation {
        println!("Dictation mode enabled");
        let words = ingestion::take_dictation().await?;
        best_input_text = Some(words.clone());
        println!("Transcribed words: {}", words);
        revisions.push(basic_models::RevisionForUpload {
            source_name: "voice".to_string(),
            content_text: words,
            format: "text".to_string(),
            details: None,
        });
    }

    if args.freestyle {
        println!("Freestyle mode enabled");
        let better_text = ingestion::freestyle(&args.name, Some(&args.llm_api_base)).await?;
        best_input_text = Some(better_text.clone());
        revisions.push(basic_models::RevisionForUpload {
            source_name: "llm".to_string(),
            content_text: better_text,
            format: "markdown".to_string(),
            details: Some("{\"model\": \"llama3.1\"}".to_string()),
        });
        tags.push("freestyle".to_string());
    }

    if let Some(content_text) = best_input_text {
        if !args.freestyle {
            let better_text =
                ingestion::improve_recipe_with_llm(&content_text, Some(&args.llm_api_base)).await?;

            revisions.push(basic_models::RevisionForUpload {
                source_name: "llm".to_string(),
                content_text: better_text,
                format: "markdown".to_string(),
                details: Some("{\"model\": \"llama3.1\"}".to_string()),
            });
        }
    }

    let recipe_upload = basic_models::RecipeForUpload {
        name: args.name,
        revisions,
        images,
        tags,
    };

    println!("Recipe upload: {:#?}", recipe_upload);

    if args.dry {
        println!("Dry run mode enabled, skipping upload");
        return Ok(());
    }
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/recipe", args.server))
        .body(bincode::serialize(&recipe_upload)?)
        .header(
            "Authorization",
            format!("Bearer {}", dotenvy::var("PRINCIPAL_SECRET")?),
        )
        .send()
        .await?;
    ensure!(
        resp.status().is_success(),
        "Failed to upload recipe. Response: {:#?}",
        resp.text().await?,
    );
    tracing::info!("Recipe uploaded successfully");

    Ok(())
}

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
    /// Also illustrate the recipe using an API
    #[arg(long)]
    illustrate: bool,
    /// Directly upload a Revision with a specified source name, details, format, and content
    #[arg(long, num_args(4))]
    direct: Option<Vec<String>>,
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
    if dotenvy::dotenv().is_err() {
        tracing::warn!("Failed to load .env file, so API keys are probably not available");
    }
    let args = Args::parse();

    let mut revisions = vec![basic_models::RevisionForUpload {
        source_name: "name".to_string(),
        content_text: args.name.clone(),
        format: "text".to_string(),
        details: None,
    }];
    let mut images = vec![];
    let mut tags = args.tags;
    let mut best_input_text = None;

    println!("Recipe name: {}", args.name);

    if let Some([source_name, details, format, content_text]) =
        args.direct.as_deref()
    {
        revisions.push(basic_models::RevisionForUpload {
            source_name: source_name.clone(),
            content_text: content_text.clone(),
            format: format.clone(),
            details: Some(details.clone()),
        });
    }

    if let Some(device_number) = args.webcam {
        println!("Webcam mode enabled");
        let mut picture = ingestion::take_picture(device_number)?;
        picture = match args.rotate.unwrap_or(Rotate::R0) {
            Rotate::R0 => picture,
            Rotate::R90 => picture.rotate90(),
            Rotate::R180 => picture.rotate180(),
            Rotate::R270 => picture.rotate270(),
        };

        let content_text = ingestion::read_text_from_image(&picture).await?;

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
        let better_text = gk::ingestion::freestyle(&args.name).await?;
        best_input_text = Some(better_text.clone());
        revisions.push(basic_models::RevisionForUpload {
            source_name: "llm".to_string(),
            content_text: better_text,
            format: "markdown".to_string(),
            details: Some("{\"model\": \"gpt-4o-mini\"}".to_string()),
        });
        tags.push("freestyle".to_string());
    }

    if let Some(content_text) = best_input_text {
        if !args.freestyle {
            let better_text = gk::ingestion::improve_recipe_with_llm(&content_text).await?;

            revisions.push(basic_models::RevisionForUpload {
                source_name: "llm".to_string(),
                content_text: better_text,
                format: "markdown".to_string(),
                details: Some("{\"model\": \"gpt-4o-mini\"}".to_string()),
            });
        }
    }

    if args.illustrate {
        images.extend_from_slice(&gk::ingestion::illustrate_recipe(&args.name).await?);
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

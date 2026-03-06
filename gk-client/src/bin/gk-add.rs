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
    /// LLM provider (openai or ollama)
    #[arg(long)]
    llm_provider: Option<String>,
    /// LLM model name (e.g., gpt-4o-mini, llama3.1)
    #[arg(long)]
    llm_model: Option<String>,
    /// Ollama base URL
    #[arg(long)]
    ollama_base_url: Option<String>,
    /// Diffusion server base URL
    #[arg(long)]
    diffusion_base_url: Option<String>,
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
        best_input_text.get_or_insert_with(|| content_text.clone());
    }

    let llm_config = ingestion::LlmConfig {
        provider: args.llm_provider.clone()
            .or_else(|| dotenvy::var("LLM_PROVIDER").ok())
            .unwrap_or_else(|| "openai".to_string()),
        model: args.llm_model.clone()
            .or_else(|| dotenvy::var("LLM_MODEL").ok()),
        ollama_base_url: args.ollama_base_url.clone()
            .or_else(|| dotenvy::var("OLLAMA_BASE_URL").ok())
            .unwrap_or_else(|| "http://localhost:11434/v1".to_string()),
    };

    if let Some(device_number) = args.webcam {
        println!("Webcam mode enabled");
        let mut picture = ingestion::take_picture(device_number)?;
        picture = match args.rotate.unwrap_or(Rotate::R0) {
            Rotate::R0 => picture,
            Rotate::R90 => picture.rotate90(),
            Rotate::R180 => picture.rotate180(),
            Rotate::R270 => picture.rotate270(),
        };

        let content_text = ingestion::read_text_from_image(&llm_config, &picture).await?;

        let webp_bytes = ingestion::convert_to_webp(&picture, 75.0)?;
        images.push(basic_models::ImageForUpload {
            category: "raw scan".to_string(),
            content_bytes: webp_bytes,
            prompt: None,
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
        let better_text = ingestion::freestyle(&llm_config, &args.name).await?;
        best_input_text = Some(better_text.clone());
        revisions.push(basic_models::RevisionForUpload {
            source_name: "llm".to_string(),
            content_text: better_text,
            format: "markdown".to_string(),
            details: Some(format!("{{\"model\": \"{}\"}}", llm_config.get_model())),
        });
        tags.push("freestyle".to_string());
    }

    if let Some(content_text) = best_input_text.as_ref() {
        if !args.freestyle {
            let better_text = ingestion::improve_recipe_with_llm(&llm_config, content_text).await?;
            best_input_text = Some(better_text.clone());

            revisions.push(basic_models::RevisionForUpload {
                source_name: "llm".to_string(),
                content_text: better_text,
                format: "markdown".to_string(),
                details: Some(format!("{{\"model\": \"{}\"}}", llm_config.get_model())),
            });
        }
    }

    if args.illustrate {
        let diffusion_base_url = args.diffusion_base_url
            .or_else(|| dotenvy::var("DIFFUSION_BASE_URL").ok())
            .unwrap_or_else(|| "http://localhost:8000".to_string());
        let illustration_source = best_input_text
            .clone()
            .unwrap_or_else(|| args.name.clone());
        images.extend_from_slice(
            &ingestion::illustrate_recipe(&llm_config, &diffusion_base_url, &illustration_source)
                .await?,
        );
    }

    let recipe_upload = basic_models::RecipeForUpload {
        name: args.name,
        description: None,
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

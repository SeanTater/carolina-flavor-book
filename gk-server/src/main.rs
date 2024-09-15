use anyhow::{Context, Result};
use axum::{
    body::{self, Bytes},
    extract::{Path, State},
    http::StatusCode,
    response::{Html, Redirect},
    routing::{get, post},
    Form, Json, Router,
};
use clap::Parser;
use gk::basic_models;
use gk_server::{
    auth::ServicePrincipal,
    database::Database,
    errors::{WebError, WebResult},
    models::{FullRecipe, Image, Recipe},
    search,
};
use handlebars::Handlebars;
use serde::Deserialize;
use serde_json::json;

lazy_static::lazy_static! {
    static ref TEMPLATES: Handlebars<'static> = handlebars();
}

#[derive(Parser, Debug)]
struct Args {
    /// The address and optionally port to bind to
    #[clap(long, default_value = "0.0.0.0:3000")]
    address: String,

    /// Whether to use HTTPS / TLS
    #[clap(long)]
    tls: bool,
}

#[derive(Clone)]
struct AllStates {
    db: Database,
    doc_index: search::DocumentIndexHandle,
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // Parse command line arguments
    let args = Args::parse();

    // connect to the database
    let default_db = Database::connect_default()
        .await
        .context("Connecting to database")?;
    // setup an embedding model
    let embedder = search::model::EmbeddingModel::new().context("Building embedding model")?;
    let document_index = search::DocumentIndexHandle::new(default_db.clone(), embedder);
    document_index
        .refresh_index()
        .context("Refreshing document index")?;
    // use the embeddings to index the recipes in the background
    tokio::spawn(document_index.clone().background_index());

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        // `GET /health` goes to `health`
        .route("/health", get(health))
        // `GET /recipe/:recipe_id` goes to `get_recipe`
        .route("/recipe/:recipe_id", get(get_recipe))
        // `POST /search` goes to `search_recipes`
        .route("/search", get(search_recipes))
        // `GET /api/recipe_without_enough_images` goes to `get_any_recipe_without_enough_images`
        .route(
            "/api/get-task/generate-image/:category",
            get(get_generate_image_task),
        )
        // `POST /api/upload_image/:recipe_id/:category` goes to `upload_image`
        .route("/api/image/:recipe_id/:category", post(upload_image))
        // `POST /api/upload_recipe` goes to `upload_recipe`
        .route("/api/recipe", post(upload_recipe))
        // serve static files from the `./src/static` directory
        .nest(
            "/static",
            axum_static::static_router("./static").with_state(()),
        )
        .layer(
            tower_http::compression::CompressionLayer::new()
                .quality(tower_http::CompressionLevel::Fastest),
        )
        .with_state(AllStates {
            db: default_db,
            doc_index: document_index.clone(),
        });

    // In development, use HTTP. In production, use HTTPS.

    if args.tls {
        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("Failed to install rustls crypto provider");
        let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(
            "/etc/letsencrypt/live/gallagher.kitchen/fullchain.pem",
            "/etc/letsencrypt/live/gallagher.kitchen/privkey.pem",
        )
        .await
        .context("Loading TLS certificate")?;

        let addr = args.address.parse()?;
        tracing::info!("Listening on {}", addr);
        axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service())
            .await
            .context("Starting TLS server")?;
    } else {
        let listener = tokio::net::TcpListener::bind(args.address).await?;
        axum::serve(listener, app).await?;
    }
    Ok(())
}

fn handlebars() -> Handlebars<'static> {
    let mut reg = Handlebars::new();
    reg.register_template_string("index", include_str!("templates/index.hbs"))
        .unwrap();
    reg.register_template_string("recipe", include_str!("templates/recipe.hbs"))
        .unwrap();
    reg.register_template_string("search", include_str!("templates/search.hbs"))
        .unwrap();

    // Register partials
    reg.register_partial("header", include_str!("templates/header.hbs"))
        .unwrap();
    reg.register_partial("footer", include_str!("templates/footer.hbs"))
        .unwrap();

    reg
}

// Render the home page
async fn root(State(allstates): State<AllStates>) -> WebResult<Html<String>> {
    Ok(Html(TEMPLATES.render(
        "index",
        &json!({"recipes": Recipe::list_some(&allstates.db)?}),
    )?))
}

// Just reply that everything is okay
async fn health() -> StatusCode {
    StatusCode::OK
}

async fn get_recipe(
    State(allstates): State<AllStates>,
    Path(recipe_id): Path<i64>,
) -> WebResult<Html<String>> {
    let recipe = Recipe::get_full_recipe(&allstates.db, recipe_id)?.ok_or(WebError::NotFound)?;
    Ok(Html(TEMPLATES.render("recipe", &recipe).unwrap()))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    query: String,
}

async fn search_recipes(
    State(allstates): State<AllStates>,
    Form(search_query): Form<SearchQuery>,
) -> WebResult<Html<String>> {
    let results = allstates.doc_index.search(&search_query.query, 20)?;
    Ok(Html(TEMPLATES.render(
        "search",
        &json!({
            "query": &search_query.query,
            "results": results,
        }),
    )?))
}

/// Get a recipe that does not have enough images, so that we can generate some AI-generated images for it.
/// This is needed because it requires a lot of resources to generate images, so we want to do it in the background,
/// not in this server, which does not have a GPU and is not optimized for image generation.
async fn get_generate_image_task(
    State(allstates): State<AllStates>,
    Path(category): Path<String>,
    _: ServicePrincipal,
) -> WebResult<Json<Option<FullRecipe>>> {
    let recipe = Recipe::get_any_recipe_without_enough_images(&allstates.db, &category)?;
    Ok(Json(recipe))
}

/// Upload an image for a recipe.
async fn upload_image(
    State(allstates): State<AllStates>,
    Path((recipe_id, category)): Path<(i64, String)>,
    _: ServicePrincipal,
    image_bytes: body::Bytes,
) -> WebResult<StatusCode> {
    Image::push(
        &allstates.db,
        recipe_id,
        basic_models::ImageForUpload {
            category,
            content_bytes: image_bytes.to_vec(),
        },
    )?;
    Ok(StatusCode::OK)
}

/// Upload a recipe and associated information
async fn upload_recipe(
    State(allstates): State<AllStates>,
    _: ServicePrincipal,
    body: Bytes,
) -> WebResult<Redirect> {
    let recipe_upload = bincode::deserialize(&body[..]).context("Deserializing recipe")?;
    let recipe_id = Recipe::push(&allstates.db, recipe_upload)?;
    Ok(Redirect::to(&format!("/recipe/{}", recipe_id)))
}

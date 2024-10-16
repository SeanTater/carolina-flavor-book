use anyhow::{Context, Result};
use axum::{
    body::{self, Bytes},
    extract::{FromRef, Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Json, Router,
};
use base64::Engine;
use clap::Parser;
use gk::basic_models;
use gk_server::{
    auth::{self, session::UserSession, OauthClient},
    config::Config,
    database::Database,
    errors::{WebError, WebResult},
    models::{FullRecipe, Image, ImageContent, Recipe},
    search::{self, DocumentIndexHandle},
};
use minijinja::context;
use rand::seq::SliceRandom;
use serde::Deserialize;
use tracing_subscriber::EnvFilter;

/// Convert a webp image to a data URL
/// This is duplicated in the client and server, but the implementation is different.
/// This one operates on owned options because that's much more convenient for minijinja filters.
fn to_data_url(bytes: Option<Vec<u8>>) -> Option<String> {
    bytes.map(|b| {
        format!(
            "data:image/webp;base64,{}",
            // For the purpose of data urls, you do NOT need to use the URL_SAFE variant
            base64::engine::general_purpose::STANDARD.encode(b)
        )
    })
}

lazy_static::lazy_static! {
    static ref TEMPLATES: minijinja::Environment<'static> = {
        let mut env = minijinja::Environment::new();
        for (name, template) in &[
            ("index.html.jinja", include_str!("../templates/index.html.jinja")),
            ("recipe.html.jinja", include_str!("../templates/recipe.html.jinja")),
            ("search.html.jinja", include_str!("../templates/search.html.jinja")),
            ("base.html.jinja", include_str!("../templates/base.html.jinja")),
            ("browse-by-tag.html.jinja", include_str!("../templates/browse-by-tag.html.jinja")),
        ] {
            env.add_template(name, template)
                .expect("Failed to register template");
        }
        env.add_filter("to_data_url", to_data_url);
        env
    };
}

#[derive(Parser, Debug)]
struct Args {
    /// The path to the yaml configuration file
    config_path: String,
}

#[derive(Clone, FromRef)]
struct AppState {
    db: Database,
    doc_index: search::DocumentIndexHandle,
    oauth: OauthClient,
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    let file_appender = tracing_appender::rolling::daily(
        if std::fs::exists("/app")? {
            "/app/data/logs".into()
        } else {
            std::env::current_dir()?
        },
        "access.log",
    );
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .json()
        .with_writer(non_blocking)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Parse command line arguments
    let args = Args::parse();

    // Load the configuration file
    let config = Config::load(&args.config_path).context("Parsing configuration")?;

    // Connect to the database
    let default_db = Database::connect(&config.database)
        .await
        .context("Connecting to database")?;

    // Setup oauth, sessions, and authentication
    let oauth = OauthClient::new_from_config(&config.auth)
        .await
        .context("Setting up authentication")?;

    // Setup an embedding model and the search engine
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
        // `GET /browse/by-tag` goes to `browse_by_tag`
        .route("/browse/by-tag", get(browse_by_tag))
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
        // `GET /api/image/:image_id` goes to `get_image`
        .route("/image/:image_id", get(get_image))
        // `POST /api/upload_image/:recipe_id/:category` goes to `upload_image`
        .route("/api/image/:recipe_id/:category", post(upload_image))
        // `POST /api/upload_recipe` goes to `upload_recipe`
        .route("/api/recipe", post(upload_recipe))
        // serve static files from the `./src/static` directory
        .route("/static/*path", get(serve_static))
        .route("/auth/login", get(auth::route::login))
        .route("/auth/callback", get(auth::route::oauth_callback))
        .route("/auth/logout", get(auth::route::logout))
        .layer(
            tower_http::compression::CompressionLayer::new()
                .quality(tower_http::CompressionLevel::Fastest),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(AppState {
            db: default_db,
            doc_index: document_index.clone(),
            oauth,
        });

    // In development, use HTTP. In production, use HTTPS.

    if let Some(tls) = config.server.tls {
        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("Failed to install rustls crypto provider");
        let tls_config =
            axum_server::tls_rustls::RustlsConfig::from_pem_file(tls.cert_path, tls.key_path)
                .await
                .context("Loading TLS certificate")?;

        let addr = config.server.address.parse()?;
        tracing::info!("Listening on {}", addr);
        axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service())
            .await
            .context("Starting TLS server")?;
    } else {
        let listener = tokio::net::TcpListener::bind(config.server.address).await?;
        axum::serve(listener, app).await?;
    }
    Ok(())
}

// Render the home page with 20 random recipes
async fn root(State(db): State<Database>) -> WebResult<Html<String>> {
    let all_recipes = Recipe::get_all_basics(&db)?;
    let some_random_ids = all_recipes
        .choose_multiple(&mut rand::thread_rng(), 20)
        .map(|r| r.recipe_id)
        .collect::<Vec<_>>();
    Ok(Html(TEMPLATES.get_template("index.html.jinja")?.render(
        context! {
            recipes => Recipe::get_extended(&db, &some_random_ids)?,
        },
    )?))
}

/// Render the browse page, which shows all the recipes for all the tags, grouped by tag,
/// and two recipes per tag with highlights
async fn browse_by_tag(
    State(db): State<Database>,
    State(doc_index): State<DocumentIndexHandle>,
) -> WebResult<Html<String>> {
    let mut recipes_by_tag = vec![];
    for (tag_name, results) in doc_index.search_tags()? {
        let highlights = results
            .choose_multiple(&mut rand::thread_rng(), 2)
            .map(|r| r.recipe.recipe_id)
            .collect::<Vec<_>>();
        recipes_by_tag.push(context! {
            tag_name => tag_name,
            highlight_recipes => Recipe::get_extended(&db, &highlights)?,
            all_recipes => results
        });
    }
    let page = TEMPLATES
        .get_template("browse-by-tag.html.jinja")?
        .render(context! {
            recipes_by_tag => recipes_by_tag,
        })?;

    Ok(Html(page))
}

// Just reply that everything is okay
async fn health() -> StatusCode {
    StatusCode::OK
}

async fn get_recipe(
    State(db): State<Database>,
    Path(recipe_id): Path<i64>,
) -> WebResult<Html<String>> {
    let recipe = Recipe::get_full_recipe(&db, recipe_id)?.ok_or(WebError::NotFound)?;
    Ok(Html(TEMPLATES.get_template("recipe.html.jinja")?.render(
        context! {
            recipe => recipe,
        },
    )?))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    query: String,
    #[serde(default)]
    page: usize,
}

async fn search_recipes(
    State(doc_index): State<DocumentIndexHandle>,
    Form(search_query): Form<SearchQuery>,
) -> WebResult<impl IntoResponse> {
    let results = doc_index.search(&search_query.query, search_query.page * 20, 20)?;
    Ok(Html(TEMPLATES.get_template("search.html.jinja")?.render(
        context! {
            query => search_query.query,
            results => results,
            page => search_query.page,
        },
    )?))
}

async fn get_image(
    State(db): State<Database>,
    Path(image_id): Path<i64>,
) -> WebResult<impl IntoResponse> {
    let image = ImageContent::get_image_content(&db, image_id)?.ok_or(WebError::NotFound)?;
    Ok(([(header::CONTENT_TYPE, "image/webp")], image.content_bytes))
}

/// Get a recipe that does not have enough images, so that we can generate some AI-generated images for it.
/// This is needed because it requires a lot of resources to generate images, so we want to do it in the background,
/// not in this server, which does not have a GPU and is not optimized for image generation.
async fn get_generate_image_task(
    State(db): State<Database>,
    Path(category): Path<String>,
    _: UserSession,
) -> WebResult<Json<Option<FullRecipe>>> {
    let recipe = Recipe::get_any_recipe_without_enough_images(&db, &category)?;
    Ok(Json(recipe))
}

/// Upload an image for a recipe.
async fn upload_image(
    State(db): State<Database>,
    Path((recipe_id, category)): Path<(i64, String)>,
    _: UserSession,
    image_bytes: body::Bytes,
) -> WebResult<StatusCode> {
    Image::push(
        &db,
        recipe_id,
        basic_models::ImageForUpload {
            category,
            content_bytes: image_bytes.to_vec(),
        },
    )
    .await?;
    Ok(StatusCode::OK)
}

/// Upload a recipe and associated information
async fn upload_recipe(
    State(db): State<Database>,
    _: UserSession,
    body: Bytes,
) -> WebResult<Redirect> {
    let recipe_upload = bincode::deserialize(&body[..]).context("Deserializing recipe")?;
    let recipe_id = Recipe::push(&db, recipe_upload).await?;
    Ok(Redirect::to(&format!("/recipe/{}", recipe_id)))
}

/// Serve static files from in memory using `include_dir!`
async fn serve_static(Path(path): Path<String>) -> WebResult<impl IntoResponse> {
    let dir = include_dir::include_dir!("$CARGO_MANIFEST_DIR/static");
    let bytes = dir.get_file(&path).ok_or(WebError::NotFound)?.contents();
    let header = (
        "Content-Type",
        match path.split('.').last() {
            Some("css") => "text/css",
            Some("js") => "text/javascript",
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("svg") => "image/svg+xml",
            Some("webp") => "image/webp",
            _ => "application/octet-stream",
        },
    );
    Ok(([header], bytes).into_response())
}

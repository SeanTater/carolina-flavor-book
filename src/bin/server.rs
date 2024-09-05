use anyhow::Result;
use axum::{
    body::{self, Body},
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Form, Json, Router,
};
use handlebars::Handlebars;
use recipes::{
    database::Database,
    errors::{WebError, WebResult},
    models::{FullRecipe, Image, Recipe},
};
use serde::Deserialize;
use serde_json::json;

lazy_static::lazy_static! {
    static ref TEMPLATES: Handlebars<'static> = handlebars();
}

#[derive(Clone)]
struct AllStates {
    db: Database,
    doc_index: recipes::search::DocumentIndexHandle,
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    // connect to the database
    let default_db = Database::connect_default().await?;
    // setup an embedding model
    let embedder = recipes::search::model::EmbeddingModel::new()?;
    let document_index = recipes::search::DocumentIndexHandle::new(default_db.clone(), embedder);
    document_index.refresh_index()?;
    // use the embeddings to index the recipes in the background
    tokio::spawn(document_index.clone().background_index());

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
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
        .route("/api/upload-image/:recipe_id/:category", post(upload_image))
        .nest(
            "/static",
            axum_static::static_router("./src/static").with_state(()),
        )
        .with_state(AllStates {
            db: default_db,
            doc_index: document_index.clone(),
        });

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn handlebars() -> Handlebars<'static> {
    let mut reg = Handlebars::new();
    reg.register_template_string("index", include_str!("../templates/index.hbs"))
        .unwrap();
    reg.register_template_string("recipe", include_str!("../templates/recipe.hbs"))
        .unwrap();
    reg.register_template_string("search", include_str!("../templates/search.hbs"))
        .unwrap();

    // Register partials
    reg.register_partial("header", include_str!("../templates/header.hbs"))
        .unwrap();
    reg.register_partial("footer", include_str!("../templates/footer.hbs"))
        .unwrap();

    reg
}

// basic handler that responds with a static string
async fn root(State(allstates): State<AllStates>) -> WebResult<Html<String>> {
    Ok(Html(TEMPLATES.render(
        "index",
        &json!({"recipes": Recipe::list_all(&allstates.db)?}),
    )?))
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
) -> WebResult<Json<Option<FullRecipe>>> {
    let recipe = Recipe::get_any_recipe_without_enough_images(&allstates.db, &category)?;
    Ok(Json(recipe))
}

/// Upload an image for a recipe.
async fn upload_image(
    State(allstates): State<AllStates>,
    Path((recipe_id, category)): Path<(i64, String)>,
    image_bytes: body::Bytes,
) -> WebResult<StatusCode> {
    Image::upload(&allstates.db, recipe_id, &category, &image_bytes[..])?;
    Ok(StatusCode::OK)
}

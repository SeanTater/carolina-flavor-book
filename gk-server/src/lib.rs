pub mod auth;
pub mod config;
pub mod database;
pub mod errors;
pub mod models;
pub mod search;

use axum::{
    body::Bytes,
    extract::{FromRef, Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Json, Router,
};
use base64::Engine;
use errors::{WebError, WebResult};
use gk::basic_models;
use minijinja::context;
use models::{FullRecipe, Image, ImageContent, Recipe};
use rand::seq::SliceRandom;
use search::DocumentIndexHandle;
use serde::Deserialize;

use auth::{session::AuthenticatedUser, AuthService};

/// Convert a webp image to a data URL.
fn to_data_url(bytes: Option<Vec<u8>>) -> Option<String> {
    bytes.map(|b| {
        format!(
            "data:image/webp;base64,{}",
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

#[derive(Clone, FromRef)]
pub struct AppState {
    pub db: database::Database,
    pub doc_index: DocumentIndexHandle,
    pub auth: AuthService,
}

/// Build the full application router with all routes.
pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/browse/by-tag", get(browse_by_tag))
        .route("/health", get(health))
        .route("/api/auth/check", get(auth_check))
        .route("/recipe/{recipe_id}", get(get_recipe))
        .route("/search", get(search_recipes))
        .route(
            "/api/get-task/generate-image/{category}",
            get(get_generate_image_task),
        )
        .route("/image/{image_id}", get(get_image))
        .route("/api/image/{recipe_id}/{category}", post(upload_image))
        .route("/api/recipe", post(upload_recipe))
        .route("/static/{*path}", get(serve_static))
        .route("/auth/login", get(auth::route::login_page).post(auth::route::login_submit))
        .route("/auth/logout", get(auth::route::logout))
        .layer(
            tower_http::compression::CompressionLayer::new()
                .quality(tower_http::CompressionLevel::Fastest),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

async fn root(State(db): State<database::Database>) -> WebResult<Html<String>> {
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

async fn browse_by_tag(
    State(db): State<database::Database>,
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

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn auth_check(user: AuthenticatedUser) -> Json<serde_json::Value> {
    match user {
        AuthenticatedUser::Session(session) => Json(serde_json::json!({
            "authenticated": true,
            "method": "session",
            "username": session.username,
        })),
        AuthenticatedUser::ServicePrincipal => Json(serde_json::json!({
            "authenticated": true,
            "method": "service_principal",
        })),
    }
}

async fn get_recipe(
    State(db): State<database::Database>,
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
    State(db): State<database::Database>,
    Path(image_id): Path<i64>,
) -> WebResult<impl IntoResponse> {
    let image = ImageContent::get_image_content(&db, image_id)?.ok_or(WebError::NotFound)?;
    Ok(([(header::CONTENT_TYPE, "image/webp")], image.content_bytes))
}

async fn get_generate_image_task(
    State(db): State<database::Database>,
    Path(category): Path<String>,
    _: AuthenticatedUser,
) -> WebResult<Json<Option<FullRecipe>>> {
    let recipe = Recipe::get_any_recipe_without_enough_images(&db, &category)?;
    Ok(Json(recipe))
}

async fn upload_image(
    State(db): State<database::Database>,
    Path((recipe_id, category)): Path<(i64, String)>,
    _: AuthenticatedUser,
    image_bytes: axum::body::Bytes,
) -> WebResult<StatusCode> {
    Image::push(
        &db,
        recipe_id,
        basic_models::ImageForUpload {
            category,
            content_bytes: image_bytes.to_vec(),
            prompt: None,
        },
    )
    .await?;
    Ok(StatusCode::OK)
}

async fn upload_recipe(
    State(db): State<database::Database>,
    _: AuthenticatedUser,
    body: Bytes,
) -> WebResult<Redirect> {
    let recipe_upload = bincode::deserialize(&body[..])
        .map_err(|e| anyhow::anyhow!("Deserializing recipe: {e}"))?;
    let recipe_id = Recipe::push(&db, recipe_upload).await?;
    Ok(Redirect::to(&format!("/recipe/{}", recipe_id)))
}

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

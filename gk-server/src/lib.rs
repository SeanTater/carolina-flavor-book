pub mod auth;
pub mod config;
pub mod database;
pub mod errors;
pub mod models;
pub mod search;

use axum::{
    body::Bytes,
    extract::{FromRef, Multipart, Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, patch, post},
    Form, Json, Router,
};
use base64::Engine;
use chrono::Datelike;
use errors::{WebError, WebResult};
use gk::basic_models;
use minijinja::context;
use models::{Article, Author, FrontPageSection, FullRecipe, Image, ImageContent, Recipe, Revision};
use rand::{seq::SliceRandom, SeedableRng};
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
            ("create-recipe.html.jinja", include_str!("../templates/create-recipe.html.jinja")),
            ("login-required.html.jinja", include_str!("../templates/login-required.html.jinja")),
            ("article.html.jinja", include_str!("../templates/article.html.jinja")),
            ("articles.html.jinja", include_str!("../templates/articles.html.jinja")),
        ] {
            env.add_template(name, template)
                .expect("Failed to register template");
        }
        env.add_filter("to_data_url", to_data_url);
        env
    };
}

/// Tag axes loaded from recipe-grid.toml for daily highlight sections.
#[derive(Clone, Default)]
pub struct TagAxes {
    pub cuisine: Vec<String>,
    pub occasion: Vec<String>,
    pub season: Vec<String>,
}

impl TagAxes {
    /// Parse tag axes from recipe-grid.toml content.
    pub fn from_toml(text: &str) -> anyhow::Result<Self> {
        let val: toml::Value = toml::from_str(text)?;
        let axes = val.get("axes").and_then(|a| a.as_table());
        let get_tags = |name: &str| -> Vec<String> {
            axes.and_then(|a| a.get(name))
                .and_then(|v| v.get("tags"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };
        Ok(Self {
            cuisine: get_tags("cuisine"),
            occasion: get_tags("occasion"),
            season: get_tags("season"),
        })
    }

    /// Load from the embedded recipe-grid.toml.
    pub fn load() -> Self {
        Self::from_toml(include_str!("../../config/recipe-grid.toml"))
            .expect("embedded recipe-grid.toml should be valid")
    }
}

/// A highlight section for the front page.
#[derive(serde::Serialize)]
struct Highlight {
    title: String,
    recipes: Vec<Recipe>,
}

/// Build deterministic daily highlight sections from tag axes.
fn daily_highlights(db: &database::Database, axes: &TagAxes) -> Vec<Highlight> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // Seed RNG deterministically from today's date
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    today.hash(&mut hasher);
    let seed = hasher.finish();
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

    let mut highlights = Vec::new();

    // 1. Season highlight
    let current_season = match chrono::Local::now().month() {
        3..=5 => "spring",
        6..=8 => "summer",
        9..=11 => "fall",
        _ => "winter",
    };
    if axes.season.contains(&current_season.to_string()) {
        if let Some(h) = build_highlight(
            db,
            &mut rng,
            format!("In Season: {}", capitalize(current_season)),
            &[current_season.to_string()],
            1,
        ) {
            highlights.push(h);
        }
    }

    // 2. Occasion highlight — everything except "dinner"
    let occasion_tags: Vec<String> = axes
        .occasion
        .iter()
        .filter(|t| *t != "dinner")
        .cloned()
        .collect();
    if !occasion_tags.is_empty() {
        if let Some(h) = build_highlight(db, &mut rng, "Not Just Dinner".into(), &occasion_tags, 1)
        {
            highlights.push(h);
        }
    }

    // 3. Cuisine highlight — everything except american-*
    let cuisine_tags: Vec<String> = axes
        .cuisine
        .iter()
        .filter(|t| !t.starts_with("american"))
        .cloned()
        .collect();
    if !cuisine_tags.is_empty() {
        if let Some(h) = build_highlight(db, &mut rng, "World Kitchen".into(), &cuisine_tags, 1) {
            highlights.push(h);
        }
    }

    highlights
}

fn build_highlight(
    db: &database::Database,
    rng: &mut rand::rngs::StdRng,
    title: String,
    tags: &[String],
    count: usize,
) -> Option<Highlight> {
    let mut ids =
        models::FrontPageSection::get_recipe_ids_for_tags(db, tags, 200).unwrap_or_default();
    if ids.is_empty() {
        return None;
    }
    ids.shuffle(rng);
    // Fetch more than we need so we can filter for ones with images
    let fetch_count = (count * 5).min(ids.len());
    let recipes = Recipe::get_extended(db, &ids[..fetch_count]).unwrap_or_default();
    let recipes: Vec<_> = recipes
        .into_iter()
        .filter(|r| r.thumbnail_image_id.is_some())
        .take(count)
        .collect();
    if recipes.is_empty() {
        return None;
    }
    Some(Highlight { title, recipes })
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

#[derive(Clone, FromRef)]
pub struct AppState {
    pub db: database::Database,
    pub doc_index: DocumentIndexHandle,
    pub auth: AuthService,
    pub tag_axes: TagAxes,
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
        .route("/api/search", get(api_search_recipes))
        .route(
            "/api/get-task/generate-image/{category}",
            get(get_generate_image_task),
        )
        .route("/image/{image_id}", get(get_image))
        .route("/api/image/{recipe_id}/{category}", post(upload_image)
            .layer(axum::extract::DefaultBodyLimit::max(20 * 1024 * 1024)))
        .route("/recipe/new", get(create_recipe_page))
        .route("/recipe/save", post(save_recipe)
            .layer(axum::extract::DefaultBodyLimit::max(20 * 1024 * 1024)))
        .route("/recipe/{recipe_id}/edit", get(edit_recipe_page)
            .post(update_recipe)
            .layer(axum::extract::DefaultBodyLimit::max(20 * 1024 * 1024)))
        .route("/api/recipe", post(upload_recipe))
        .route("/api/recipe/{recipe_id}", patch(patch_recipe))
        .route("/api/tags/{recipe_id}", post(add_tags))
        .route("/api/tags", get(get_all_tags))
        .route("/api/recipes/basic", get(get_all_basics_api))
        .route("/api/recipes/missing-images", get(get_recipes_missing_images))
        .route("/api/recipes/text", get(get_all_recipes_text))
        .route("/article/{slug}", get(view_article))
        .route("/articles", get(list_articles))
        .route("/api/article", post(create_article))
        .route("/api/author", post(upsert_author))
        .route("/api/schedule", post(upsert_schedule))
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

async fn root(
    State(db): State<database::Database>,
    State(tag_axes): State<TagAxes>,
) -> WebResult<Html<String>> {
    let all_recipes = Recipe::get_all_basics(&db)?;
    let some_random_ids = all_recipes
        .choose_multiple(&mut rand::thread_rng(), 20)
        .map(|r| r.recipe_id)
        .collect::<Vec<_>>();
    let today = chrono::Local::now().format("%m-%d").to_string();
    let sections = FrontPageSection::get_for_date(&db, &today).unwrap_or_default();
    // For each section, resolve query_tags to actual recipes
    let mut section_data = vec![];
    for section in &sections {
        let tags: Vec<String> = serde_json::from_str(&section.query_tags).unwrap_or_default();
        let recipe_ids = FrontPageSection::get_recipe_ids_for_tags(&db, &tags, 6)?;
        let recipes = Recipe::get_extended(&db, &recipe_ids)?;
        section_data.push(context! {
            title => section.title,
            blurb => section.blurb,
            section => section.section,
            recipes => recipes,
        });
    }
    let highlights = daily_highlights(&db, &tag_axes);
    let articles = Article::get_published(&db, 2)?;
    let mut article_data = vec![];
    for article in &articles {
        let linked_ids = Article::get_linked_recipe_ids(&db, article.article_id)?;
        let first_recipe = if let Some(&rid) = linked_ids.first() {
            Recipe::get_extended(&db, &[rid])?.into_iter().next()
        } else {
            None
        };
        article_data.push(context! {
            article => article,
            recipe_image_id => first_recipe.and_then(|r| r.thumbnail_image_id),
        });
    }
    Ok(Html(TEMPLATES.get_template("index.html.jinja")?.render(
        context! {
            recipes => Recipe::get_extended(&db, &some_random_ids)?,
            sections => section_data,
            highlights => highlights,
            articles => article_data,
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

async fn api_search_recipes(
    State(doc_index): State<DocumentIndexHandle>,
    axum::extract::Query(search_query): axum::extract::Query<SearchQuery>,
) -> WebResult<Json<Vec<serde_json::Value>>> {
    let results = doc_index.search(&search_query.query, search_query.page * 20, 20)?;
    Ok(Json(results.iter().map(|r| serde_json::json!({
        "recipe_id": r.recipe.recipe_id,
        "name": r.recipe.name,
        "relevance": r.relevance_percent,
    })).collect()))
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

async fn create_recipe_page(_user: AuthenticatedUser) -> WebResult<Html<String>> {
    Ok(Html(
        TEMPLATES
            .get_template("create-recipe.html.jinja")?
            .render(context! {})?,
    ))
}

async fn save_recipe(
    _user: AuthenticatedUser,
    State(db): State<database::Database>,
    mut multipart: Multipart,
) -> WebResult<impl IntoResponse> {
    let mut name: Option<String> = None;
    let mut content: Option<String> = None;
    let mut image_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| anyhow::anyhow!("Reading multipart field: {e}"))?
    {
        match field.name() {
            Some("name") => {
                name = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| anyhow::anyhow!("Reading name: {e}"))?,
                );
            }
            Some("content") => {
                content = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| anyhow::anyhow!("Reading content: {e}"))?,
                );
            }
            Some("image") => {
                image_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| anyhow::anyhow!("Reading image: {e}"))?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }

    let name = name.filter(|s| !s.is_empty()).ok_or_else(|| anyhow::anyhow!("Missing recipe name"))?;
    let content = content.filter(|s| !s.is_empty()).ok_or_else(|| anyhow::anyhow!("Missing recipe content"))?;
    let image_bytes = image_bytes.filter(|b| !b.is_empty()).ok_or_else(|| anyhow::anyhow!("Missing image"))?;

    let upload = basic_models::RecipeForUpload {
        name: name.clone(),
        description: None,
        tags: vec!["manual".into()],
        revisions: vec![basic_models::RevisionForUpload {
            source_name: "manual".into(),
            content_text: content,
            format: "markdown".into(),
            details: None,
        }],
        images: vec![],
    };
    let recipe_id = Recipe::push(&db, upload).await?;

    // Convert uploaded image (any format) to WebP for Image::push
    let img = image::load_from_memory(&image_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid image: {e}"))?;
    let webp_bytes = webp::Encoder::from_image(&img)
        .map_err(|e| anyhow::anyhow!("WebP encoding: {e:?}"))?
        .encode(75.0)
        .to_vec();

    Image::push(
        &db,
        recipe_id,
        basic_models::ImageForUpload {
            category: "user-upload".into(),
            content_bytes: webp_bytes,
            prompt: None,
        },
    )
    .await?;

    Ok(Redirect::to(&format!("/recipe/{}", recipe_id)))
}

async fn edit_recipe_page(
    _user: AuthenticatedUser,
    State(db): State<database::Database>,
    Path(recipe_id): Path<i64>,
) -> WebResult<Html<String>> {
    let recipe = Recipe::get_full_recipe(&db, recipe_id)?.ok_or(WebError::NotFound)?;
    Ok(Html(
        TEMPLATES
            .get_template("create-recipe.html.jinja")?
            .render(context! { recipe => recipe })?,
    ))
}

async fn update_recipe(
    _user: AuthenticatedUser,
    State(db): State<database::Database>,
    Path(recipe_id): Path<i64>,
    mut multipart: Multipart,
) -> WebResult<impl IntoResponse> {
    let mut name: Option<String> = None;
    let mut content: Option<String> = None;
    let mut image_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| anyhow::anyhow!("Reading multipart field: {e}"))?
    {
        match field.name() {
            Some("name") => {
                name = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| anyhow::anyhow!("Reading name: {e}"))?,
                );
            }
            Some("content") => {
                content = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| anyhow::anyhow!("Reading content: {e}"))?,
                );
            }
            Some("image") => {
                image_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| anyhow::anyhow!("Reading image: {e}"))?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }

    let name = name.filter(|s| !s.is_empty()).ok_or_else(|| anyhow::anyhow!("Missing recipe name"))?;
    let content = content.filter(|s| !s.is_empty()).ok_or_else(|| anyhow::anyhow!("Missing recipe content"))?;

    Recipe::update_name(&db, recipe_id, &name)?;
    Revision::push(
        &db,
        basic_models::RevisionForUpload {
            source_name: "manual".into(),
            content_text: content,
            format: "markdown".into(),
            details: None,
        },
        recipe_id,
    )?;

    if let Some(image_bytes) = image_bytes.filter(|b| !b.is_empty()) {
        let img = image::load_from_memory(&image_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid image: {e}"))?;
        let webp_bytes = webp::Encoder::from_image(&img)
            .map_err(|e| anyhow::anyhow!("WebP encoding: {e:?}"))?
            .encode(75.0)
            .to_vec();

        Image::push(
            &db,
            recipe_id,
            basic_models::ImageForUpload {
                category: "user-upload".into(),
                content_bytes: webp_bytes,
                prompt: None,
            },
        )
        .await?;
    }

    Ok(Redirect::to(&format!("/recipe/{}", recipe_id)))
}

async fn add_tags(
    State(db): State<database::Database>,
    Path(recipe_id): Path<i64>,
    _: AuthenticatedUser,
    Json(tags): Json<Vec<String>>,
) -> WebResult<StatusCode> {
    for tag in &tags {
        models::Tag::push(&db, recipe_id, tag)?;
    }
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct RecipePatch {
    name: Option<String>,
    description: Option<String>,
    content: Option<String>,
    tags: Option<TagPatch>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TagPatch {
    Set(Vec<String>),
    Ops(TagOps),
}

#[derive(Deserialize)]
struct TagOps {
    #[serde(default)]
    add: Vec<String>,
    #[serde(default)]
    remove: Vec<String>,
}

async fn patch_recipe(
    State(db): State<database::Database>,
    Path(recipe_id): Path<i64>,
    _: AuthenticatedUser,
    Json(patch): Json<RecipePatch>,
) -> WebResult<StatusCode> {
    if let Some(name) = &patch.name {
        Recipe::update_name(&db, recipe_id, name)?;
    }
    if let Some(description) = &patch.description {
        Recipe::update_description(&db, recipe_id, Some(description))?;
    }
    if let Some(content) = &patch.content {
        Revision::push(
            &db,
            basic_models::RevisionForUpload {
                source_name: "manual".into(),
                content_text: content.clone(),
                format: "markdown".into(),
                details: None,
            },
            recipe_id,
        )?;
    }
    if let Some(tags) = &patch.tags {
        match tags {
            TagPatch::Set(tags) => {
                models::Tag::set_for_recipe(&db, recipe_id, tags)?;
            }
            TagPatch::Ops(ops) => {
                for tag in &ops.add {
                    models::Tag::push(&db, recipe_id, tag)?;
                }
                models::Tag::remove(&db, recipe_id, &ops.remove)?;
            }
        }
    }
    Ok(StatusCode::OK)
}

async fn get_all_tags(
    State(db): State<database::Database>,
) -> WebResult<Json<Vec<models::Tag>>> {
    Ok(Json(models::Tag::get_all(&db)?))
}

async fn get_all_basics_api(
    State(db): State<database::Database>,
) -> WebResult<Json<Vec<Recipe>>> {
    Ok(Json(Recipe::get_all_basics(&db)?))
}

#[derive(serde::Deserialize)]
struct MissingImagesQuery {
    #[serde(default)]
    max_images: Option<i64>,
}

async fn get_recipes_missing_images(
    State(db): State<database::Database>,
    axum::extract::Query(query): axum::extract::Query<MissingImagesQuery>,
) -> WebResult<Json<Vec<serde_json::Value>>> {
    let threshold = query.max_images.unwrap_or(0);
    let conn = db.pool.get().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut stmt = conn.prepare(
        "SELECT Recipe.recipe_id, Recipe.name, COUNT(Image.image_id) AS image_count
         FROM Recipe
         LEFT JOIN Image ON Recipe.recipe_id = Image.recipe_id
         GROUP BY Recipe.recipe_id
         HAVING image_count <= ?
         ORDER BY image_count ASC, Recipe.recipe_id ASC",
    ).map_err(|e| anyhow::anyhow!("{e}"))?;
    let rows = stmt.query_map(rusqlite::params![threshold], |row| {
        Ok(serde_json::json!({
            "recipe_id": row.get::<_, i64>(0)?,
            "name": row.get::<_, String>(1)?,
            "image_count": row.get::<_, i64>(2)?,
        }))
    }).map_err(|e| anyhow::anyhow!("{e}"))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(Json(rows))
}

async fn get_all_recipes_text(
    State(db): State<database::Database>,
) -> WebResult<Json<Vec<models::RecipeWithText>>> {
    Ok(Json(Recipe::get_all_with_text(&db)?))
}

async fn upsert_schedule(
    State(db): State<database::Database>,
    _: AuthenticatedUser,
    Json(sections): Json<Vec<FrontPageSection>>,
) -> WebResult<StatusCode> {
    for section in &sections {
        FrontPageSection::upsert(&db, section)?;
    }
    Ok(StatusCode::OK)
}

async fn view_article(
    State(db): State<database::Database>,
    Path(slug): Path<String>,
) -> WebResult<Html<String>> {
    let article = Article::get_by_slug(&db, &slug)?.ok_or(WebError::NotFound)?;
    // Check publish date
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if article.publish_date > today {
        return Err(WebError::NotFound);
    }
    let author = Author::get_by_id(&db, &article.author_id)?;
    let recipe_ids = Article::get_linked_recipe_ids(&db, article.article_id)?;
    let linked_recipes = Recipe::get_extended(&db, &recipe_ids)?;
    Ok(Html(TEMPLATES.get_template("article.html.jinja")?.render(
        context! {
            article => article,
            author => author,
            linked_recipes => linked_recipes,
        },
    )?))
}

async fn list_articles(
    State(db): State<database::Database>,
) -> WebResult<Html<String>> {
    let articles = Article::get_published(&db, 100)?;
    Ok(Html(TEMPLATES.get_template("articles.html.jinja")?.render(
        context! { articles => articles },
    )?))
}

#[derive(Deserialize)]
struct CreateArticleRequest {
    author_id: String,
    title: String,
    slug: String,
    summary: Option<String>,
    content_text: String,
    publish_date: String,
    thumbnail_image_id: Option<i64>,
}

async fn create_article(
    State(db): State<database::Database>,
    _: AuthenticatedUser,
    Json(req): Json<CreateArticleRequest>,
) -> WebResult<Json<serde_json::Value>> {
    let article_id = Article::push(
        &db,
        &req.author_id,
        &req.title,
        &req.slug,
        req.summary.as_deref(),
        &req.content_text,
        &req.publish_date,
        req.thumbnail_image_id,
    )?;
    Ok(Json(serde_json::json!({ "article_id": article_id })))
}

#[derive(Deserialize)]
struct UpsertAuthorRequest {
    author_id: String,
    display_name: String,
    bio: String,
}

async fn upsert_author(
    State(db): State<database::Database>,
    _: AuthenticatedUser,
    Json(req): Json<UpsertAuthorRequest>,
) -> WebResult<StatusCode> {
    Author::upsert(&db, &Author {
        author_id: req.author_id,
        display_name: req.display_name,
        bio_rendered: String::new(), // not stored
        bio: req.bio,
    })?;
    Ok(StatusCode::OK)
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

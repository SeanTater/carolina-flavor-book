#[allow(dead_code)]
mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use gk_server::{
    build_app, AppState,
    models::{Article, Author, FrontPageSection, Recipe},
    search::DocumentIndexHandle,
};
use tower::ServiceExt;

async fn test_app_state() -> AppState {
    let db = common::test_db().await;
    let doc_index = DocumentIndexHandle::empty(db.clone());
    let auth = common::test_auth().await;
    AppState { db, doc_index, auth, tag_axes: Default::default() }
}

#[tokio::test]
async fn health_returns_ok() {
    let app = build_app(test_app_state().await);
    let resp = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn root_returns_html() {
    let app = build_app(test_app_state().await);
    let resp = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("<"), "response should contain HTML");
}

#[tokio::test]
async fn recipe_not_found() {
    let app = build_app(test_app_state().await);
    let resp = app
        .oneshot(Request::builder().uri("/recipe/9999").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn recipe_found() {
    let state = test_app_state().await;
    let id = Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let app = build_app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(&format!("/recipe/{}", id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Mix flour, sugar, cocoa"));
}

#[tokio::test]
async fn static_css_content_type() {
    let app = build_app(test_app_state().await);
    let resp = app
        .oneshot(Request::builder().uri("/static/index.css").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/css"
    );
}

#[tokio::test]
async fn unauthenticated_upload_rejected() {
    let app = build_app(test_app_state().await);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/recipe")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_recipe_page_requires_auth() {
    let app = build_app(test_app_state().await);
    let resp = app
        .oneshot(Request::builder().uri("/recipe/new").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_recipe_page_ok() {
    let app = build_app(test_app_state().await);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/recipe/new")
                .header("Authorization", "Bearer test-secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Add a Recipe"));
}

#[tokio::test]
async fn save_recipe_requires_auth() {
    let app = build_app(test_app_state().await);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/recipe/save")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Build a multipart/form-data body with the given fields.
fn build_multipart_body(
    name: &str,
    content: &str,
    image_bytes: &[u8],
) -> (String, Vec<u8>) {
    let boundary = "----TestBoundary12345";
    let mut body = Vec::new();

    // name field
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\n{name}\r\n").as_bytes());
    // content field
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"content\"\r\n\r\n{content}\r\n").as_bytes());
    // image field
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"test.png\"\r\nContent-Type: image/png\r\n\r\n").as_bytes());
    body.extend_from_slice(image_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// Build a multipart body with only name and content (no image).
fn build_multipart_body_no_image(name: &str, content: &str) -> (String, Vec<u8>) {
    let boundary = "----TestBoundary12345";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\n{name}\r\n").as_bytes());
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"content\"\r\n\r\n{content}\r\n").as_bytes());
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

#[tokio::test]
async fn save_recipe_creates_recipe_with_image() {
    let state = test_app_state().await;
    let db = state.db.clone();
    let app = build_app(state);

    // Large-ish JPEG that exceeds Axum's default 2MB body limit
    let jpeg_bytes = {
        let img = image::RgbImage::from_fn(4096, 1024, |x, y| {
            // Noisy pattern to resist JPEG compression
            image::Rgb([
                ((x * 7 + y * 13) % 256) as u8,
                ((x * 11 + y * 3) % 256) as u8,
                ((x * 5 + y * 17) % 256) as u8,
            ])
        });
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        buf.into_inner()
    };
    assert!(jpeg_bytes.len() > 2_000_000, "test image should exceed default 2MB limit: {} bytes", jpeg_bytes.len());

    let (content_type, body) = build_multipart_body(
        "Pasted Chocolate Cake",
        "# Chocolate Cake\n\nMix everything. Bake.",
        &jpeg_bytes,
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/recipe/save")
                .header("Authorization", "Bearer test-secret-token")
                .header("Content-Type", &content_type)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect to the new recipe
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let location = resp.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("/recipe/"), "redirect to: {location}");

    // Verify recipe exists
    let recipe_id: i64 = location.trim_start_matches("/recipe/").parse().unwrap();
    let recipe = Recipe::get_full_recipe(&db, recipe_id).unwrap();
    assert!(recipe.is_some());

    // Verify image exists
    let full = recipe.unwrap();
    assert!(!full.images.is_empty(), "recipe should have an image");
}

#[tokio::test]
async fn edit_recipe_page_requires_auth() {
    let state = test_app_state().await;
    let id = Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let app = build_app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(&format!("/recipe/{}/edit", id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn edit_recipe_page_ok() {
    let state = test_app_state().await;
    let id = Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let app = build_app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(&format!("/recipe/{}/edit", id))
                .header("Authorization", "Bearer test-secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Edit Recipe"));
    assert!(html.contains("Test Chocolate Cake"));
    assert!(html.contains("Mix flour, sugar, cocoa"));
}

#[tokio::test]
async fn update_recipe_saves_new_revision() {
    let state = test_app_state().await;
    let db = state.db.clone();
    let id = Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let app = build_app(state);

    let (content_type, body) = build_multipart_body_no_image(
        "Updated Chocolate Cake",
        "# Updated Recipe\n\nNew instructions here.",
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/recipe/{}/edit", id))
                .header("Authorization", "Bearer test-secret-token")
                .header("Content-Type", &content_type)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let location = resp.headers().get("location").unwrap().to_str().unwrap();
    assert_eq!(location, format!("/recipe/{}", id));

    let full = Recipe::get_full_recipe(&db, id).unwrap().unwrap();
    assert_eq!(full.recipe.name, "Updated Chocolate Cake");
    assert_eq!(full.revisions.len(), 2);
    assert!(full.best_revision.unwrap().content_text.contains("New instructions"));
}

#[tokio::test]
async fn update_recipe_without_image_keeps_existing() {
    let state = test_app_state().await;
    let db = state.db.clone();

    // Create recipe with image via the full save flow
    let upload = common::sample_recipe_upload();
    let id = Recipe::push(&db, upload).await.unwrap();
    // Add an image directly
    gk_server::models::Image::push(
        &db,
        id,
        gk::basic_models::ImageForUpload {
            category: "user-upload".into(),
            content_bytes: {
                let img = image::RgbImage::from_fn(2, 2, |_, _| image::Rgb([255, 0, 0]));
                let webp = webp::Encoder::from_image(&image::DynamicImage::ImageRgb8(img))
                    .unwrap().encode(75.0).to_vec();
                webp
            },
            prompt: None,
        },
    ).await.unwrap();

    let app = build_app(AppState {
        db: db.clone(),
        doc_index: DocumentIndexHandle::empty(db.clone()),
        auth: common::test_auth().await,
        tag_axes: Default::default(),
    });

    let (content_type, body) = build_multipart_body_no_image(
        "Same Name",
        "Same content",
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/recipe/{}/edit", id))
                .header("Authorization", "Bearer test-secret-token")
                .header("Content-Type", &content_type)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let full = Recipe::get_full_recipe(&db, id).unwrap().unwrap();
    assert!(!full.images.is_empty(), "existing image should be preserved");
}

#[tokio::test]
async fn get_all_tags_api() {
    let state = test_app_state().await;
    Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let app = build_app(state);
    let resp = app
        .oneshot(Request::builder().uri("/api/tags").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let tags: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert!(tags.len() >= 2);
}

#[tokio::test]
async fn patch_recipe_name() {
    let state = test_app_state().await;
    let id = Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let db = state.db.clone();
    let app = build_app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(&format!("/api/recipe/{}", id))
                .header("Authorization", "Bearer test-secret-token")
                .header("Content-Type", "application/json")
                .body(Body::from(format!(r#"{{"name":"Patched Name"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let recipe = Recipe::get_by_id(&db, id).unwrap().unwrap();
    assert_eq!(recipe.name, "Patched Name");
}

#[tokio::test]
async fn patch_recipe_content() {
    let state = test_app_state().await;
    let id = Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let db = state.db.clone();
    let app = build_app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(&format!("/api/recipe/{}", id))
                .header("Authorization", "Bearer test-secret-token")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"content":"New content here"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let full = Recipe::get_full_recipe(&db, id).unwrap().unwrap();
    assert_eq!(full.revisions.len(), 2);
}

#[tokio::test]
async fn patch_recipe_tags_set() {
    let state = test_app_state().await;
    let id = Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let db = state.db.clone();
    let app = build_app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(&format!("/api/recipe/{}", id))
                .header("Authorization", "Bearer test-secret-token")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"tags":["vegan","quick"]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let full = Recipe::get_full_recipe(&db, id).unwrap().unwrap();
    let names: Vec<&str> = full.tags.iter().map(|t| t.tag.as_str()).collect();
    assert!(names.contains(&"vegan"));
    assert!(!names.contains(&"dessert"));
}

#[tokio::test]
async fn get_all_basics_api() {
    let state = test_app_state().await;
    Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let app = build_app(state);
    let resp = app
        .oneshot(Request::builder().uri("/api/recipes/basic").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let recipes: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(recipes.len(), 1);
}

#[tokio::test]
async fn get_recipes_missing_images_api() {
    let state = test_app_state().await;
    Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let app = build_app(state);
    let resp = app
        .oneshot(Request::builder().uri("/api/recipes/missing-images").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let recipes: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(recipes.len(), 1); // sample has 0 images
}

#[tokio::test]
async fn get_all_recipes_text_api() {
    let state = test_app_state().await;
    Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let app = build_app(state);
    let resp = app
        .oneshot(Request::builder().uri("/api/recipes/text").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let recipes: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(recipes.len(), 1);
}

#[tokio::test]
async fn upsert_schedule_api() {
    let state = test_app_state().await;
    let app = build_app(state);
    let body = serde_json::to_string(&vec![FrontPageSection {
        date: "03-15".into(),
        section: "featured".into(),
        title: "Test".into(),
        blurb: None,
        query_tags: "dessert".into(),
    }]).unwrap();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/schedule")
                .header("Authorization", "Bearer test-secret-token")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn patch_recipe_description() {
    let state = test_app_state().await;
    let id = Recipe::push(&state.db, common::sample_recipe_upload()).await.unwrap();
    let db = state.db.clone();
    let app = build_app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(&format!("/api/recipe/{}", id))
                .header("Authorization", "Bearer test-secret-token")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"description":"A rich chocolate cake"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let recipe = Recipe::get_by_id(&db, id).unwrap().unwrap();
    assert_eq!(recipe.description.as_deref(), Some("A rich chocolate cake"));
}

#[tokio::test]
async fn create_and_view_article() {
    let state = test_app_state().await;
    let db = state.db.clone();

    // Create author
    Author::upsert(&db, &Author {
        author_id: "edgar".into(),
        display_name: "Edgar".into(),
        bio: "A pastry chef.".into(),
        bio_rendered: String::new(),
    }).unwrap();

    // Create article with today's date (published)
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let article_id = Article::push(
        &db, "edgar", "Test Article", "test-article",
        Some("A test summary"), "# Hello\n\nThis is a test.", &today, None,
    ).unwrap();
    assert!(article_id > 0);

    // View via route
    let app = build_app(state);
    let resp = app
        .oneshot(Request::builder().uri("/article/test-article").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("Test Article"));
    assert!(html.contains("Edgar"));
}

#[tokio::test]
async fn future_article_not_visible() {
    let state = test_app_state().await;
    let db = state.db.clone();

    Author::upsert(&db, &Author {
        author_id: "don".into(),
        display_name: "Don".into(),
        bio: "BBQ guy.".into(),
        bio_rendered: String::new(),
    }).unwrap();

    Article::push(
        &db, "don", "Future Article", "future-article",
        None, "Coming soon.", "2099-12-31", None,
    ).unwrap();

    let app = build_app(state);
    let resp = app
        .oneshot(Request::builder().uri("/article/future-article").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn articles_archive_page() {
    let state = test_app_state().await;
    let app = build_app(state);
    let resp = app
        .oneshot(Request::builder().uri("/articles").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_article_api_requires_auth() {
    let app = build_app(test_app_state().await);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/article")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_article_api_with_auth() {
    let state = test_app_state().await;
    let db = state.db.clone();

    Author::upsert(&db, &Author {
        author_id: "fran".into(),
        display_name: "Fran".into(),
        bio: "Budget cooking.".into(),
        bio_rendered: String::new(),
    }).unwrap();

    // Create a recipe to link to
    let recipe_id = Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();

    let app = build_app(state);
    let body = serde_json::json!({
        "author_id": "fran",
        "title": "Budget Meals",
        "slug": "budget-meals",
        "summary": "Eat well for less.",
        "content_text": format!("# Budget Meals\n\nTry the [Chocolate Cake](/recipe/{recipe_id}) for cheap."),
        "publish_date": "2026-03-01"
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/article")
                .header("Authorization", "Bearer test-secret-token")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp_body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert!(json["article_id"].as_i64().unwrap() > 0);

    // Verify recipe link
    let article_id = json["article_id"].as_i64().unwrap();
    let linked = Article::get_linked_recipe_ids(&db, article_id).unwrap();
    assert_eq!(linked, vec![recipe_id]);
}

#[tokio::test]
async fn upsert_author_api() {
    let app = build_app(test_app_state().await);
    let body = serde_json::json!({
        "author_id": "kevin",
        "display_name": "Kevin",
        "bio": "No time, two kids."
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/author")
                .header("Authorization", "Bearer test-secret-token")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn front_page_includes_articles() {
    let state = test_app_state().await;
    let db = state.db.clone();

    Author::upsert(&db, &Author {
        author_id: "eloise".into(),
        display_name: "Eloise".into(),
        bio: "Butter lover.".into(),
        bio_rendered: String::new(),
    }).unwrap();

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    Article::push(
        &db, "eloise", "Butter Everything", "butter-everything",
        Some("More butter."), "Use butter.", &today, None,
    ).unwrap();

    let app = build_app(state);
    let resp = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains("From Our Writers"));
    assert!(html.contains("Butter Everything"));
}

// browse_by_tag requires a live embedding model (search_tags panics on empty index),
// so it's skipped — same category as search/mod.rs.

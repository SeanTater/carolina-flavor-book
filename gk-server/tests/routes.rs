#[allow(dead_code)]
mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use gk_server::{
    build_app, AppState,
    models::Recipe,
    search::DocumentIndexHandle,
};
use tower::ServiceExt;

async fn test_app_state() -> AppState {
    let db = common::test_db().await;
    let doc_index = DocumentIndexHandle::empty(db.clone());
    let auth = common::test_auth().await;
    AppState { db, doc_index, auth }
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

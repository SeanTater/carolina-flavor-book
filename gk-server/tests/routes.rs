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

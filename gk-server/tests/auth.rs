use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Json, Router,
};
use gk_server::{
    auth::{self, session::AuthenticatedUser, AuthService},
    config::{AuthConfig, UserCredential},
};
use tower::ServiceExt;

/// Build a minimal router with just auth routes for testing.
fn test_app(auth: AuthService) -> Router {
    Router::new()
        .route(
            "/auth/login",
            get(auth::route::login_page).post(auth::route::login_submit),
        )
        .route("/auth/logout", get(auth::route::logout))
        .route("/api/auth/check", get(auth_check))
        .with_state(auth)
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

/// Create test AuthService with a known user and secret.
async fn test_auth_service() -> AuthService {
    let password = "testpassword123";
    let hash = bcrypt::hash(password, 4).unwrap(); // cost=4 for fast tests
    let config = AuthConfig {
        service_principal_secret: "test-secret-token".into(),
        session_storage_path: "/tmp/gk-test-sessions.json".into(),
        users: vec![UserCredential {
            username: "testuser".into(),
            password_hash: hash,
        }],
    };
    AuthService::new_from_config(&config).await.unwrap()
}

async fn body_string(body: Body) -> String {
    String::from_utf8(
        axum::body::to_bytes(body, usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

async fn body_json(body: Body) -> serde_json::Value {
    serde_json::from_slice(&axum::body::to_bytes(body, usize::MAX).await.unwrap()).unwrap()
}

#[tokio::test]
async fn login_page_renders() {
    let app = test_app(test_auth_service().await);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    assert!(body.contains("<form"), "login page should contain a form");
    assert!(
        body.contains("password"),
        "login page should have a password field"
    );
}

#[tokio::test]
async fn login_with_correct_password_redirects() {
    let app = test_app(test_auth_service().await);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=testpassword123"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("location").unwrap(), "/");
    assert!(
        resp.headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("session_id="),
        "should set session_id cookie"
    );
}

#[tokio::test]
async fn login_with_wrong_password_fails() {
    let app = test_app(test_auth_service().await);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=wrongpassword"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(resp.status(), StatusCode::SEE_OTHER);
    let body = body_string(resp.into_body()).await;
    assert!(body.contains("Login failed"));
}

#[tokio::test]
async fn login_with_unknown_user_fails() {
    let app = test_app(test_auth_service().await);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("username=nobody&password=testpassword123"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(resp.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn service_principal_auth_works() {
    let app = test_app(test_auth_service().await);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/check")
                .header("authorization", "Bearer test-secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["method"], "service_principal");
}

#[tokio::test]
async fn service_principal_wrong_token_rejected() {
    let app = test_app(test_auth_service().await);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/check")
                .header("authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn no_auth_rejected() {
    let app = test_app(test_auth_service().await);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/check")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn session_cookie_auth_works() {
    let auth = test_auth_service().await;
    let app = test_app(auth.clone());

    // First login to get a session cookie
    let login_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("username=testuser&password=testpassword123"))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = login_resp
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let cookie_val = cookie.split(';').next().unwrap();

    // Use that cookie to access a protected endpoint
    let app2 = test_app(auth);
    let resp = app2
        .oneshot(
            Request::builder()
                .uri("/api/auth/check")
                .header("cookie", cookie_val)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["method"], "session");
    assert_eq!(body["username"], "testuser");
}

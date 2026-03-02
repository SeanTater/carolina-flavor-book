use gk::basic_models::{RecipeForUpload, RevisionForUpload};
use gk_server::{
    auth::AuthService,
    config::{AuthConfig, UserCredential},
    database::Database,
};

/// Create an in-memory test database with migrations applied.
pub async fn test_db() -> Database {
    Database::connect_memory().await.unwrap()
}

/// Create a test AuthService with a known user and secret.
pub async fn test_auth() -> AuthService {
    let hash = bcrypt::hash("testpassword123", 4).unwrap();
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

/// Create a sample recipe upload for testing.
pub fn sample_recipe_upload() -> RecipeForUpload {
    RecipeForUpload {
        name: "Test Chocolate Cake".into(),
        tags: vec!["dessert".into(), "chocolate".into()],
        revisions: vec![RevisionForUpload {
            source_name: "manual".into(),
            content_text: "Mix flour, sugar, cocoa. Bake at 350F for 30 minutes.".into(),
            format: "markdown".into(),
            details: None,
        }],
        images: vec![],
    }
}

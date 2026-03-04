use gk_content::{client::ContentClient, gaps, grid, ingest};
use gk_server::{auth::AuthService, config::{AuthConfig, UserCredential}, database::Database, search::DocumentIndexHandle, AppState, build_app};
const TEST_TOKEN: &str = "test-secret-token";

async fn start_test_server() -> String {
    let db = Database::connect_memory().await.unwrap();
    let doc_index = DocumentIndexHandle::empty(db.clone());
    let hash = bcrypt::hash("testpassword123", 4).unwrap();
    let auth = AuthService::new_from_config(&AuthConfig {
        service_principal_secret: TEST_TOKEN.into(),
        session_storage_path: "/tmp/gk-content-test-sessions.json".into(),
        users: vec![UserCredential { username: "testuser".into(), password_hash: hash }],
    }).await.unwrap();

    let app = build_app(AppState { db, doc_index, auth });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("http://{addr}")
}

#[tokio::test]
async fn ingest_and_gaps_round_trip() {
    let server = start_test_server().await;
    let client = ContentClient::new(&server, TEST_TOKEN);
    let grid = grid::RecipeGrid::load("../config/recipe-grid.toml").unwrap();

    let recipes = vec![ingest::RecipeIngest {
        name: "Test Mapo Tofu".into(),
        content: "A spicy tofu dish".into(),
        tags: vec!["sichuan".into(), "vegetarian".into(), "tofu".into()],
        image_prompt: None,
    }];
    let report = ingest::ingest_recipes(&client, &recipes, false, &[]).await.unwrap();
    assert_eq!(report.created, 1);

    let all_tags = client.get_all_tags().await.unwrap();
    let basics = client.get_all_basics().await.unwrap();
    let gap_report = gaps::analyze(&all_tags, basics.len() as u64, &grid, None, &[]);
    assert_eq!(gap_report.total_recipes, 1);

    let cuisine = &gap_report.axes["cuisine"];
    let sichuan = cuisine.tags.iter().find(|t| t.tag == "sichuan").unwrap();
    assert_eq!(sichuan.count, 1);

    let attr = &gap_report.axes["attribute"];
    let veg = attr.tags.iter().find(|t| t.tag == "vegetarian").unwrap();
    assert_eq!(veg.count, 1);

    let filtered = gaps::analyze(&all_tags, basics.len() as u64, &grid, Some("sichuan"), &[]);
    assert_eq!(filtered.total_recipes, 1);
}

#[tokio::test]
async fn ingest_tags_round_trip() {
    let server = start_test_server().await;
    let client = ContentClient::new(&server, TEST_TOKEN);

    // Create a recipe first
    let recipe_id = client.push_recipe("Plain Recipe", "Some recipe", &[]).await.unwrap();

    let mut tags_map = std::collections::BTreeMap::new();
    tags_map.insert(recipe_id, vec!["korean".into(), "healthy".into(), "breakfast".into()]);
    let report = ingest::ingest_tags(&client, &tags_map).await.unwrap();
    assert_eq!(report.recipes, 1);
    assert_eq!(report.added, 3);

    let grid = grid::RecipeGrid::load("../config/recipe-grid.toml").unwrap();
    let all_tags = client.get_all_tags().await.unwrap();
    let basics = client.get_all_basics().await.unwrap();
    let gap_report = gaps::analyze(&all_tags, basics.len() as u64, &grid, Some("korean"), &[]);
    assert_eq!(gap_report.total_recipes, 1);
}

#[tokio::test]
async fn patch_recipe_rename_and_tags() {
    let server = start_test_server().await;
    let client = ContentClient::new(&server, TEST_TOKEN);

    let recipe_id = client.push_recipe("Old Name", "Some content", &["italian".into()]).await.unwrap();

    // Rename
    client.rename_recipe(recipe_id, "New Name (Italian pasta bake)").await.unwrap();
    let basics = client.get_all_basics().await.unwrap();
    let recipe = basics.iter().find(|r| r.recipe_id == recipe_id).unwrap();
    assert_eq!(recipe.name, "New Name (Italian pasta bake)");

    // Patch tags with set
    client.patch_recipe(recipe_id, &serde_json::json!({"tags": ["french", "baked"]})).await.unwrap();
    let all_tags = client.get_all_tags().await.unwrap();
    let recipe_tags: Vec<_> = all_tags.iter().filter(|t| t.recipe_id == recipe_id).map(|t| t.tag.as_str()).collect();
    assert!(recipe_tags.contains(&"french"));
    assert!(recipe_tags.contains(&"baked"));
    assert!(!recipe_tags.contains(&"italian")); // replaced

    // Patch tags with add/remove ops
    client.patch_recipe(recipe_id, &serde_json::json!({"tags": {"add": ["grilled"], "remove": ["baked"]}})).await.unwrap();
    let all_tags = client.get_all_tags().await.unwrap();
    let recipe_tags: Vec<_> = all_tags.iter().filter(|t| t.recipe_id == recipe_id).map(|t| t.tag.as_str()).collect();
    assert!(recipe_tags.contains(&"french"));
    assert!(recipe_tags.contains(&"grilled"));
    assert!(!recipe_tags.contains(&"baked"));
}

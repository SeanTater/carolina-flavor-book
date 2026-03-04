#[allow(dead_code)]
mod common;

use gk_server::models::{Embedding, FrontPageSection, Recipe, Revision, Tag};
use half::f16;

#[tokio::test]
async fn migrate_creates_tables() {
    let db = common::test_db().await;
    let recipes = Recipe::get_all_basics(&db).unwrap();
    assert!(recipes.is_empty());
}

#[tokio::test]
async fn recipe_push_and_get_basics() {
    let db = common::test_db().await;
    let upload = common::sample_recipe_upload();
    Recipe::push(&db, upload).await.unwrap();
    let recipes = Recipe::get_all_basics(&db).unwrap();
    assert_eq!(recipes.len(), 1);
    assert_eq!(recipes[0].name, "Test Chocolate Cake");
}

#[tokio::test]
async fn recipe_get_by_id() {
    let db = common::test_db().await;
    let id = Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    assert!(Recipe::get_by_id(&db, id).unwrap().is_some());
    assert!(Recipe::get_by_id(&db, 9999).unwrap().is_none());
}

#[tokio::test]
async fn recipe_get_full() {
    let db = common::test_db().await;
    let id = Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    let full = Recipe::get_full_recipe(&db, id).unwrap().unwrap();
    assert_eq!(full.tags.len(), 2);
    assert_eq!(full.revisions.len(), 1);
    assert!(full.best_revision.is_some());
    assert_eq!(full.best_revision.unwrap().source_name, "manual");
}

#[tokio::test]
async fn recipe_get_by_tag() {
    let db = common::test_db().await;
    Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    let found = Recipe::get_by_tag(&db, "dessert").unwrap();
    assert_eq!(found.len(), 1);
    let not_found = Recipe::get_by_tag(&db, "nonexistent").unwrap();
    assert!(not_found.is_empty());
}

#[tokio::test]
async fn recipe_get_extended() {
    let db = common::test_db().await;
    let id = Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    let extended = Recipe::get_extended(&db, &[id]).unwrap();
    assert_eq!(extended.len(), 1);
    assert_eq!(extended[0].name, "Test Chocolate Cake");
}

#[tokio::test]
async fn tag_get_distinct() {
    let db = common::test_db().await;
    Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    let tags = Tag::get_distinct(&db).unwrap();
    let tag_names: Vec<&str> = tags.iter().map(|t| t.tag.as_str()).collect();
    assert!(tag_names.contains(&"dessert"));
    assert!(tag_names.contains(&"chocolate"));
}

#[tokio::test]
async fn revision_without_embeddings() {
    let db = common::test_db().await;
    let id = Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    let unindexed = Revision::get_revisions_without_embeddings(&db, "test-model", 10).unwrap();
    assert_eq!(unindexed.len(), 1);

    // Push an embedding for this revision
    let revision = &unindexed[0];
    let embedding = Embedding {
        embedding_id: 1,
        recipe_id: id,
        revision_id: revision.revision_id,
        span_start: 0,
        span_end: 10,
        created_on: "2024-01-01 00:00:00".into(),
        model_name: "test-model".into(),
        embedding: vec![f16::from_f32(0.0); 384],
    };
    Embedding::push(&db, &[embedding]).unwrap();

    let unindexed = Revision::get_revisions_without_embeddings(&db, "test-model", 10).unwrap();
    assert!(unindexed.is_empty());
}

#[tokio::test]
async fn embedding_push_count_list() {
    let db = common::test_db().await;
    let id = Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    let _recipe = Recipe::get_by_id(&db, id).unwrap().unwrap();
    let full = Recipe::get_full_recipe(&db, id).unwrap().unwrap();
    let rev = &full.revisions[0];

    assert_eq!(Embedding::count_embeddings(&db).unwrap(), 0);

    let embedding = Embedding {
        embedding_id: 1,
        recipe_id: id,
        revision_id: rev.revision_id,
        span_start: 0,
        span_end: 10,
        created_on: "2024-01-01 00:00:00".into(),
        model_name: "test-model".into(),
        embedding: vec![f16::from_f32(0.5); 384],
    };
    Embedding::push(&db, &[embedding]).unwrap();

    assert_eq!(Embedding::count_embeddings(&db).unwrap(), 1);
    let all = Embedding::list_all(&db, "test-model").unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].embedding.len(), 384);
}

#[tokio::test]
async fn recipe_without_enough_images() {
    let db = common::test_db().await;
    Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    // Our sample recipe has 0 images, so it should be found
    let result = Recipe::get_any_recipe_without_enough_images(&db, "ai-photo").unwrap();
    assert!(result.is_some());
}

#[tokio::test]
async fn tag_set_for_recipe() {
    let db = common::test_db().await;
    let id = Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    // Replace tags
    Tag::set_for_recipe(&db, id, &["vegan".into(), "quick".into()]).unwrap();
    let recipe = Recipe::get_full_recipe(&db, id).unwrap().unwrap();
    let tag_names: Vec<&str> = recipe.tags.iter().map(|t| t.tag.as_str()).collect();
    assert!(tag_names.contains(&"vegan"));
    assert!(tag_names.contains(&"quick"));
    assert!(!tag_names.contains(&"dessert")); // original tag gone
}

#[tokio::test]
async fn tag_remove() {
    let db = common::test_db().await;
    let id = Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    Tag::remove(&db, id, &["dessert".into()]).unwrap();
    let recipe = Recipe::get_full_recipe(&db, id).unwrap().unwrap();
    let tag_names: Vec<&str> = recipe.tags.iter().map(|t| t.tag.as_str()).collect();
    assert!(!tag_names.contains(&"dessert"));
    assert!(tag_names.contains(&"chocolate")); // other tag remains
}

#[tokio::test]
async fn front_page_section_upsert_and_get() {
    let db = common::test_db().await;
    let section = FrontPageSection {
        date: "03-15".into(),
        section: "featured".into(),
        title: "Spring Favorites".into(),
        blurb: Some("Fresh seasonal picks".into()),
        query_tags: "spring,salad".into(),
    };
    FrontPageSection::upsert(&db, &section).unwrap();

    let sections = FrontPageSection::get_for_date(&db, "03-15").unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].title, "Spring Favorites");

    // Upsert replaces
    let updated = FrontPageSection {
        title: "Updated Title".into(),
        ..section
    };
    FrontPageSection::upsert(&db, &updated).unwrap();
    let sections = FrontPageSection::get_for_date(&db, "03-15").unwrap();
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].title, "Updated Title");
}

#[tokio::test]
async fn front_page_get_for_date_empty() {
    let db = common::test_db().await;
    let sections = FrontPageSection::get_for_date(&db, "12-25").unwrap();
    assert!(sections.is_empty());
}

#[tokio::test]
async fn front_page_get_recipe_ids_for_tags() {
    let db = common::test_db().await;
    Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    let ids = FrontPageSection::get_recipe_ids_for_tags(&db, &["dessert".into()], 10).unwrap();
    assert_eq!(ids.len(), 1);

    // Empty tags returns empty
    let ids = FrontPageSection::get_recipe_ids_for_tags(&db, &[], 10).unwrap();
    assert!(ids.is_empty());
}

#[tokio::test]
async fn recipe_update_name() {
    let db = common::test_db().await;
    let id = Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    Recipe::update_name(&db, id, "Renamed Cake").unwrap();
    let recipe = Recipe::get_by_id(&db, id).unwrap().unwrap();
    assert_eq!(recipe.name, "Renamed Cake");
}

#[tokio::test]
async fn recipe_get_all_with_text() {
    let db = common::test_db().await;
    Recipe::push(&db, common::sample_recipe_upload()).await.unwrap();
    let results = Recipe::get_all_with_text(&db).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].content_text.contains("Mix flour"));
}

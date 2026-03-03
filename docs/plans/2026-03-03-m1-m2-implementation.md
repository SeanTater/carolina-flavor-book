# M1 + M2: Recipe Grid & Content CLI Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build `gk-content` CLI with `gaps`, `ingest`, and `ingest-tags` subcommands, plus the TOML recipe grid config.

**Architecture:** New workspace crate `gk-content` with clap subcommands. Depends on `gk-server` for `Database`, `models::Recipe`, `models::Tag`, and `models::Revision`. Reads `config/recipe-grid.toml` at runtime. No new DB migrations needed — uses existing `Tag` table.

**Tech Stack:** Rust, clap (derive), toml, serde, serde_json. Existing workspace deps for rusqlite/r2d2 via gk-server.

---

### Task 1: Create the recipe grid TOML config

**Files:**
- Create: `config/recipe-grid.toml`

**Step 1: Write the config file**

```toml
# Tag vocabulary for recipe categorization.
# Each axis defines a set of valid tags. Recipes can have tags from any/all axes.

[axes.cuisine]
display = "Cuisine"
tags = [
    "brazilian", "serbian", "moroccan", "korean", "cuban", "swedish",
    "sichuan", "cantonese", "dongbei", "hunan", "fujian", "yunnan",
    "japanese", "thai", "indian-north", "indian-south", "ethiopian",
    "peruvian", "lebanese", "georgian", "american-south", "mexican",
    "italian", "french",
]

[axes.attribute]
display = "Attribute"
tags = [
    "vegetarian", "gluten-free", "indulgent", "authentic",
    "quick-and-easy", "low-cholesterol", "low-sodium",
    "comfort-food", "healthy",
]

[axes.method]
display = "Cooking Method"
tags = [
    "grilled", "slow-cooker", "braised", "raw", "no-cook",
    "fermented", "smoked", "deep-fried", "steamed", "stir-fried",
    "baked", "one-pot",
]

[axes.occasion]
display = "Meal Occasion"
tags = [
    "breakfast", "lunch", "dinner", "snack", "dinner-party",
    "potluck", "packed-lunch", "late-night",
]

[axes.ingredient]
display = "Ingredient Spotlight"
tags = [
    "tofu", "lentils", "seafood", "offal", "root-vegetables",
    "stone-fruit", "fresh-herbs", "rice", "noodles", "beans",
]

[axes.effort]
display = "Effort"
tags = ["5-ingredient", "one-pot", "weekend-project", "multi-day"]

[axes.era]
display = "Era / Tradition"
tags = ["historical", "heirloom", "modern-fusion"]

[axes.temperature]
display = "Temperature"
tags = ["cold-dish", "frozen-dessert", "hot-soup", "room-temp"]
```

**Step 2: Commit**

```bash
git add config/recipe-grid.toml
git commit -m "Add recipe grid TOML with tag vocabulary across 8 axes"
```

---

### Task 2: Scaffold gk-content crate

**Files:**
- Create: `gk-content/Cargo.toml`
- Create: `gk-content/src/main.rs`
- Modify: `Cargo.toml` (workspace root, line 2 — add to members)

**Step 1: Create `gk-content/Cargo.toml`**

```toml
[package]
name = "gk-content"
version.workspace = true
edition.workspace = true

[dependencies]
anyhow = { workspace = true }
clap = { workspace = true, features = ["derive"] }
gk = { path = "../gk" }
gk-server = { path = "../gk-server" }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
tokio = { workspace = true, features = ["full"] }
toml = "1.0.3"
```

**Step 2: Create minimal `gk-content/src/main.rs`**

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gk-content", about = "Recipe content pipeline tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show gaps in recipe tag coverage
    Gaps,
}

fn main() {
    let _cli = Cli::parse();
    println!("gk-content placeholder");
}
```

**Step 3: Add to workspace members**

In root `Cargo.toml`, change line 2:
```toml
members = ["gk", "gk-client", "gk-server", "gk-content"]
```

**Step 4: Verify it compiles**

Run: `cargo build -p gk-content`
Expected: compiles successfully

**Step 5: Commit**

```bash
git add gk-content/ Cargo.toml Cargo.lock
git commit -m "Scaffold gk-content crate with clap CLI skeleton"
```

---

### Task 3: Parse the recipe grid TOML

**Files:**
- Create: `gk-content/src/grid.rs`
- Modify: `gk-content/src/main.rs`

**Step 1: Write failing test**

In `gk-content/src/grid.rs`:

```rust
use anyhow::Result;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct RecipeGrid {
    pub axes: BTreeMap<String, Axis>,
}

#[derive(Debug, Deserialize)]
pub struct Axis {
    pub display: String,
    pub tags: Vec<String>,
}

impl RecipeGrid {
    pub fn load(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let grid: RecipeGrid = toml::from_str(&content)?;
        Ok(grid)
    }

    /// Return a flat set of all valid tags across all axes.
    pub fn all_tags(&self) -> Vec<&str> {
        self.axes.values()
            .flat_map(|a| a.tags.iter().map(|s| s.as_str()))
            .collect()
    }

    /// Find which axis a tag belongs to, if any.
    pub fn axis_for_tag(&self, tag: &str) -> Option<&str> {
        for (axis_name, axis) in &self.axes {
            if axis.tags.iter().any(|t| t == tag) {
                return Some(axis_name.as_str());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_recipe_grid() {
        let grid = RecipeGrid::load("../config/recipe-grid.toml").unwrap();
        assert!(grid.axes.contains_key("cuisine"));
        assert!(grid.axes.contains_key("attribute"));
        assert!(grid.axes["cuisine"].tags.contains(&"sichuan".to_string()));
    }

    #[test]
    fn axis_for_tag_lookup() {
        let grid = RecipeGrid::load("../config/recipe-grid.toml").unwrap();
        assert_eq!(grid.axis_for_tag("sichuan"), Some("cuisine"));
        assert_eq!(grid.axis_for_tag("vegetarian"), Some("attribute"));
        assert_eq!(grid.axis_for_tag("nonexistent"), None);
    }
}
```

**Step 2: Wire up the module**

In `gk-content/src/main.rs`, add at the top:
```rust
mod grid;
```

**Step 3: Run tests**

Run: `cargo test -p gk-content`
Expected: 2 tests pass

**Step 4: Commit**

```bash
git add gk-content/src/grid.rs gk-content/src/main.rs
git commit -m "Add recipe grid TOML parser with axis lookup"
```

---

### Task 4: Implement the `gaps` subcommand

**Files:**
- Create: `gk-content/src/gaps.rs`
- Modify: `gk-content/src/main.rs`

This is the core analysis tool. It queries the Tag table, groups by axis, and shows what's underrepresented.

**Step 1: Write the gaps module**

In `gk-content/src/gaps.rs`:

```rust
use anyhow::Result;
use gk_server::database::Database;
use rusqlite::params;
use std::collections::BTreeMap;

use crate::grid::RecipeGrid;

/// Tag count: how many recipes have this tag
#[derive(Debug, serde::Serialize)]
pub struct TagCount {
    pub tag: String,
    pub count: u64,
}

/// Per-axis gap report
#[derive(Debug, serde::Serialize)]
pub struct AxisReport {
    pub display: String,
    pub tags: Vec<TagCount>,
    pub total: u64,
}

/// Full gap report
#[derive(Debug, serde::Serialize)]
pub struct GapReport {
    pub total_recipes: u64,
    pub axes: BTreeMap<String, AxisReport>,
}

pub fn analyze(db: &Database, grid: &RecipeGrid, filter_cuisine: Option<&str>, ignore: &[String]) -> Result<GapReport> {
    let conn = db.pool.get()?;

    // Total recipe count (optionally filtered by cuisine)
    let total_recipes: u64 = if let Some(cuisine) = filter_cuisine {
        conn.query_row(
            "SELECT COUNT(DISTINCT recipe_id) FROM Tag WHERE tag = ?",
            params![cuisine],
            |row| row.get(0),
        )?
    } else {
        conn.query_row("SELECT COUNT(*) FROM Recipe", params![], |row| row.get(0))?
    };

    let mut axes = BTreeMap::new();

    for (axis_name, axis) in &grid.axes {
        if ignore.iter().any(|i| i == axis_name) {
            continue;
        }

        let mut tags = Vec::new();
        let mut axis_total = 0u64;

        for tag in &axis.tags {
            let count: u64 = if let Some(cuisine) = filter_cuisine {
                // Count recipes that have BOTH this tag AND the cuisine tag
                conn.query_row(
                    "SELECT COUNT(DISTINCT t1.recipe_id)
                     FROM Tag t1
                     JOIN Tag t2 ON t1.recipe_id = t2.recipe_id
                     WHERE t1.tag = ? AND t2.tag = ?",
                    params![tag, cuisine],
                    |row| row.get(0),
                )?
            } else {
                conn.query_row(
                    "SELECT COUNT(*) FROM Tag WHERE tag = ?",
                    params![tag],
                    |row| row.get(0),
                )?
            };
            axis_total += count;
            tags.push(TagCount { tag: tag.clone(), count });
        }

        tags.sort_by(|a, b| b.count.cmp(&a.count));

        axes.insert(axis_name.clone(), AxisReport {
            display: axis.display.clone(),
            tags,
            total: axis_total,
        });
    }

    Ok(GapReport { total_recipes, axes })
}

/// Format the gap report as a human-readable string.
pub fn format_text(report: &GapReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("Total recipes: {}\n\n", report.total_recipes));

    for (axis_name, axis) in &report.axes {
        out.push_str(&format!("{}  ({})\n", axis.display, axis_name));

        // Show tags in columns: tag:count
        let items: Vec<String> = axis.tags.iter()
            .map(|t| format!("  {}:{}", t.tag, t.count))
            .collect();
        for item in &items {
            out.push_str(item);
            out.push('\n');
        }

        // Highlight gaps (tags with 0 recipes)
        let gaps: Vec<&str> = axis.tags.iter()
            .filter(|t| t.count == 0)
            .map(|t| t.tag.as_str())
            .collect();
        if !gaps.is_empty() {
            out.push_str(&format!("  GAPS: {}\n", gaps.join(", ")));
        }
        out.push('\n');
    }
    out
}
```

**Step 2: Wire up the subcommand in `main.rs`**

Replace `gk-content/src/main.rs` entirely:

```rust
mod gaps;
mod grid;

use anyhow::Result;
use clap::{Parser, Subcommand};
use gk_server::database::Database;
use gk_server::config::DatabaseConfig;

#[derive(Parser)]
#[command(name = "gk-content", about = "Recipe content pipeline tools")]
struct Cli {
    /// Path to the database
    #[arg(long, default_value = "data/recipes.db")]
    db: String,

    /// Path to the recipe grid config
    #[arg(long, default_value = "config/recipe-grid.toml")]
    grid: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show gaps in recipe tag coverage
    Gaps {
        /// Filter to recipes with this cuisine tag
        #[arg(long)]
        cuisine: Option<String>,

        /// Axes to ignore in the report
        #[arg(long, value_delimiter = ',')]
        ignore: Vec<String>,

        /// Output as JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db = Database::connect(&DatabaseConfig { path: cli.db }).await?;
    let grid = grid::RecipeGrid::load(&cli.grid)?;

    match cli.command {
        Commands::Gaps { cuisine, ignore, json } => {
            let report = gaps::analyze(&db, &grid, cuisine.as_deref(), &ignore)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", gaps::format_text(&report));
            }
        }
    }

    Ok(())
}
```

**Step 3: Add missing workspace dependency**

The `gk-content` crate needs `rusqlite` in its dependency list since `gaps.rs` uses `rusqlite::params`. Add to `gk-content/Cargo.toml` under `[dependencies]`:

```toml
rusqlite = { workspace = true, features = ["bundled"] }
```

**Step 4: Verify it compiles and runs**

Run: `cargo build -p gk-content`
Expected: compiles

Run: `cargo run -p gk-content -- --db data/recipes.db --grid config/recipe-grid.toml gaps`
Expected: prints gap report showing mostly 0-count tags, except a few like `chinese:1`, `asian:1`

Run: `cargo run -p gk-content -- gaps --json`
Expected: JSON output of the same data

**Step 5: Commit**

```bash
git add gk-content/src/
git commit -m "Implement gaps subcommand with human and JSON output"
```

---

### Task 5: Implement `ingest-tags` subcommand

**Files:**
- Modify: `gk-content/src/main.rs` (add subcommand variant)
- Create: `gk-content/src/ingest.rs`

This reads a JSON file mapping recipe IDs to tag arrays and applies them.

**Step 1: Write the ingest module**

In `gk-content/src/ingest.rs`:

```rust
use anyhow::Result;
use gk_server::database::Database;
use gk_server::models::Tag;
use std::collections::BTreeMap;

/// Apply tags from a JSON map of {recipe_id: [tags]}.
pub fn ingest_tags(db: &Database, tags_map: &BTreeMap<i64, Vec<String>>) -> Result<IngestTagsReport> {
    let mut added = 0u64;
    let mut skipped = 0u64;

    for (recipe_id, tags) in tags_map {
        for tag in tags {
            // Tag::push uses INSERT OR IGNORE, so duplicates are safe
            Tag::push(db, *recipe_id, tag)?;
            added += 1;
        }
    }

    Ok(IngestTagsReport { added, skipped, recipes: tags_map.len() as u64 })
}

#[derive(Debug, serde::Serialize)]
pub struct IngestTagsReport {
    pub added: u64,
    pub skipped: u64,
    pub recipes: u64,
}
```

**Step 2: Add subcommand to main.rs**

Add to the `Commands` enum:

```rust
    /// Apply tags to existing recipes from a JSON file
    IngestTags {
        /// Path to JSON file: {"recipe_id": ["tag1", "tag2"], ...}
        file: String,
    },
```

Add to the match block:

```rust
        Commands::IngestTags { file } => {
            let content = std::fs::read_to_string(&file)?;
            let tags_map: std::collections::BTreeMap<i64, Vec<String>> = serde_json::from_str(&content)?;
            let report = ingest::ingest_tags(&db, &tags_map)?;
            println!("Tagged {} recipes, {} tags added", report.recipes, report.added);
        }
```

Add at the top of main.rs:
```rust
mod ingest;
```

**Step 3: Verify it compiles**

Run: `cargo build -p gk-content`
Expected: compiles

**Step 4: Commit**

```bash
git add gk-content/src/
git commit -m "Add ingest-tags subcommand for batch tagging from JSON"
```

---

### Task 6: Implement `ingest` subcommand for new recipes

**Files:**
- Modify: `gk-content/src/ingest.rs`
- Modify: `gk-content/src/main.rs`

This reads a JSON array of recipe objects and pushes them to the DB.

**Step 1: Define the ingest recipe format**

Add to `gk-content/src/ingest.rs`:

```rust
use gk::basic_models::{RecipeForUpload, RevisionForUpload};
use gk_server::models::Recipe;

#[derive(Debug, serde::Deserialize)]
pub struct RecipeIngest {
    pub name: String,
    pub content: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub image_prompt: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct IngestReport {
    pub created: u64,
    pub failed: u64,
    pub images_generated: u64,
}

pub async fn ingest_recipes(
    db: &Database,
    recipes: &[RecipeIngest],
    generate_images: bool,
) -> Result<IngestReport> {
    let mut created = 0u64;
    let mut failed = 0u64;
    let mut images_generated = 0u64;

    for recipe in recipes {
        let upload = RecipeForUpload {
            name: recipe.name.clone(),
            tags: recipe.tags.clone(),
            revisions: vec![RevisionForUpload {
                source_name: "generated".into(),
                content_text: recipe.content.clone(),
                format: "markdown".into(),
                details: None,
            }],
            images: vec![],
        };

        match Recipe::push(db, upload).await {
            Ok(recipe_id) => {
                created += 1;
                // Generate image if requested and prompt is provided
                if generate_images {
                    if let Some(prompt) = &recipe.image_prompt {
                        match generate_and_push_image(db, recipe_id, prompt).await {
                            Ok(()) => images_generated += 1,
                            Err(e) => eprintln!("Image generation failed for {}: {e}", recipe.name),
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to ingest {}: {e}", recipe.name);
                failed += 1;
            }
        }
    }

    Ok(IngestReport { created, failed, images_generated })
}

/// Call the external image-gen script and push the result.
async fn generate_and_push_image(db: &Database, recipe_id: i64, prompt: &str) -> Result<()> {
    use gk::basic_models::ImageForUpload;
    use gk_server::models::Image;

    let tmp = tempfile::NamedTempFile::new()?.into_temp_path();
    let output_path = format!("{}.webp", tmp.display());

    let status = tokio::process::Command::new("image-gen")
        .arg(prompt)
        .arg("-o")
        .arg(&output_path)
        .status()
        .await?;

    anyhow::ensure!(status.success(), "image-gen exited with {status}");

    let image_bytes = tokio::fs::read(&output_path).await?;
    Image::push(
        db,
        recipe_id,
        ImageForUpload {
            category: "ai-generated".into(),
            content_bytes: image_bytes,
            prompt: Some(prompt.to_string()),
        },
    )
    .await?;

    // Clean up
    let _ = tokio::fs::remove_file(&output_path).await;

    Ok(())
}
```

**Step 2: Add tempfile to gk-content/Cargo.toml dependencies**

```toml
tempfile = { workspace = true }
```

**Step 3: Add subcommand to main.rs**

Add to the `Commands` enum:

```rust
    /// Ingest new recipes from a JSON file
    Ingest {
        /// Path to JSON file: [{name, content, tags, image_prompt?}, ...]
        file: String,

        /// Generate images using image-gen for recipes with image_prompt
        #[arg(long)]
        images: bool,
    },
```

Add to the match block:

```rust
        Commands::Ingest { file, images } => {
            let content = std::fs::read_to_string(&file)?;
            let recipes: Vec<ingest::RecipeIngest> = serde_json::from_str(&content)?;
            println!("Ingesting {} recipes...", recipes.len());
            let report = ingest::ingest_recipes(&db, &recipes, images).await?;
            println!("Created: {}, Failed: {}, Images: {}",
                report.created, report.failed, report.images_generated);
        }
```

**Step 4: Verify it compiles**

Run: `cargo build -p gk-content`
Expected: compiles

**Step 5: Write a small test JSON and do a dry run**

Create a temporary test file `/tmp/test-recipes.json`:
```json
[
  {
    "name": "Test Sichuan Mapo Tofu",
    "content": "# Mapo Tofu\n\nA classic Sichuan dish.\n\n## Ingredients\n- Tofu\n- Doubanjiang\n\n## Instructions\n1. Fry the aromatics\n2. Add tofu",
    "tags": ["sichuan", "vegetarian", "authentic", "quick-and-easy", "tofu"]
  }
]
```

Run: `cargo run -p gk-content -- --db data/recipes.db ingest /tmp/test-recipes.json`
Expected: "Created: 1, Failed: 0, Images: 0"

Verify: `cargo run -p gk-content -- --db data/recipes.db gaps --cuisine sichuan`
Expected: Shows sichuan with associated tag counts

**Step 6: Commit**

```bash
git add gk-content/
git commit -m "Add ingest subcommand for batch recipe creation with optional image generation"
```

---

### Task 7: Integration test

**Files:**
- Create: `gk-content/tests/integration.rs`

**Step 1: Write integration test**

```rust
use std::io::Write;

/// Test the full pipeline: load grid, ingest recipes, check gaps.
/// We can't easily use the in-memory DB through the CLI binary,
/// so test the library functions directly.
#[tokio::test]
async fn ingest_and_gaps_round_trip() {
    // Set up in-memory DB
    let db = gk_server::database::Database::connect_memory().await.unwrap();

    // Load grid
    let grid = gk_content::grid::RecipeGrid::load("../config/recipe-grid.toml").unwrap();

    // Ingest a recipe
    let recipes = vec![gk_content::ingest::RecipeIngest {
        name: "Test Mapo Tofu".into(),
        content: "A spicy tofu dish".into(),
        tags: vec!["sichuan".into(), "vegetarian".into(), "tofu".into()],
        image_prompt: None,
    }];
    let report = gk_content::ingest::ingest_recipes(&db, &recipes, false).await.unwrap();
    assert_eq!(report.created, 1);

    // Check gaps
    let gap_report = gk_content::gaps::analyze(&db, &grid, None, &[]).unwrap();
    assert_eq!(gap_report.total_recipes, 1);

    // Sichuan should show count 1
    let cuisine = &gap_report.axes["cuisine"];
    let sichuan = cuisine.tags.iter().find(|t| t.tag == "sichuan").unwrap();
    assert_eq!(sichuan.count, 1);

    // Vegetarian should show count 1
    let attr = &gap_report.axes["attribute"];
    let veg = attr.tags.iter().find(|t| t.tag == "vegetarian").unwrap();
    assert_eq!(veg.count, 1);

    // Filter by cuisine
    let filtered = gk_content::gaps::analyze(&db, &grid, Some("sichuan"), &[]).unwrap();
    assert_eq!(filtered.total_recipes, 1);
}

#[tokio::test]
async fn ingest_tags_applies_to_existing_recipe() {
    let db = gk_server::database::Database::connect_memory().await.unwrap();

    // Create a recipe directly
    let id = gk_server::models::Recipe::push(&db, gk::basic_models::RecipeForUpload {
        name: "Plain Recipe".into(),
        tags: vec![],
        revisions: vec![gk::basic_models::RevisionForUpload {
            source_name: "manual".into(),
            content_text: "Some recipe".into(),
            format: "markdown".into(),
            details: None,
        }],
        images: vec![],
    }).await.unwrap();

    // Apply tags
    let mut tags_map = std::collections::BTreeMap::new();
    tags_map.insert(id, vec!["korean".into(), "healthy".into(), "breakfast".into()]);
    let report = gk_content::ingest::ingest_tags(&db, &tags_map).unwrap();
    assert_eq!(report.recipes, 1);
    assert_eq!(report.added, 3);

    // Verify tags stuck
    let grid = gk_content::grid::RecipeGrid::load("../config/recipe-grid.toml").unwrap();
    let gap_report = gk_content::gaps::analyze(&db, &grid, Some("korean"), &[]).unwrap();
    assert_eq!(gap_report.total_recipes, 1);
}
```

**Step 2: Make modules public for integration tests**

In `gk-content/src/main.rs`, ensure the module declarations are `pub`:

```rust
pub mod gaps;
pub mod grid;
pub mod ingest;
```

Also add a `[lib]` section to `gk-content/Cargo.toml` so the integration tests can import the library:

```toml
[lib]
name = "gk_content"
path = "src/lib.rs"

[[bin]]
name = "gk-content"
path = "src/main.rs"
```

Then create `gk-content/src/lib.rs`:

```rust
pub mod gaps;
pub mod grid;
pub mod ingest;
```

And change `main.rs` module declarations to:

```rust
use gk_content::{gaps, grid, ingest};
```

(Remove the `mod` declarations from main.rs since they live in lib.rs now.)

**Step 3: Run tests**

Run: `cargo test -p gk-content`
Expected: all tests pass (2 integration tests + 2 unit tests from grid.rs)

**Step 4: Commit**

```bash
git add gk-content/
git commit -m "Add integration tests for ingest and gaps round-trip"
```

---

### Task 8: Final cleanup and verify everything

**Step 1: Run full workspace tests**

Run: `cargo test --workspace`
Expected: all tests pass, no warnings

**Step 2: Test the CLI end-to-end against the real DB**

Run: `cargo run -p gk-content -- gaps`
Expected: shows all axes with current tag counts (mostly 0 for new vocabulary)

Run: `cargo run -p gk-content -- gaps --cuisine american-south`
Expected: shows 0 (no recipes tagged american-south yet — church cookbook recipes haven't been tagged)

Run: `cargo run -p gk-content -- gaps --json | head -20`
Expected: clean JSON output

**Step 3: Commit any final fixes, then tag**

```bash
git add -A
git commit -m "M1+M2 complete: gk-content CLI with gaps, ingest, and ingest-tags"
```

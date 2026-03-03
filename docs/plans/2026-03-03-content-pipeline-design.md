# Recipe Content Pipeline Design

## Problem

The recipe collection is 488 recipes, ~73% from a single US Southern church cookbook. Tags are course-type only (dessert, appetizer, etc.) with almost no cuisine, method, or dietary tagging. The front page shows 20 random cards with no editorial structure.

We want a front page with sections like "Featured Recipe," "Healthy Options," and "Food Spotlight: Northern China" — which requires both richer tagging of existing recipes and generating thousands of new recipes across diverse cuisines and categories.

## Decisions Made

- **LLM runtime**: Agent-driven (Claude Code / Codex exec) rather than direct API calls. Uses existing subscriptions, no API key management.
- **Deterministic tooling**: Rust CLI (`gk-content`) for gap analysis and ingestion. Agents handle creative generation.
- **Category structure**: TOML file defines the tag vocabulary across 8 axes. Recipes are tagged with multiple axes, not placed in grid cells.
- **Image generation**: Local InvokeAI via `image-gen` CLI. Agents call it as a script. No API costs.
- **Tag vocabulary**: Fixed — tagger assigns from predefined lists only.
- **Seasonal calendar**: 365 days, calendar-mapped to seasonal themes. Changes daily, deterministic by date.
- **Scale target**: ~10,000 new recipes. Deep per-cuisine coverage rather than sparse grid filling.

## Architecture

```
config/recipe-grid.toml        (tag vocabulary, 8 axes)
        |
  gk-content gaps              (Rust: queries DB, shows underrepresented tags)
        |
  agent reads gaps + generates  (Claude Code / Codex exec)
        |
  recipes.json                  (structured output from agent)
        |
  gk-content ingest             (Rust: pushes to DB via Recipe::push)
        |
  image-gen                     (local InvokeAI, optional per recipe)
```

## The Category Grid

Eight axes, each a flat list of tags. A recipe can have tags from any/all axes. Not every combination is meaningful — agents use judgment.

### Axes

| Axis | Example tags |
|------|-------------|
| **Cuisine** | brazilian, serbian, moroccan, korean, cuban, swedish, sichuan, cantonese, dongbei, hunan, fujian, yunnan, japanese, thai, indian-north, indian-south, ethiopian, peruvian, lebanese, georgian, american-south, mexican, italian, french |
| **Attribute** | vegetarian, gluten-free, indulgent, authentic, quick-and-easy, low-cholesterol, low-sodium, comfort-food, healthy |
| **Cooking Method** | grilled, slow-cooker, braised, raw, no-cook, fermented, smoked, deep-fried, steamed, stir-fried, baked, one-pot |
| **Meal Occasion** | breakfast, lunch, dinner, snack, dinner-party, potluck, packed-lunch, late-night |
| **Ingredient Spotlight** | tofu, lentils, seafood, offal, root-vegetables, stone-fruit, fresh-herbs, rice, noodles, beans |
| **Effort** | 5-ingredient, one-pot, weekend-project, multi-day |
| **Era / Tradition** | historical, heirloom, modern-fusion |
| **Temperature** | cold-dish, frozen-dessert, hot-soup, room-temp |

### Sampling Strategy

Don't fill the grid — sample from it with taste:

1. **Cuisine-anchored generation**: Each cuisine gets deep coverage (~400 recipes at scale). The agent picks tag combinations that are culturally natural.
2. **Gap-driven iteration**: After each batch, `gk-content gaps` shows which non-cuisine axes are sparse. Next batch fills those holes through cuisine-appropriate recipes.
3. **Agent judgment over mechanical coverage**: "Gluten-free Cantonese fermented historical breakfast" is not a goal. The agent picks 2-4 non-cuisine tags per recipe that make sense together.

## Components

### 1. `config/recipe-grid.toml`

Defines every valid tag organized by axis. No targets or counts — just the vocabulary. The TOML structure uses `[axes.<name>]` sections with `display` (human name) and `tags` (list of valid values).

### 2. `gk-content` Rust CLI

New binary in the workspace with subcommands:

**`gk-content gaps [--cuisine X] [--attribute X] [--ignore X,...] [--json]`**
- Queries the Tag table, groups by axis
- Shows counts per tag, highlights sparse areas
- `--ignore` suppresses axes from the report
- `--json` for agent consumption, human-readable table by default

**`gk-content ingest <recipes.json>`**
- Takes a JSON array of `{name, content, tags, image_prompt?}`
- Pushes each via `Recipe::push` + `Tag::push`
- If `image_prompt` is present, calls `image-gen` and pushes via `Image::push`

**`gk-content ingest-tags <tags.json>`**
- Takes `{recipe_id: [tags]}` mapping
- Applies tags to existing recipes via `Tag::push`

### 3. Agent Workflow

Not code — prompts and instructions for Claude Code / Codex sessions:

1. Run `gk-content gaps --json` to see what's underrepresented
2. Pick a focus area (cuisine, axis, or combination)
3. Generate 10-50 recipes as structured JSON
4. Write to file, run `gk-content ingest recipes.json`
5. Optionally run image generation per recipe

### 4. Seasonal Calendar

New DB table:

```sql
CREATE TABLE FrontPageSection (
    date TEXT NOT NULL,        -- 'MM-DD' for annual mapping
    section TEXT NOT NULL,     -- 'featured', 'spotlight', 'seasonal', 'healthy'
    title TEXT NOT NULL,       -- 'Food Spotlight: Northern China'
    blurb TEXT,                -- 'Hearty, warming dishes from Dongbei...'
    query_tags TEXT NOT NULL,  -- JSON: ["dongbei", "comfort-food"]
    PRIMARY KEY (date, section)
);
```

**`gk-content schedule`** generates or updates this table. Calendar-mapped themes:

| Months | Themes |
|--------|--------|
| Jan-Feb | Hearty soups, braised dishes, comfort food, slow-cooker |
| Mar-Apr | Fresh herbs, light dishes, spring vegetables |
| May-Jun | Grilling, salads, cold dishes, quick-and-easy |
| Jul-Aug | No-cook, frozen desserts, cold dishes, seafood |
| Sep-Oct | Harvest, root vegetables, baking, weekend projects |
| Nov-Dec | Holiday/dinner-party, indulgent, historical, multi-day |

The front page queries `FrontPageSection` for today's `MM-DD` and renders sections.

### 5. Front Page Redesign

The index template renders sections from `FrontPageSection`:

- **Featured Recipe** — hero card with editorial blurb
- **Healthy Options** — grid filtered by `healthy` tag
- **Food Spotlight: {title}** — rotating cuisine/theme, changes daily
- **Seasonal Pick** — tag-driven, tied to calendar themes
- (Existing random grid remains as "Discover" or "More Recipes")

## Milestones

### M1: Recipe Grid + Gap Analysis
- Create `config/recipe-grid.toml` with all 8 axes
- New `gk-content` binary with `gaps` subcommand
- Human-readable and JSON output modes
- Tests for gap counting logic

### M2: Ingestion CLI
- `gk-content ingest` for new recipes from JSON
- `gk-content ingest-tags` for tagging existing recipes
- Integration with `image-gen` for image generation
- Tests for ingestion

### M3: Auto-Tag Existing Recipes
- Agent session: read all 488 recipes, classify against fixed vocabulary
- Run via `gk-content ingest-tags`
- Verify with `gk-content gaps` that tags distributed sensibly

### M4: First Generation Batch (500-1000 recipes)
- Agent sessions focused on underrepresented cuisines
- Prioritize cuisines with zero recipes (most of them)
- Each recipe gets 2-4 tags across axes
- Generate images for each

### M5: Seasonal Calendar
- `FrontPageSection` schema migration
- `gk-content schedule` subcommand
- Agent generates 365 days of section data (titles, blurbs, tag queries)

### M6: Front Page Redesign
- Index template renders `FrontPageSection`-driven sections
- Featured recipe hero, healthy options grid, rotating spotlight, seasonal pick
- Existing random grid becomes "Discover More"

### M7: Scale to 10k
- Continued agent generation runs
- Use `gk-content gaps` iteratively to guide generation
- Deep per-cuisine coverage

## Files Created/Modified

| File | Change |
|------|--------|
| `config/recipe-grid.toml` | New: tag vocabulary |
| `gk-content/Cargo.toml` | New: CLI crate |
| `gk-content/src/main.rs` | New: subcommands (gaps, ingest, ingest-tags, schedule) |
| `gk-server/src/database.rs` | Migration: FrontPageSection table |
| `gk-server/src/models.rs` | FrontPageSection model |
| `gk-server/src/lib.rs` | Front page handler queries FrontPageSection |
| `gk-server/templates/index.html.jinja` | Redesigned with sections |

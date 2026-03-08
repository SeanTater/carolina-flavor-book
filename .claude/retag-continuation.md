# Recipe Retag Continuation — Batches 46-64

## Context

We're bulk-retagging all 3245 recipes. 2300 recipes (batches 0-45) are done and applied. 19 batches remain (46-64, ~945 recipes). The code, batch input files, and tooling are all in place.

## What's already done

- `config/recipe-grid.toml` has new dietary tags (vegan, dairy-free, nut-free, kosher, pescatarian)
- `gk-content` has `retag` and `apply-retag` subcommands (on branch `feature/retag-recipes` in `.worktrees/retag`)
- Batch input files exist at `/tmp/retag-batch-{0..64}.json`
- Output files exist at `/tmp/retag-output-{0..45}.json`
- All 2300 recipes from batches 0-45 have been applied to prod

## What needs to happen

### Step 1: Tag remaining batches

For each batch N from 46 to 64, dispatch a general-purpose agent with this prompt (replacing N):

```
You are a recipe tagger. Read /tmp/retag-batch-N.json. For each recipe, assign fresh tags based on actual content. Write output to /tmp/retag-output-N.json.

Output format: {"recipe_id_string": {"tags": ["tag1", "tag2"], "provenance": ["from-input"]}, ...}

Dietary restriction rules (check actual ingredients, when in doubt do NOT apply):
- vegetarian: NO meat/poultry/fish/gelatin. Check for hidden meat: stock, lard, anchovy, fish sauce, Worcestershire, bacon.
- vegan: vegetarian + no dairy/eggs/honey.
- gluten-free: NO wheat/barley/rye/flour/bread/pasta/couscous/seitan/soy sauce(unless tamari).
- dairy-free: NO milk/cream/butter/ghee/cheese/yogurt.
- nut-free: NO tree nuts/peanuts/almond flour. Coconut OK.
- kosher: NO pork/shellfish, no mixing meat+dairy.
- pescatarian: vegetarian but fish/seafood OK.

Valid tags — Cuisine: american-south, american-new-england, american-midwest, american-tex-mex, american-pacific-nw, american-hawaiian, british, german, italian, french, swedish, serbian, georgian, greek, spanish, polish, mexican, brazilian, cuban, peruvian, japanese, korean, cantonese, sichuan, dongbei, hunan, fujian, yunnan, thai, indian-north, indian-south, vietnamese, lebanese, moroccan, ethiopian, turkish | Attribute: vegetarian, vegan, gluten-free, dairy-free, nut-free, kosher, pescatarian, indulgent, authentic, quick-and-easy, low-cholesterol, low-sodium, comfort-food, healthy, baking | Method: grilled, slow-cooker, braised, raw, no-cook, fermented, smoked, deep-fried, steamed, stir-fried, baked, one-pot | Occasion: breakfast, lunch, dinner, snack, dinner-party, potluck, packed-lunch, late-night | Ingredient: tofu, lentils, seafood, offal, root-vegetables, stone-fruit, fresh-herbs, rice, noodles, beans | Effort: 5-ingredient, one-pot, weekend-project, multi-day | Era: historical, heirloom, modern-fusion | Temperature: cold-dish, frozen-dessert, hot-soup, room-temp | Season: spring, summer, fall, winter

Rules: 3-8 tags per recipe. STRICT with dietary. Do NOT apply frozen-dessert unless actually frozen. Every recipe must appear in output. Pass provenance through unchanged.
```

You can run all 19 in parallel as background agents.

### Step 2: Apply results

Once all output files exist (check with `ls /tmp/retag-output-{46..64}.json`):

```bash
cd /home/sean/repos/recipes/.worktrees/retag
cargo run -p gk-content -- --config config/prod.toml apply-retag
```

This is idempotent — it re-applies all output files, so the already-applied batches 0-45 just get re-patched with the same data.

### Step 3: Verify

```bash
# Check tag distribution
cargo run -p gk-content -- --config config/prod.toml gaps | head -40

# Spot-check: these should NOT have the listed tags
# - Chicken recipes should NOT have vegetarian
# - Bread recipes should NOT have gluten-free
# - Casseroles/soups should NOT have frozen-dessert
```

### Step 4: Merge code

The code changes are on branch `feature/retag-recipes` in the worktree at `.worktrees/retag`. After verification, merge to master:

```bash
cd /home/sean/repos/recipes
git merge feature/retag-recipes
```

## Important notes

- The batch files are in `/tmp` so they'll survive reboots on this machine but aren't permanent. If they're gone, re-run: `cargo run -p gk-content -- --config config/prod.toml retag`
- The `apply-retag` command uses `PATCH /api/recipe/{id}` which does full tag replacement (not additive), so it's safe to re-run
- Provenance tags (church-cookbook, pin-like, hyman, freestyle, from-notes, breadmaker, manual, contrib, bulk) are preserved by the agents — they pass them through in the "provenance" field and `apply-retag` merges them back

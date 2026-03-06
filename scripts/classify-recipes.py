#!/usr/bin/env python3
"""Classify recipes with missing tags using Ollama.

Incremental: only processes recipes missing tags on one or more axes.
Reusable: point at any server via --config.

Usage:
    python3 scripts/classify-recipes.py --config config/dev.toml
    python3 scripts/classify-recipes.py --config config/prod.toml --apply
    python3 scripts/classify-recipes.py --config config/dev.toml --ollama-url http://localhost:11435 --model qwen3.5:35b
"""
import json, subprocess, sys, os, urllib.request, time, argparse, concurrent.futures
from collections import defaultdict

# --- Argument parsing ---
parser = argparse.ArgumentParser(description="Classify recipes with missing tags")
parser.add_argument("--config", required=True, help="Path to server config TOML")
parser.add_argument("--apply", action="store_true", help="Apply tags via gk-content ingest-tags")
parser.add_argument("--api-url", default="http://localhost:11435", help="LLM API base URL (Ollama or OpenAI-compatible)")
parser.add_argument("--api-type", choices=["ollama", "openai"], default="ollama", help="API type")
parser.add_argument("--model", default="qwen3.5:35b", help="Model name")
parser.add_argument("--batch-size", type=int, default=20, help="Recipes per batch")
parser.add_argument("--workers", type=int, default=3, help="Parallel requests")
args = parser.parse_args()

# --- Parse server config ---
server_url = None
token = None
with open(args.config) as f:
    for line in f:
        line = line.strip()
        if line.startswith("address"):
            server_url = "http://" + line.split('"')[1]
        if line.startswith("service_principal_secret"):
            token = line.split('"')[1]

if not server_url or not token:
    print(f"Could not parse server address or token from {args.config}")
    sys.exit(1)
print(f"Server: {server_url}")
print(f"LLM: {args.api_url} ({args.api_type}) model={args.model}")

# --- Tag vocabulary (from recipe-grid.toml) ---
TAG_AXES = {
    "cuisine": [
        "american-south", "american-classic", "american-modern", "american-baking", "american-diner",
        "british", "german", "italian", "french", "swedish", "serbian", "georgian", "greek",
        "spanish", "polish", "mexican", "brazilian", "cuban", "peruvian",
        "japanese", "korean", "cantonese", "sichuan", "dongbei", "hunan", "fujian", "yunnan",
        "thai", "indian-north", "indian-south", "vietnamese", "lebanese", "moroccan", "ethiopian", "turkish",
    ],
    "attribute": [
        "vegetarian", "gluten-free", "indulgent", "authentic", "quick-and-easy",
        "low-cholesterol", "low-sodium", "comfort-food", "healthy",
    ],
    "method": [
        "grilled", "slow-cooker", "braised", "raw", "no-cook", "fermented",
        "smoked", "deep-fried", "steamed", "stir-fried", "baked", "one-pot",
    ],
    "occasion": [
        "breakfast", "lunch", "dinner", "snack", "dinner-party", "potluck", "packed-lunch", "late-night",
    ],
    "ingredient": [
        "tofu", "lentils", "seafood", "offal", "root-vegetables", "stone-fruit",
        "fresh-herbs", "rice", "noodles", "beans",
    ],
    "effort": ["5-ingredient", "one-pot", "weekend-project", "multi-day"],
    "era": ["historical", "heirloom", "modern-fusion"],
    "temperature": ["cold-dish", "frozen-dessert", "hot-soup", "room-temp"],
    "season": ["spring", "summer", "fall", "winter"],
}
ALL_VALID_TAGS = set()
TAG_TO_AXIS = {}
for axis, tags in TAG_AXES.items():
    for t in tags:
        ALL_VALID_TAGS.add(t)
        TAG_TO_AXIS[t] = axis

TARGET_AXES = {"season", "occasion", "method", "attribute", "ingredient", "temperature"}

SYSTEM_PROMPT = """You are a recipe classifier. For each recipe, assign tags from ONLY these valid tags:

AXES TO CLASSIFY:
- season: spring, summer, fall, winter — assign based on seasonal ingredients (e.g. pumpkin=fall, berries=summer). Many recipes are year-round; only tag if clearly seasonal.
- occasion: breakfast, lunch, dinner, snack, dinner-party, potluck, packed-lunch, late-night — what meal is this? Most main dishes are "dinner". Assign multiple if appropriate.
- method: grilled, slow-cooker, braised, raw, no-cook, fermented, smoked, deep-fried, steamed, stir-fried, baked, one-pot — primary cooking method.
- attribute: vegetarian, gluten-free, indulgent, authentic, quick-and-easy, low-cholesterol, low-sodium, comfort-food, healthy — "vegetarian" only if no meat/fish. "quick-and-easy" if under 30 min active time.
- ingredient: tofu, lentils, seafood, offal, root-vegetables, stone-fruit, fresh-herbs, rice, noodles, beans — only if it's a PRIMARY component.
- temperature: cold-dish, frozen-dessert, hot-soup, room-temp — only if serving temperature is distinctive.

RULES:
- Only add tags the recipe clearly qualifies for. When in doubt, skip.
- Do NOT repeat tags the recipe already has.
- Output ONLY valid JSON (no markdown fences, no explanation): a map of recipe_id (as string) to array of new tags.
- Example: {"12345": ["dinner", "baked", "comfort-food"], "67890": ["summer", "grilled"]}"""


def api_get(path):
    req = urllib.request.Request(
        f"{server_url}{path}",
        headers={"Authorization": f"Bearer {token}"}
    )
    with urllib.request.urlopen(req) as resp:
        return json.load(resp)


def llm_chat(messages, retries=3):
    """Call LLM API and return the assistant message content."""
    if args.api_type == "openai":
        payload = json.dumps({
            "model": args.model,
            "messages": messages,
            "temperature": 0.3,
            "max_tokens": 16384,
            "response_format": {"type": "json_object"},
            "chat_template_kwargs": {"enable_thinking": False},
        }).encode()
        url = f"{args.api_url}/v1/chat/completions"
    else:  # ollama
        payload = json.dumps({
            "model": args.model,
            "messages": messages,
            "stream": False,
            "format": "json",
            "options": {"temperature": 0.3, "num_predict": 16384},
        }).encode()
        url = f"{args.api_url}/api/chat"

    for attempt in range(retries):
        try:
            req = urllib.request.Request(url, data=payload, headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(req, timeout=600) as resp:
                body = json.load(resp)

            if args.api_type == "openai":
                return body["choices"][0]["message"]["content"]
            else:
                content = body["message"].get("content", "")
                if not content.strip():
                    content = body["message"].get("thinking", "")
                return content
        except Exception as e:
            if attempt < retries - 1:
                wait = 5 * (attempt + 1)
                print(f"    Retry {attempt+1}/{retries} after error: {str(e)[:100]} (waiting {wait}s)")
                time.sleep(wait)
            else:
                raise


def classify_batch(batch_idx, batch):
    """Classify a batch of recipes via Ollama. Returns (batch_idx, result_dict)."""
    recipe_descriptions = []
    for r in batch:
        desc = f"ID: {r['recipe_id']}\nName: {r['name']}\nExisting tags: {', '.join(r['existing_tags'])}\nMissing axes: {', '.join(r['missing_axes'])}"
        if r.get("content_text"):
            desc += f"\nContent: {r['content_text'][:800]}"
        recipe_descriptions.append(desc)

    user_prompt = (
        f"Classify these {len(batch)} recipes. Only assign tags for the missing axes listed.\n\n"
        + "\n---\n".join(recipe_descriptions)
    )

    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": user_prompt},
    ]

    try:
        text = llm_chat(messages)
        # Strip markdown fences if present
        text = text.strip()
        if text.startswith("```"):
            text = text.split("\n", 1)[1] if "\n" in text else text[3:]
            if text.endswith("```"):
                text = text[:-3]
            text = text.strip()
        # Find JSON object
        start = text.find("{")
        end = text.rfind("}") + 1
        if start >= 0 and end > start:
            return batch_idx, json.loads(text[start:end]), None
        else:
            # Save raw for debugging
            with open(f"/tmp/tagging/batch-{batch_idx}-raw.txt", "w") as dbg:
                dbg.write(text)
            return batch_idx, None, f"no JSON found (len={len(text)})"
    except Exception as e:
        return batch_idx, None, str(e)[:200]


# --- Step 1: Fetch recipes and tags ---
print("\n=== Fetching recipes ===")
all_recipes = api_get("/api/recipes/basic")
print(f"  {len(all_recipes)} recipes")

all_tags_list = api_get("/api/tags")
print(f"  {len(all_tags_list)} tag assignments")

existing_tags = defaultdict(set)
for t in all_tags_list:
    existing_tags[t["recipe_id"]].add(t["tag"])


def covered_axes(tags):
    return {TAG_TO_AXIS[t] for t in tags if t in TAG_TO_AXIS}


# --- Step 2: Find recipes needing classification ---
print("\n=== Analyzing tag coverage ===")
needs_classification = []
for recipe in all_recipes:
    rid = recipe["recipe_id"]
    tags = existing_tags.get(rid, set())
    missing = TARGET_AXES - covered_axes(tags)
    if missing:
        needs_classification.append({
            "recipe_id": rid,
            "name": recipe["name"],
            "existing_tags": sorted(tags),
            "missing_axes": sorted(missing),
        })

print(f"  {len(needs_classification)} recipes need classification")
print(f"  {len(all_recipes) - len(needs_classification)} already fully tagged")

if not needs_classification:
    print("\nAll recipes are tagged. Nothing to do.")
    sys.exit(0)

# --- Step 3: Fetch recipe text ---
print("\n=== Fetching recipe text ===")
all_text = api_get("/api/recipes/text")
text_by_id = {r["recipe_id"]: r.get("content_text", "") for r in all_text}

for r in needs_classification:
    r["content_text"] = text_by_id.get(r["recipe_id"], "")[:2000]

# --- Step 4: Classify in parallel ---
batches = [needs_classification[i:i+args.batch_size] for i in range(0, len(needs_classification), args.batch_size)]
print(f"\n=== Classifying in {len(batches)} batches ({args.workers} workers) ===")

os.makedirs("/tmp/tagging", exist_ok=True)
results = {}
failed = 0

with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
    futures = {pool.submit(classify_batch, i, b): i for i, b in enumerate(batches)}
    for future in concurrent.futures.as_completed(futures):
        batch_idx, batch_result, error = future.result()
        if error:
            print(f"  FAIL batch {batch_idx+1}/{len(batches)}: {error}")
            failed += 1
        else:
            results.update(batch_result)
            print(f"  OK batch {batch_idx+1}/{len(batches)}: {len(batch_result)} recipes")

if failed:
    print(f"\n  {failed}/{len(batches)} batches failed")

# --- Step 5: Validate and merge ---
print(f"\n=== Validating {len(results)} classifications ===")
final_tags = {}
invalid_count = 0
tag_counts = defaultdict(int)

for rid_str, tags in results.items():
    rid = int(rid_str)
    existing = existing_tags.get(rid, set())
    valid_new = []
    for t in tags:
        if t not in ALL_VALID_TAGS:
            invalid_count += 1
            continue
        if t in existing:
            continue
        valid_new.append(t)
        tag_counts[t] += 1
    if valid_new:
        final_tags[rid] = valid_new

print(f"  {len(final_tags)} recipes will get new tags")
print(f"  {sum(len(v) for v in final_tags.values())} total new tag assignments")
if invalid_count:
    print(f"  {invalid_count} invalid tags dropped")

print("\n  Top tags being added:")
for tag, count in sorted(tag_counts.items(), key=lambda x: -x[1])[:15]:
    print(f"    {tag}: {count}")

# --- Step 6: Write output ---
timestamp = time.strftime("%Y%m%d-%H%M%S")
output_path = f"/tmp/tagging/tags-{timestamp}.json"
with open(output_path, "w") as f:
    json.dump(final_tags, f, indent=2)
print(f"\nWrote {output_path}")

# --- Step 7: Optionally apply ---
if args.apply and final_tags:
    print("\n=== Applying tags ===")
    result = subprocess.run(
        ["cargo", "run", "-q", "-p", "gk-content", "--", "--config", args.config, "ingest-tags", output_path],
        capture_output=True, text=True,
    )
    print(result.stdout.strip())
    if result.returncode != 0:
        print(f"FAIL: {result.stderr.strip()}")
elif not args.apply and final_tags:
    print(f"\nDry run. To apply:\n  python3 scripts/classify-recipes.py --config {args.config} --apply")
    print(f"Or manually:\n  cargo run -p gk-content -- --config {args.config} ingest-tags {output_path}")

# Recipe Generator Agent Memory

## Workflow Notes
- The Write tool requires reading a file first if it already exists — use `cat > file` via Bash for /tmp files that may have prior content from other sessions
- Session memory at /tmp/recipe-agent-memory.json does not persist across sessions; always initialize from `{"created_dishes":[]}`
- Ingest command: `cargo run -p gk-content -- --config config/prod.toml ingest FILE --images --image-gen-arg=--port --image-gen-arg=9091`
- Search command: `cargo run -p gk-content -- --config config/prod.toml search "keyword"`
- Archive of batches 32–141 DB contents: see db-contents-archive.md

## Tag Validation Reminders
- `beans` IS a valid Ingredient Spotlight tag
- `dim sum` is NOT a valid tag — use `breakfast` or `lunch` for meal occasion
- `street-food-style` and `street-food` are NOT valid tags — use `late-night` or `snack`
- Always include the cuisine tag — it is the most commonly missed tag
- Do NOT tag `gluten-free` if recipe contains soy sauce, wheat starch, filo, or flour
- Do NOT add sugar to beans until fully cooked — it hardens the skins
- Ha Gau wrappers use wheat starch — do NOT tag gluten-free

## Dishes Confirmed in Database (batch 151)
- Hawaiian Loco Moco (comfort-food/authentic/heirloom/dinner/lunch/one-pot), Hawaiian Shoyu Chicken (comfort-food/authentic/heirloom/dinner/lunch/one-pot/braised) (american-hawaiian)
- Poi / Hawaiian Taro Paste (authentic/historical/heirloom/vegetarian/gluten-free/room-temp), Lomi Lomi Salmon (seafood/authentic/heirloom/cold-dish/gluten-free/no-cook/healthy) (american-hawaiian)
- Hawaiian Butter Mochi (baking/vegetarian/authentic/heirloom/comfort-food/potluck/snack), King's Hawaiian Sweet Bread / Pão Doce (baking/vegetarian/authentic/heirloom/comfort-food/breakfast/weekend-project) (american-hawaiian)
- Hawaiian Acai Bowl with Granola and Fresh Fruit (breakfast/healthy/vegetarian/gluten-free/quick-and-easy/modern-fusion), Hawaiian Portuguese Sausage and Eggs (breakfast/authentic/heirloom/comfort-food/quick-and-easy) (american-hawaiian)
- Nobu-Style Miso-Glazed Butterfish / Black Cod (seafood/authentic/modern-fusion/dinner-party/weekend-project/healthy), Lilikoi (Passion Fruit) Cheesecake (baking/vegetarian/authentic/modern-fusion/dinner-party/indulgent/cold-dish) (american-hawaiian)
- NOTE: "Cathy's Quick Pineapple Upside Down Cake" already in DB — used Lilikoi Cheesecake for second modern-fusion/dessert slot
- NOTE: Acai bowl tagged gluten-free — recipe contains no gluten (granola listed as optional; recipe body calls it the "preferred" topping, so tag is reasonable if using GF granola, but safer to drop gluten-free tag in future if granola is included by default)

## Dishes Confirmed in Database (batch 149)
- Sopapillas with Honey (deep-fried/vegetarian/comfort-food/authentic/heirloom/snack/baking), Tex-Mex Tres Leches Cake (baking/vegetarian/indulgent/authentic/dinner-party/comfort-food/heirloom) (american-tex-mex)
- Gulf Coast Shrimp Ceviche (seafood/gluten-free/healthy/authentic/cold-dish/snack/no-cook), Sizzling Shrimp Fajitas (seafood/authentic/quick-and-easy/dinner/dinner-party/comfort-food) (american-tex-mex)
- Tex-Mex Bean and Cheese Tacos (vegetarian/beans/comfort-food/authentic/heirloom/breakfast/quick-and-easy/snack), Chiles Rellenos with Tomato Ranchera Sauce (vegetarian/deep-fried/authentic/comfort-food/dinner/dinner-party) (american-tex-mex)
- Tex-Mex Chile con Queso (vegetarian/comfort-food/authentic/snack/potluck/indulgent/quick-and-easy), Seven-Layer Tex-Mex Dip (vegetarian/comfort-food/authentic/potluck/snack/cold-dish/heirloom) (american-tex-mex)
- Smoked Quail with Jalapeño-Honey Glaze (smoked/grilled/authentic/dinner-party/indulgent/gluten-free/weekend-project), Enchiladas Suizas (baked/authentic/dinner-party/indulgent/comfort-food/dinner/heirloom) (american-tex-mex)
- NOTE: Fish tacos already exist in DB (Best Baja Fish Tacos + Tacos de Pescado al Pastor) — used Shrimp Fajitas for second seafood slot instead
- NOTE: Ceviche has many DB entries (Cuban, Peruvian, Brazilian) — Gulf Coast Shrimp Ceviche (poached shrimp, Tex-Mex style) is distinct

## Dishes Confirmed in Database (batch 146)
- Shoyu Ahi Poke Bowl (seafood/raw/healthy/quick-and-easy/authentic), Spam Musubi (authentic/snack/lunch/comfort-food/heirloom) (american-hawaiian)
- Kalua Pork / Slow-Cooker Imu Style (slow-cooker/authentic/comfort-food/dinner/heirloom/one-pot), Lau Lau / Pork and Butterfish in Taro Leaves (steamed/authentic/heirloom/dinner/weekend-project) (american-hawaiian)
- Chicken Katsu with Tonkatsu Sauce (deep-fried/authentic/comfort-food/dinner/lunch), Hawaiian Plate Lunch Macaroni Salad (authentic/comfort-food/potluck/lunch/heirloom/cold-dish) (american-hawaiian)
- Haupia / Hawaiian Coconut Pudding (vegetarian/gluten-free/authentic/heirloom/cold-dish), Leonard's-Style Malasadas (deep-fried/baking/authentic/indulgent/breakfast/heirloom/weekend-project) (american-hawaiian)
- Macadamia-Crusted Mahi-Mahi with Lilikoi Butter Sauce (seafood/baked/dinner-party/modern-fusion/authentic), Seared Ahi Tuna with Ponzu, Avocado, and Crispy Wonton (seafood/modern-fusion/dinner-party/healthy/quick-and-easy) (american-hawaiian)
- NOTE: Generic "Macaroni Salad" already exists in DB — used "Hawaiian Plate Lunch Macaroni Salad" as distinctive title

## Dishes Confirmed in Database (batch 145)
- Dungeness Crab Cioppino (seafood/one-pot/comfort-food/dinner-party/dinner/authentic), Alder-Smoked Oysters with Pickled Cucumber Mignonette (seafood/smoked/authentic/dinner-party/gluten-free) (american-pacific-nw)
- Foraged Chanterelle and Morel Ragout on Creamy Polenta (vegetarian/modern-fusion/dinner-party/authentic/fresh-herbs), Pan-Seared Halibut with Spring Fiddleheads and Lemon-Caper Butter (seafood/dinner-party/authentic/healthy/quick-and-easy/fresh-herbs) (american-pacific-nw)
- Pacific Northwest Salmon Chowder (seafood/comfort-food/authentic/hot-soup/one-pot/dinner), Marionberry Cobbler (baking/vegetarian/comfort-food/heirloom/authentic) (american-pacific-nw)
- Alder-Wood Grilled Whole Chinook Salmon (grilled/smoked/seafood/authentic/gluten-free/dinner-party), Coffee-Rubbed Smoked Tri-Tip with Huckleberry Barbecue Sauce (smoked/grilled/authentic/dinner-party/weekend-project/stone-fruit) (american-pacific-nw)
- Walla Walla Onion and Gruyère Tart (vegetarian/baking/authentic/dinner-party/heirloom), Rainier Cherry and Hazelnut Salad with Goat Cheese (vegetarian/healthy/stone-fruit/fresh-herbs/authentic/quick-and-easy) (american-pacific-nw)
- NOTE: "salad" is not a valid tag — use healthy/vegetarian/fresh-herbs etc. instead
- NOTE: Cedar plank salmon already existed as "Eli's Cedar Plank Salmon" — used alder-grilled whole Chinook instead

## Dishes Confirmed in Database (batch 144)
- Tex-Mex Cheese Enchiladas with Chile Gravy (comfort-food/baked/authentic/heirloom/dinner), King Ranch Chicken Casserole (comfort-food/baked/heirloom/potluck/dinner) (american-tex-mex)
- Smoked Beef Brisket Tacos (smoked/authentic/dinner/weekend-project/dinner-party), Carne Asada with Avocado Salsa (grilled/authentic/dinner/quick-and-easy/gluten-free) (american-tex-mex)
- Tex-Mex Migas (breakfast/authentic/comfort-food/quick-and-easy/vegetarian), Tex-Mex Breakfast Tacos (breakfast/authentic/comfort-food/quick-and-easy/indulgent) (american-tex-mex)
- Texas Chili con Carne (one-pot/authentic/heirloom/comfort-food/dinner/gluten-free), Charro Beans / Frijoles Charros (one-pot/beans/authentic/comfort-food/potluck/dinner) (american-tex-mex)
- Crispy Beef Flautas with Crema (deep-fried/snack/late-night/authentic/indulgent), San Antonio Puffy Tacos (deep-fried/authentic/snack/late-night/dinner/weekend-project) (american-tex-mex)
- NOTE: King Ranch Chicken Soup already existed as a slow-cooker version — the casserole is distinct and was safe to add
- NOTE: Pozole already exists in DB (multiple versions) — used Charro Beans for the one-pot slot instead

## Dishes Confirmed in Database (batch 143)
- Funeral Potatoes / Cheesy Hash Brown Casserole (comfort-food/potluck/baked/vegetarian/indulgent), Minnesota Wild Rice Hotdish (comfort-food/potluck/baked/one-pot/heirloom/authentic) (american-midwest)
- Grilled Bratwurst with Beer and Onions (grilled/authentic/comfort-food/dinner/potluck/heirloom), Iowa Thick-Cut Pork Chops with Apple Cider Glaze (grilled/authentic/comfort-food/dinner/dinner-party) (american-midwest)
- Czech-Style Kolache (baking/breakfast/heirloom/authentic/comfort-food/weekend-project), Swedish Tea Ring (baking/breakfast/heirloom/authentic/comfort-food/weekend-project) (american-midwest)
- Cincinnati-Style Chili (one-pot/authentic/heirloom/comfort-food/dinner/noodles), American Goulash (one-pot/comfort-food/heirloom/quick-and-easy/dinner/noodles) (american-midwest)
- Fried Wisconsin Cheese Curds (deep-fried/authentic/snack/late-night/comfort-food/indulgent), Detroit-Style Deep Dish Pizza (baked/authentic/comfort-food/dinner/modern-fusion/indulgent/weekend-project) (american-midwest)

## Dishes Confirmed in Database (batch 147)
- New England Fried Clams (deep-fried/seafood/authentic/heirloom/snack/lunch), New England Lobster Bisque (seafood/comfort-food/indulgent/dinner-party/dinner/authentic) (american-new-england)
- New England Apple Cider Donuts (deep-fried/baking/breakfast/snack/heirloom/authentic/comfort-food/weekend-project), Yankee Beef and Root Vegetable Pot Pie (baking/braised/comfort-food/heirloom/dinner/root-vegetables/weekend-project/authentic) (american-new-england)
- Maine Lobster Salad (seafood/cold-dish/authentic/heirloom/lunch/no-cook/fresh-herbs), New England Strawberry Shortcake (baking/vegetarian/comfort-food/heirloom/authentic/breakfast/stone-fruit) (american-new-england)
- New England Salt Cod Fish Cakes (seafood/comfort-food/heirloom/authentic/quick-and-easy/dinner/lunch), New England Corn Chowder (comfort-food/heirloom/authentic/one-pot/quick-and-easy/dinner/hot-soup) (american-new-england)
- Indian Pudding (baking/vegetarian/historical/heirloom/authentic/comfort-food/dinner-party), Parker House Rolls (baking/vegetarian/historical/heirloom/authentic/comfort-food/dinner-party/breakfast) (american-new-england)
- NOTE: Chicken pot pie has many versions in DB — used a distinct "Yankee Beef and Root Vegetable Pot Pie" to avoid duplication
- NOTE: Oyster stew already in DB (Aaron's and Low Country versions) — used Lobster Bisque for second seafood slot instead

## Dishes Confirmed in Database (batch 148)
- Sour Cream Coffee Cake (baking/breakfast/comfort-food/vegetarian/potluck/heirloom), Hoosier Persimmon Pudding (baking/historical/heirloom/comfort-food/authentic/vegetarian/weekend-project) (american-midwest)
- Midwestern Chicken and Noodles (comfort-food/heirloom/one-pot/dinner/potluck/authentic/noodles), Pretzel Jello Salad (comfort-food/heirloom/potluck/vegetarian/cold-dish/authentic) (american-midwest)
- German Apple Pancake / Dutch Baby (breakfast/vegetarian/comfort-food/authentic/heirloom/quick-and-easy), Norwegian Lefse (breakfast/heirloom/authentic/vegetarian/comfort-food/weekend-project/historical) (american-midwest)
- Pan-Fried Walleye with Lemon Dill Butter (seafood/dinner-party/authentic/quick-and-easy/fresh-herbs), Craft Beer-Braised Short Ribs with Horseradish Gremolata (braised/dinner-party/modern-fusion/comfort-food/authentic/weekend-project) (american-midwest)
- Iowa Loose Meat Sandwiches / Maid-Rite Style (comfort-food/heirloom/authentic/quick-and-easy/lunch/dinner/snack), Wisconsin Butter Burger (comfort-food/authentic/indulgent/dinner/lunch/quick-and-easy/heirloom) (american-midwest)
- NOTE: Sloppy Joes already in DB (multiple versions) — used Loose Meat Sandwich instead
- NOTE: Tater Tot Casserole already in DB (multiple versions) — used Pretzel Jello Salad for potluck slot
- NOTE: Pimento Cheese already in DB (multiple versions) — skipped

## Dishes Confirmed in Database (batch 150)
- Huckleberry Buttermilk Pancakes (breakfast/vegetarian/comfort-food/authentic/heirloom/stone-fruit), Pacific NW Smoked Salmon Eggs Benedict (breakfast/seafood/indulgent/authentic/dinner-party/fresh-herbs) (american-pacific-nw)
- Pacific NW Geoduck Crudo with Ponzu and Pickled Ginger (seafood/raw/gluten-free/healthy/authentic/dinner-party/no-cook), Pan-Fried Razor Clams with Lemon Herb Butter (seafood/authentic/comfort-food/quick-and-easy/dinner/heirloom/fresh-herbs) (american-pacific-nw)
- Tillamook Cheddar and Jalapeño Pull-Apart Bread (baking/vegetarian/comfort-food/authentic/potluck/weekend-project/indulgent), Oregon Hazelnut Shortbread Cookies (baking/vegetarian/authentic/heirloom/snack/comfort-food) (american-pacific-nw)
- Pacific NW Elk and Pinto Bean Chili (one-pot/comfort-food/authentic/dinner/beans/braised), Pacific NW Venison and Root Vegetable Stew (braised/one-pot/comfort-food/authentic/dinner/root-vegetables/weekend-project) (american-pacific-nw)
- Pacific NW Smoked Salmon Flatbread with Capers and Dill (seafood/snack/quick-and-easy/lunch/authentic/fresh-herbs), Beer-Battered Lingcod Fish and Chips (seafood/deep-fried/comfort-food/authentic/dinner/quick-and-easy) (american-pacific-nw)
- NOTE: "Smoked Salmon Dip" already exists in DB — used Smoked Salmon Flatbread instead (distinct preparation)
- NOTE: "Wild Mushroom Soup" exists as a Yunnan version — used Elk Chili for the one-pot comfort slot instead
- NOTE: "German Apple Pancake (Dutch Baby)" already in DB (Midwest batch) — used Smoked Salmon Benedict for second brunch slot
- NOTE: Fish tacos (Baja style) already in DB (multiple) — used Beer-Battered Lingcod Fish and Chips instead

## Dishes Confirmed in Database (batch 152)
- Rhode Island Clam Cakes (deep-fried/seafood/snack/authentic/heirloom/quick-and-easy), New Haven White Clam Pizza / Apizza alle Vongole (baked/seafood/authentic/late-night/dinner/weekend-project) (american-new-england)
- Connecticut-Style Butter Lobster Roll (seafood/authentic/heirloom/comfort-food/lunch/dinner/quick-and-easy), New England Pickled Bread and Butter Vegetables (no-cook/vegetarian/gluten-free/room-temp/heirloom/authentic/historical/potluck/snack) (american-new-england)
- New England Succotash with Shell Beans and Corn (vegetarian/gluten-free/authentic/heirloom/historical/comfort-food/dinner/potluck), Maple-Glazed Acorn Squash with Pepitas and Sage (vegetarian/gluten-free/authentic/comfort-food/baked/dinner/potluck/healthy) (american-new-england)
- Yankee Pot Roast with Root Vegetables (braised/one-pot/comfort-food/heirloom/authentic/historical/dinner/weekend-project/root-vegetables), Vermont Cheddar and Ale Soup (vegetarian/comfort-food/authentic/hot-soup/one-pot/dinner/indulgent) (american-new-england)
- Maple Walnut Ice Cream (frozen-dessert/vegetarian/authentic/indulgent/heirloom/comfort-food/weekend-project), New England Peach Brown Betty (baked/vegetarian/stone-fruit/comfort-food/heirloom/historical/authentic/dinner-party/potluck) (american-new-england)
- NOTE: Lobster Rolls already in DB (fancy pickled fiddlehead version) — Connecticut-Style Butter Lobster Roll is distinct (hot, butter-only, split-top bun)
- NOTE: Southern Succotash already in DB — New England Succotash is distinct (cranberry/shell beans, no okra, farmhouse style)
- NOTE: Pot roast has generic slow-cooker versions in DB — Yankee Pot Roast is distinct (turnips, historical New England identity, oven-braised)
- NOTE: Connecticut lobster roll is HOT + butter (no mayo) — distinct from Maine cold mayo style and the fiddlehead fusion version

## Dishes Confirmed in Database (batch 153)
- Milwaukee-Style Frozen Custard (frozen-dessert/vegetarian/indulgent/authentic/heirloom/comfort-food), Puppy Chow / Muddy Buddies (snack/comfort-food/potluck/vegetarian/heirloom/authentic/quick-and-easy) (american-midwest)
- Friday Night Fish Fry (seafood/deep-fried/authentic/heirloom/comfort-food/dinner/potluck), Door County Fish Boil (seafood/authentic/heirloom/historical/one-pot/dinner-party/weekend-project) (american-midwest)
- Potato and Cheddar Pierogi (vegetarian/comfort-food/authentic/heirloom/dinner/weekend-project), Minnesota Wild Rice Pilaf with Mushrooms and Herbs (vegetarian/authentic/heirloom/gluten-free/healthy/dinner/rice) (american-midwest)
- Classic Midwestern Deviled Eggs (cold-dish/vegetarian/gluten-free/comfort-food/heirloom/potluck/authentic), Midwestern Sweet Cream Cucumber Salad (cold-dish/vegetarian/gluten-free/healthy/heirloom/authentic/no-cook/potluck) (american-midwest)
- Classic Beef Wellington (dinner-party/weekend-project/indulgent/authentic/baked/heirloom), Holiday Standing Prime Rib Roast with Au Jus (dinner-party/weekend-project/indulgent/authentic/heirloom/multi-day/gluten-free) (american-midwest)
- NOTE: Ambrosia salad already in DB as "Ambrosia Mold (creamy fruit gelatin salad)" — used Deviled Eggs and Cucumber Salad for cold-dish slots instead
- NOTE: Pierogi skillet already in DB ("Game Day Kielbasa & Pierogi Skillet") — Potato and Cheddar Pierogi from scratch is distinct
- NOTE: Puppy Chow is not a frozen dessert despite being in the "frozen-dessert/sweet" suggestion — filed under snack/potluck; used Frozen Custard for the actual frozen-dessert slot

## Dishes Confirmed in Database (batch 154)
- Tex-Mex Birria-Style Braised Beef Tacos (slow-cooker/braised/comfort-food/authentic/dinner/weekend-project), Slow-Cooker Chicken Tinga Tostadas (slow-cooker/braised/comfort-food/authentic/dinner/lunch/one-pot) (american-tex-mex)
- Texas-Style Smoked Pork Spare Ribs (smoked/authentic/weekend-project/dinner/dinner-party/heirloom), Elotes Callejeros / Tex-Mex Grilled Street Corn (grilled/vegetarian/authentic/snack/dinner-party/quick-and-easy) (american-tex-mex)
- Classic Tex-Mex Guacamole (vegetarian/gluten-free/no-cook/cold-dish/authentic/snack/quick-and-easy/potluck), Tex-Mex Frito Pie (comfort-food/authentic/heirloom/snack/late-night/quick-and-easy) (american-tex-mex)
- Roasted Poblano and Cheese Quesadillas (vegetarian/comfort-food/authentic/dinner/lunch/quick-and-easy/snack), Tex-Mex Rice and Black Bean Burrito (vegetarian/comfort-food/authentic/dinner/lunch/packed-lunch/one-pot) (american-tex-mex)
- Mexican Agua Fresca Paletas / Fresh Fruit Ice Pops (frozen-dessert/vegetarian/gluten-free/authentic/snack/healthy/quick-and-easy), Tex-Mex Café de Olla Flan (baking/vegetarian/authentic/indulgent/dinner-party/heirloom/weekend-project) (american-tex-mex)
- NOTE: Carnitas already in DB (2 versions), Barbacoa already in DB (2 versions), Birria/Quesabirria already in DB — used Birria-Style Braised Beef Tacos (Tex-Mex angle) and Chicken Tinga for slow-cooker slots instead
- NOTE: Churros already in DB (Mexican + Cuban versions), Flan Cubano already in DB — used Paletas and Café de Olla Flan as distinct Tex-Mex dessert slots
- NOTE: Pico de Gallo already in DB, Taco Salad already in DB (3 versions) — used Guacamole (0 matches) and Frito Pie (0 matches) for cold/room-temp slots
- NOTE: Existing quesadilla DB entries are all non-Tex-Mex fusions — Roasted Poblano quesadilla is authentically Tex-Mex and distinct

## Dishes Confirmed in Database (batch 155)
- Oregon Pinot Noir Chocolate Cake (baking/indulgent/vegetarian/dinner-party/weekend-project), Marionberry Oat Crumble Bars (baking/vegetarian/comfort-food/potluck/heirloom) (american-pacific-nw)
- Cascadia Smoked Salmon Candy (smoked/fermented/seafood/authentic/heirloom/snack/weekend-project), Pacific NW Quick-Pickled Beets with Dill and Caraway (fermented/vegetarian/gluten-free/heirloom/authentic/room-temp/potluck) (american-pacific-nw)
- Pacific NW Wild Mushroom Risotto with Chanterelles and Thyme (vegetarian/one-pot/authentic/dinner-party/rice/comfort-food), Dungeness Crab and Sweet Corn Bisque (seafood/one-pot/comfort-food/indulgent/dinner-party/hot-soup) (american-pacific-nw)
- Yakima Valley Grilled Lamb Chops with Mint-Herb Chermoula (grilled/authentic/dinner-party/gluten-free/fresh-herbs/healthy), Cedar Plank Grilled Oysters with Garlic-Herb Butter (grilled/smoked/seafood/authentic/dinner-party/gluten-free/fresh-herbs) (american-pacific-nw)
- Pacific NW Dungeness Crab Louie (seafood/cold-dish/authentic/historical/heirloom/lunch/dinner-party/no-cook/gluten-free), Smoked Trout and Watercress Salad with Horseradish Creme Fraiche (seafood/cold-dish/healthy/authentic/gluten-free/quick-and-easy/dinner-party/fresh-herbs) (american-pacific-nw)
- NOTE: Marionberry Cobbler already in DB — used Marionberry Oat Crumble Bars as distinct baking format
- NOTE: "Eli's Cedar Plank Salmon" and alder-smoked oysters already in DB — Cedar Plank Grilled Oysters is distinct (oysters, not salmon; garlic-herb butter focus)
- NOTE: Cascadia Smoked Salmon Candy tagged `fermented` — the multi-day brine + pellicle + smoke process qualifies as a preservation method in the fermented tag context
- NOTE: Clam chowder already in DB (Pacific NW Salmon Chowder + NE versions) — used Crab and Corn Bisque for second one-pot comfort slot

## Dishes Confirmed in Database (batch 156)
- Huli-Huli Chicken (grilled/authentic/heirloom/comfort-food/dinner/dinner-party), Hawaiian Kalbi / Korean-Style BBQ Short Ribs (grilled/authentic/heirloom/comfort-food/dinner/dinner-party) (american-hawaiian)
- Saimin / Hawaiian Noodle Soup (one-pot/authentic/heirloom/comfort-food/noodles/dinner/lunch), Hawaiian Oxtail Soup (one-pot/authentic/heirloom/comfort-food/dinner/braised/weekend-project) (american-hawaiian)
- Hawaiian Kim Chee (fermented/authentic/heirloom/cold-dish/gluten-free/vegetarian/snack/potluck), Namasu / Japanese-Hawaiian Pickled Daikon and Carrot (no-cook/cold-dish/room-temp/vegetarian/gluten-free/authentic/heirloom/potluck/healthy) (american-hawaiian)
- Hawaiian Shave Ice with Tropical Syrups (frozen-dessert/vegetarian/gluten-free/authentic/heirloom/snack/quick-and-easy), Hurricane Popcorn (snack/quick-and-easy/authentic/heirloom/comfort-food/potluck/vegetarian) (american-hawaiian)
- Tofu Poke Bowl (vegetarian/healthy/authentic/modern-fusion/tofu/rice/quick-and-easy/lunch/dinner), Hawaiian Tropical Smoothie Bowl (vegetarian/gluten-free/healthy/breakfast/quick-and-easy/modern-fusion) (american-hawaiian)
- NOTE: Pineapple Coleslaw already in DB — avoided; used Namasu and Hawaiian Kim Chee for cold-dish slots instead
- NOTE: Hawaiian Kim Chee uses green cabbage (not napa), no fish sauce — milder, sweeter than Korean kimchi; correctly NOT tagged gluten-free (contains soy sauce)
- NOTE: Tofu Poke Bowl NOT tagged gluten-free (contains soy sauce); recipe notes tamari substitution for GF

## Dishes Confirmed in Database (batch 142)
- Rhode Island Clear Clam Chowder (seafood/one-pot/authentic/heirloom/hot-soup/comfort-food/dinner), Baked Stuffed Cod with Ritz Cracker Crust (seafood/baked/authentic/comfort-food/heirloom/dinner) (american-new-england)
- Anadama Bread (baking/heirloom/authentic/historical/comfort-food/breakfast), Maine Whoopie Pies (baking/indulgent/heirloom/comfort-food/authentic/snack) (american-new-england)
- New England Boiled Dinner (one-pot/heirloom/authentic/comfort-food/historical/dinner), Boston Baked Beans (beans/baked/heirloom/authentic/historical/one-pot/comfort-food) (american-new-england)
- Wild Maine Blueberry Pancakes (breakfast/vegetarian/authentic/heirloom/comfort-food/quick-and-easy), Cranberry Orange Breakfast Muffins (baking/breakfast/vegetarian/authentic/comfort-food/heirloom) (american-new-england)
- Lobster Rolls with Pickled Fiddleheads and Corn Crema (seafood/modern-fusion/dinner-party/fresh-herbs/quick-and-easy), Pan-Seared Striped Bass with Ramp Butter and Spring Pea Shoots (seafood/grilled/dinner-party/modern-fusion/fresh-herbs/healthy/gluten-free) (american-new-england)

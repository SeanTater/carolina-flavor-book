use std::collections::BTreeMap;

use crate::client::TagEntry;
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

/// Analyze gaps from pre-fetched tag data.
pub fn analyze(
    all_tags: &[TagEntry],
    total_recipes: u64,
    grid: &RecipeGrid,
    filter_cuisine: Option<&str>,
    ignore: &[String],
) -> GapReport {
    // Build recipe->tags map for filtering
    let mut recipe_tags: BTreeMap<i64, Vec<&str>> = BTreeMap::new();
    for entry in all_tags {
        recipe_tags.entry(entry.recipe_id).or_default().push(&entry.tag);
    }

    // If filtering by cuisine, only count recipes that have that cuisine tag
    let filtered_recipe_ids: Option<Vec<i64>> = filter_cuisine.map(|cuisine| {
        recipe_tags.iter()
            .filter(|(_, tags)| tags.contains(&cuisine))
            .map(|(id, _)| *id)
            .collect()
    });

    let effective_total = match &filtered_recipe_ids {
        Some(ids) => ids.len() as u64,
        None => total_recipes,
    };

    let mut axes = BTreeMap::new();

    for (axis_name, axis) in &grid.axes {
        if ignore.iter().any(|i| i == axis_name) {
            continue;
        }

        let mut tags = Vec::new();
        let mut axis_total = 0u64;

        for tag in &axis.tags {
            let count = match &filtered_recipe_ids {
                Some(ids) => {
                    ids.iter()
                        .filter(|id| {
                            recipe_tags.get(id)
                                .map_or(false, |t| t.contains(&tag.as_str()))
                        })
                        .count() as u64
                }
                None => {
                    all_tags.iter().filter(|e| e.tag == *tag).count() as u64
                }
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

    GapReport { total_recipes: effective_total, axes }
}

/// Format the gap report as a human-readable string.
pub fn format_text(report: &GapReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("Total recipes: {}\n\n", report.total_recipes));

    for (axis_name, axis) in &report.axes {
        out.push_str(&format!("{}  ({})\n", axis.display, axis_name));

        for t in &axis.tags {
            out.push_str(&format!("  {}:{}\n", t.tag, t.count));
        }

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

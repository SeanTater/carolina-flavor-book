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

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
pub struct RecipeForUpload {
    pub name: String,
    pub revisions: Vec<RevisionForUpload>,
    pub images: Vec<ImageForUpload>,
    pub tags: Vec<String>,
}

impl std::fmt::Debug for RecipeForUpload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecipeForUpload")
            .field("name", &self.name)
            .field("revisions", &self.revisions)
            .field("images", &self.images.len())
            .field("tags", &self.tags)
            .finish()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RevisionForUpload {
    pub source_name: String,
    pub content_text: String,
    pub format: String,
    pub details: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ImageForUpload {
    pub category: String,
    pub content_bytes: Vec<u8>,
}

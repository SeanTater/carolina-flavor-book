use anyhow::{ensure, Result};
use candle_core::D;
use half::f16;
use model::EmbeddingModel;
use std::sync::RwLock;
use std::{collections::HashMap, sync::Arc, time::Duration};

use itertools::Itertools;

use crate::{
    database::Database,
    models::{Embedding, Recipe, Revision},
};

pub mod model;
const LATEST_MODEL_NAME: &str = "nomic-embed-text-v1.5-truncate-64";
const EMBEDDING_SIZE: usize = 64;

#[derive(Clone)]
pub struct DocumentIndexHandle {
    db: Database,
    embedder: EmbeddingModel,
    index: Arc<RwLock<Arc<DocumentIndex>>>,
}

pub struct DocumentIndex {
    db: Database,
    embedder: EmbeddingModel,
    embeddings: candle_core::Tensor,
    recipe_ids: Vec<i64>,
}

impl DocumentIndex {
    /// Replace the embeddings in the index with new embeddings.
    pub fn with_embeddings(&self, embeddings: Vec<Vec<f16>>, recipe_ids: Vec<i64>) -> Result<Self> {
        ensure!(embeddings.len() == recipe_ids.len());
        ensure!(embeddings.iter().all(|v| v.len() == EMBEDDING_SIZE));
        let embeddings_flat = embeddings.into_iter().flatten().collect_vec();
        Ok(Self {
            db: self.db.clone(),
            embedder: self.embedder.clone(),
            embeddings: candle_core::Tensor::from_vec(
                embeddings_flat,
                (recipe_ids.len(), EMBEDDING_SIZE),
                &candle_core::Device::Cpu,
            )?,
            recipe_ids: recipe_ids,
        })
    }
}

impl DocumentIndexHandle {
    /// A new empty document index.
    pub fn new(db: Database, embedder: EmbeddingModel) -> Self {
        let index = DocumentIndex {
            db: db.clone(),
            embedder: embedder.clone(),
            embeddings: candle_core::Tensor::zeros(
                (0, EMBEDDING_SIZE),
                candle_core::DType::F16,
                &candle_core::Device::Cpu,
            )
            .unwrap(),
            recipe_ids: vec![],
        };
        Self {
            db,
            embedder,
            index: Arc::new(RwLock::new(Arc::new(index))),
        }
    }
    /// Asynchronously performs background indexing of embeddings for recipe revisions.
    ///
    /// This function continuously loops and calls the `background_index_one_batch` function to index embeddings
    /// for recipe revisions. If an error occurs during indexing, it is logged and ignored. A sleep of 5 seconds is
    /// performed to avoid a tight loop in case of errors.
    ///
    /// This function takes ownership of the `DocumentIndexHandle` instance, and it is intended
    /// that you would call .clone() (which is cheap) just before running it in a separate task.
    pub async fn background_index(self) {
        loop {
            let count = Self::background_index_one_batch(self.db.clone(), &self.embedder)
                .await
                .unwrap_or_else(|e| {
                    tracing::error!("Error indexing embeddings, ignoring: {:?}", e);
                    0
                });
            if count == 0 {
                // Sleep for a bit before checking again.
                // This is to avoid a tight loop in case of errors,
                // but it is also a good time to refresh the index.
                self.refresh_index().unwrap_or_else(|e| {
                    tracing::error!("Error refreshing index: {:?}", e);
                });
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            // If the count is not zero, we should loop immediately to process the next batch
        }
    }

    /// Asynchronously performs background indexing of embeddings for a single batch of recipe revisions.
    ///
    /// This function retrieves the latest unindexed revision from the database and indexes its embeddings using
    /// the provided embedding model. If there are no unindexed revisions, the function sleeps for 5 minutes before
    /// checking again. The indexed embeddings are then stored in the database.
    ///
    /// # Arguments
    ///
    /// * `db` - The database connection.
    /// * `model` - The embedding model used for indexing.
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// This function can return an error if there is a problem retrieving revisions from the database or if there
    /// is an error during the embedding process.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use myapp::database::Database;
    /// use myapp::models::EmbeddingModel;
    ///
    /// let db = Database::connect();
    /// let model = EmbeddingModel::new();
    ///
    /// match background_index_one_batch(db, &model).await {
    ///     Ok(()) => println!("Batch indexing completed successfully."),
    ///     Err(e) => eprintln!("Error during batch indexing: {:?}", e),
    /// }
    /// ```
    async fn background_index_one_batch(db: Database, model: &EmbeddingModel) -> Result<usize> {
        let unindexed_revision =
            Revision::get_revisions_without_embeddings(&db, LATEST_MODEL_NAME, 1)?;
        if unindexed_revision.is_empty() {
            // Sleep for a bit before checking again.
            // This is to avoid a tight loop in case of errors,
            // but it is also a good time to refresh the index.
            return Ok(0);
        }
        let revision = &unindexed_revision[0];

        let paragraphs = model::paragraphize(&revision.content_text);
        let paragraph_highlights = paragraphs.iter().map(|p| p.highlight).collect_vec();
        let embeddings = model
            .embed_documents(&paragraph_highlights)?
            .into_iter()
            .zip(paragraphs.iter())
            .map(|(raw_embedding, span)| Embedding {
                embedding_id: rand::random(),
                recipe_id: revision.recipe_id,
                revision_id: revision.revision_id,
                model_name: LATEST_MODEL_NAME.to_string(),
                span_start: span.start as u32,
                span_end: span.end as u32,
                // This quirk is to be in line with SQLite's datetime format
                // Without this, ORDER BY created_on will not work as expected
                created_on: chrono::Local::now().to_rfc3339().replace('T', " "),
                embedding: raw_embedding,
            })
            .collect_vec();
        Embedding::push(&db, &embeddings)?;
        tracing::info!(
            "Indexed {} paragraphs for revision {}",
            embeddings.len(),
            revision.revision_id
        );
        Ok(embeddings.len())
    }

    /// Find the most similar recipes to a query.
    ///
    /// Returns the top `k` most similar recipes to the query, along with their similarity scores.
    pub fn search(&self, query: &str, k: usize) -> Result<Vec<Recipe>> {
        let query = self.embedder.embed_query(query)?;
        // Rather than locking the index for the whole operation, we just clone it once
        let index = self.index.read().unwrap().clone();
        let query =
            candle_core::Tensor::from_vec(query, (1, EMBEDDING_SIZE), &candle_core::Device::Cpu)?;
        // We assume they are already normalized
        let (similarities, match_index) = query
            .matmul(&index.embeddings.transpose(0, 1)?)?
            .squeeze(0)?
            // Hashmap's will overwrite duplicates, so we need to sort by similarity as worst first
            .sort_last_dim(true)?;
        // That got us the best paragraphs of any revision of the recipe. But we want to max pool by recipe
        let best_by_recipe = similarities
            .to_vec1()?
            .into_iter()
            .zip(match_index.to_vec1()?)
            .map(|(sim, ix): (f16, u32)| (index.recipe_ids[ix as usize], sim))
            .collect::<HashMap<i64, f16>>();
        Ok(best_by_recipe
            .into_iter()
            .sorted_by(|a, b| b.1.partial_cmp(&a.1).unwrap())
            .filter_map(|(id, _sim)| Recipe::get_by_id(&self.db, id).ok().flatten())
            .take(k)
            .collect())
    }

    /// Replace the current index with a new index.
    pub fn refresh_index(&self) -> Result<()> {
        if Embedding::count_embeddings(&self.db)? == self.index.read().unwrap().recipe_ids.len() {
            // No new embeddings to index
            return Ok(());
        }

        let embeddings = Embedding::list_all(&self.db)?;
        let recipe_ids = embeddings.iter().map(|e| e.recipe_id).collect_vec();
        let matrix = embeddings
            .into_iter()
            .flat_map(|e| e.embedding)
            .collect_vec();
        let tensor = candle_core::Tensor::from_vec(
            matrix,
            (recipe_ids.len(), EMBEDDING_SIZE),
            &candle_core::Device::Cpu,
        )?;
        let index = DocumentIndex {
            db: self.db.clone(),
            embedder: self.embedder.clone(),
            embeddings: tensor,
            recipe_ids,
        };
        *self.index.write().unwrap() = Arc::new(index);
        Ok(())
    }
}

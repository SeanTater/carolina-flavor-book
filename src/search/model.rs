use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use half::f16;

use fastembed::{InitOptions, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel};
use itertools::Itertools;

#[derive(Clone)]
pub struct EmbeddingModel {
    model: Arc<TextEmbedding>,
}

impl EmbeddingModel {
    pub fn new() -> Result<Self> {
        use std::fs::read;
        let base = PathBuf::from(
            "models/nomic-embed-text-v1.5-q/snapshots/679199c2575b5bfe93b06161d06cd7c16ebe4124",
        );
        let user_model = UserDefinedEmbeddingModel::new(
            read(base.join("onnx/model_quantized.onnx"))?,
            TokenizerFiles {
                tokenizer_config_file: read(base.join("tokenizer_config.json"))?,
                tokenizer_file: read(base.join("tokenizer.json"))?,
                special_tokens_map_file: read(base.join("special_tokens_map.json"))?,
                config_file: read(base.join("config.json"))?,
            },
        );
        Ok(Self {
            model: TextEmbedding::try_new_from_user_defined(
                user_model,
                InitOptions::new(fastembed::EmbeddingModel::NomicEmbedTextV15Q).into(),
            )?
            .into(),
        })
    }

    /// Embed a list of sentences using the model.
    ///
    /// The `prefix` argument is used to prepend a phrase to each sentence before embedding;
    /// many embedding models perform better with a prefix specific to each task.
    pub fn embed<S: AsRef<str>>(&self, sentences: &[S], prefix: &str) -> Result<Vec<Vec<f16>>> {
        let sentences = sentences
            .iter()
            .map(|s| format!("{}: {}", prefix, s.as_ref()))
            .collect_vec();
        Ok(Self::truncate(self.model.embed(sentences, None)?))
    }

    /// Convenience method to embed a single query sentence with the default prefix for queries.
    pub fn embed_documents<S: AsRef<str>>(&self, documents: &[S]) -> Result<Vec<Vec<f16>>> {
        self.embed(
            documents,
            // This specific phrase is meaningful to BAAI/bge-small-en-v1.5
            "search_document",
        )
    }

    /// Convenience method to embed a single query sentence with the default prefix for queries.
    pub fn embed_query(&self, query: &str) -> Result<Vec<f16>> {
        Ok(self
            .embed(
                &[query],
                // This specific phrase is meaningful to BAAI/bge-small-en-v1.5
                "search_query",
            )?
            .pop()
            .unwrap())
    }

    /// Convert a vector of f32 embeddings into a vector of f16 embeddings and truncate it to 64 elements.
    /// This is useful for storing embeddings in a database, to save space in trade for som eloss in accuracy.
    pub fn truncate(embeddings: Vec<Vec<f32>>) -> Vec<Vec<f16>> {
        embeddings
            .into_iter()
            .map(|e| e.into_iter().take(64).map(f16::from_f32).collect())
            .collect()
    }
}

pub fn paragraphize(text: &str) -> Vec<Span<'_>> {
    let sentences = text
        .match_indices(|c| c == '.' || c == '\n')
        .map(|(i, mt)| Span {
            highlight: mt,
            start: i,
            end: i + mt.len(),
        })
        .collect_vec();

    if sentences.len() < 4 {
        // If there are less than 4 sentences, just return the whole text as a single paragraph.
        return vec![Span {
            highlight: text,
            start: 0,
            end: text.len(),
        }];
    }

    let mut paragraphs = vec![
        Span::concat(text, &sentences[0..3]),
        Span::concat(text, &sentences[sentences.len() - 3..]),
    ];
    paragraphs.extend(
        sentences
            .windows(3)
            .map(|window| Span::concat(text, window)),
    );
    paragraphs
}

pub struct Span<'t> {
    pub highlight: &'t str,
    pub start: usize,
    pub end: usize,
}
impl Span<'_> {
    pub fn concat<'t>(original: &'t str, spans: &[Self]) -> Span<'t> {
        let start = spans.iter().map(|s| s.start).min().unwrap_or(0);
        let end = spans.iter().map(|s| s.end).max().unwrap_or(0);
        Span {
            highlight: original.get(start..end).unwrap_or(""),
            start,
            end,
        }
    }
}

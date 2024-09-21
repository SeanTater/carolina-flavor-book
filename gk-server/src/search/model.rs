use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use half::f16;

use fastembed::{InitOptions, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel};
use itertools::Itertools;

use super::EMBEDDING_SIZE;

#[derive(Clone)]
pub struct EmbeddingModel {
    model: Arc<TextEmbedding>,
}

impl EmbeddingModel {
    pub fn new() -> Result<Self> {
        use std::fs::read;
        let base = PathBuf::from("models/snowflake-arctic-embed-xs");
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
                InitOptions::new(fastembed::EmbeddingModel::AllMiniLML6V2Q).into(),
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
            .map(|s| format!("{}{}", prefix, s.as_ref()))
            .collect_vec();
        Ok(Self::truncate(self.model.embed(sentences, None)?))
    }

    /// Convenience method to embed a single query sentence with the default prefix for queries.
    pub fn embed_documents<S: AsRef<str>>(&self, documents: &[S]) -> Result<Vec<Vec<f16>>> {
        self.embed(
            documents, "", // snowflake-arctic-embed-xs does not use a prefix for documents
        )
    }

    /// Convenience method to embed a single query sentence with the default prefix for queries.
    pub fn embed_query(&self, query: &str) -> Result<Vec<f16>> {
        Ok(self
            .embed(
                &[query],
                // This specific phrase is meaningful to snowflake-arctic-embed-xs
                "Represent this sentence for searching relevant passages: ",
            )?
            .pop()
            .unwrap())
    }

    /// Convert a vector of f32 embeddings into a vector of f16 embeddings and truncate it to 64 elements.
    /// This is useful for storing embeddings in a database, to save space in trade for som eloss in accuracy.
    pub fn truncate(embeddings: Vec<Vec<f32>>) -> Vec<Vec<f16>> {
        embeddings
            .into_iter()
            .map(|e| {
                e.into_iter()
                    .take(EMBEDDING_SIZE)
                    .map(f16::from_f32)
                    .collect()
            })
            .collect()
    }
}

pub fn paragraphize(text: &str) -> Vec<Span<'_>> {
    // Split the text into sentences at newlines, but consider multiple newlines as one.
    let splitter = regex::Regex::new(r".*(\n+|$)").unwrap();
    let sentences = splitter
        .find_iter(text)
        .map(|m| Span {
            highlight: m.as_str(),
            start: m.start(),
            end: m.end(),
        })
        .collect_vec();

    if sentences.len() < 5 {
        // If there are less than 5 sentences, just return the whole text as a single paragraph.
        return vec![Span {
            highlight: text,
            start: 0,
            end: text.len(),
        }];
    }

    let mut paragraphs = vec![];
    let mut start = 0;
    let mut end = 0;
    let mut len = 0;
    let TARGET_MAX_LEN = 1000;
    let TARGET_MIN_LEN = 500;
    // We want to get spans of about 1000 characters, but we don't want to split sentences.
    while end < sentences.len() {
        while len > TARGET_MIN_LEN && start < end {
            start += 1;
            len -= sentences[start].end - sentences[start].start;
        }
        while len < TARGET_MAX_LEN && end < sentences.len() {
            len += sentences[end].end - sentences[end].start;
            end += 1;
        }
        paragraphs.push(Span::concat(text, &sentences[start..end]));
    }
    paragraphs
}

pub struct Span<'t> {
    pub highlight: &'t str,
    pub start: usize,
    pub end: usize,
}
impl Span<'_> {
    pub fn concat<'t>(original: &'t str, spans: &[Span]) -> Span<'t> {
        let start = spans.into_iter().map(|s| s.start).min().unwrap_or(0);
        let end = spans.into_iter().map(|s| s.end).max().unwrap_or(0);
        Span {
            highlight: original.get(start..end).unwrap_or(""),
            start,
            end,
        }
    }
}

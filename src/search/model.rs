use candle_transformers::models::bert::{BertModel, Config, HiddenAct, DTYPE};

use anyhow::{Error as E, Result};
use candle_core::Tensor;
use candle_nn::VarBuilder;
use hf_hub::{api::sync::Api, Repo, RepoType};
use tokenizers::{PaddingParams, Tokenizer};

pub struct EmbeddingModel {
    /// The model to use, check out available models: https://huggingface.co/models?library=sentence-transformers&sort=trending
    // #[arg(long)]
    pub model_id: String,

    // #[arg(long)]
    pub revision: String,

    /// Use the pytorch weights rather than the safetensors ones
    // #[arg(long)]
    pub use_pth: bool,

    /// Use tanh based approximation for Gelu instead of erf implementation.
    // #[arg(long, default_value = "false")]
    pub approximate_gelu: bool,
}

impl Default for EmbeddingModel {
    fn default() -> Self {
        Self {
            model_id: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            revision: "main".to_string(),
            use_pth: false,
            approximate_gelu: true,
        }
    }
}

impl EmbeddingModel {
    fn build_model_and_tokenizer(&self) -> Result<(BertModel, Tokenizer)> {
        let device = candle_core::Device::Cpu;

        let repo = Repo::with_revision(
            self.model_id.clone(),
            RepoType::Model,
            self.revision.clone(),
        );
        let (config_filename, tokenizer_filename, weights_filename) = {
            let api = Api::new()?;
            let api = api.repo(repo);
            let config = api.get("config.json")?;
            let tokenizer = api.get("tokenizer.json")?;
            let weights = if self.use_pth {
                api.get("pytorch_model.bin")?
            } else {
                api.get("model.safetensors")?
            };
            (config, tokenizer, weights)
        };
        let config = std::fs::read_to_string(config_filename)?;
        let mut config: Config = serde_json::from_str(&config)?;
        let tokenizer = Tokenizer::from_file(tokenizer_filename).map_err(E::msg)?;

        let vb = if self.use_pth {
            VarBuilder::from_pth(&weights_filename, DTYPE, &device)?
        } else {
            unsafe { VarBuilder::from_mmaped_safetensors(&[weights_filename], DTYPE, &device)? }
        };
        if self.approximate_gelu {
            config.hidden_act = HiddenAct::GeluApproximate;
        }
        let model = BertModel::load(vb, &config)?;
        Ok((model, tokenizer))
    }

    pub fn run(&self, sentences: &[String]) -> Result<Vec<Vec<f32>>> {
        let (model, mut tokenizer) = self.build_model_and_tokenizer()?;
        let device = &model.device;

        // let sentences = [
        //     "The cat sits outside",
        //     "A man is playing guitar",
        //     "I love pasta",
        //     "The new movie is awesome",
        //     "The cat plays in the garden",
        //     "A woman watches TV",
        //     "The new movie is so great",
        //     "Do you like pizza?",
        // ];
        if let Some(pp) = tokenizer.get_padding_mut() {
            pp.strategy = tokenizers::PaddingStrategy::BatchLongest
        } else {
            let pp = PaddingParams {
                strategy: tokenizers::PaddingStrategy::BatchLongest,
                ..Default::default()
            };
            tokenizer.with_padding(Some(pp));
        }
        let tokens = tokenizer
            .encode_batch(sentences.to_vec(), true)
            .map_err(E::msg)?;
        let token_ids = tokens
            .iter()
            .map(|tokens| {
                let tokens = tokens.get_ids().to_vec();
                Ok(Tensor::new(tokens.as_slice(), device)?)
            })
            .collect::<Result<Vec<_>>>()?;

        let token_ids = Tensor::stack(&token_ids, 0)?;
        let token_type_ids = token_ids.zeros_like()?;
        println!("running inference on batch {:?}", token_ids.shape());
        let embeddings = model.forward(&token_ids, &token_type_ids)?;
        println!("generated embeddings {:?}", embeddings.shape());
        // Apply some avg-pooling by taking the mean embedding value for all tokens (including padding)
        let (_n_sentence, n_tokens, _hidden_size) = embeddings.dims3()?;
        let embeddings = (embeddings.sum(1)? / (n_tokens as f64))?;
        let embeddings = normalize_l2(&embeddings)?;
        println!("pooled embeddings {:?}", embeddings.shape());
        Ok(embeddings.to_vec2()?)

        // let n_sentences = sentences.len();
        // let mut similarities = vec![];
        // for i in 0..n_sentences {
        //     let e_i = embeddings.get(i)?;
        //     for j in (i + 1)..n_sentences {
        //         let e_j = embeddings.get(j)?;
        //         let sum_ij = (&e_i * &e_j)?.sum_all()?.to_scalar::<f32>()?;
        //         let sum_i2 = (&e_i * &e_i)?.sum_all()?.to_scalar::<f32>()?;
        //         let sum_j2 = (&e_j * &e_j)?.sum_all()?.to_scalar::<f32>()?;
        //         let cosine_similarity = sum_ij / (sum_i2 * sum_j2).sqrt();
        //         similarities.push((cosine_similarity, i, j))
        //     }
        // }
        // similarities.sort_by(|u, v| v.0.total_cmp(&u.0));
        // for &(score, i, j) in similarities[..5].iter() {
        //     println!("score: {score:.2} '{}' '{}'", sentences[i], sentences[j])
        // }
    }
}

fn normalize_l2(v: &Tensor) -> Result<Tensor> {
    Ok(v.broadcast_div(&v.sqr()?.sum_keepdim(1)?.sqrt()?)?)
}

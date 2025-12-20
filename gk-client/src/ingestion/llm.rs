use anyhow::{anyhow, Result};
use async_openai::{
    config::OpenAIConfig, types::ChatCompletionRequestMessage,
    types::ChatCompletionRequestUserMessage, types::CreateChatCompletionRequestArgs,
};

pub struct LlmConfig {
    pub provider: String,
    pub model: Option<String>,
    pub ollama_base_url: String,
}

impl LlmConfig {
    pub fn get_model(&self) -> String {
        if let Some(model) = &self.model {
            return model.clone();
        }
        match self.provider.as_str() {
            "ollama" => "llama3.1".to_string(),
            "openai" => "gpt-4o-mini".to_string(),
            _ => "gpt-4o-mini".to_string(),
        }
    }

    pub fn create_client(&self) -> Result<async_openai::Client<OpenAIConfig>> {
        let config = match self.provider.as_str() {
            "ollama" => {
                tracing::info!("Using Ollama at {}", self.ollama_base_url);
                OpenAIConfig::new()
                    .with_api_base(&self.ollama_base_url)
                    .with_api_key("ollama") // Ollama doesn't need real key but library requires one
            }
            "openai" => {
                let api_key = dotenvy::var("OPENAI_API_KEY")
                    .expect("OPENAI_API_KEY required when LLM_PROVIDER=openai");
                tracing::info!("Using OpenAI");
                OpenAIConfig::new().with_api_key(api_key)
            }
            _ => anyhow::bail!("LLM_PROVIDER must be 'openai' or 'ollama', got: {}", self.provider),
        };
        Ok(async_openai::Client::build(Default::default(), config, Default::default()))
    }
}

async fn call_llm(config: &LlmConfig, prompt: &str) -> Result<String> {
    let client = config.create_client()?;
    let model = config.get_model();
    let req_args = CreateChatCompletionRequestArgs::default()
        .model(&model)
        .messages([ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessage {
                content: prompt.into(),
                name: None,
            },
        )])
        .build()?;
    let text = client
        .chat()
        .create(req_args)
        .await?
        .choices
        .first()
        .ok_or(anyhow!("No response from LLM"))?
        .clone()
        .message
        .content
        .ok_or(anyhow!("No response from LLM"))?;
    Ok(text)
}

pub async fn improve_recipe_with_llm(config: &LlmConfig, content_text: &str) -> Result<String> {
    tracing::info!("Improving recipe ..");
    let prompt_template = include_str!("../prompts/cleanup-ocr.md");
    let prompt = prompt_template.replace("{content}", content_text);
    tracing::debug!("Prompt: {}", prompt);
    call_llm(config, &prompt).await
}

pub async fn freestyle(config: &LlmConfig, recipe_name: &str) -> Result<String> {
    tracing::info!("Creating a new recipe ..");
    let prompt_template = include_str!("../prompts/freestyle.md");
    let prompt = prompt_template.replace("{name}", recipe_name);
    tracing::debug!("Prompt: {}", prompt);
    call_llm(config, &prompt).await
}

pub async fn generate_recipe_scene(config: &LlmConfig, best_recipe_text: &str) -> Result<String> {
    let prompt_template = include_str!("../prompts/generate-scenes.md");
    let prompt = prompt_template.replace("{content}", best_recipe_text);
    tracing::debug!("Prompt: {}", prompt);
    call_llm(config, &prompt).await
}

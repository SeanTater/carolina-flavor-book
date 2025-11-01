use anyhow::anyhow;
use anyhow::Result;
use async_openai::{
    config::OpenAIConfig, types::ChatCompletionRequestMessage,
    types::ChatCompletionRequestUserMessage, types::CreateChatCompletionRequestArgs,
};

lazy_static::lazy_static! {
    pub static ref OpenAIClient: async_openai::Client<OpenAIConfig> = async_openai::Client::build(
        Default::default(),
        OpenAIConfig::new()
            .with_api_key(
                dotenvy::var("OPENAI_API_KEY")
                .expect("Could not find OPENAI_API_KEY in the environment.")
            ),
        Default::default());
}

/// Calls the LLM one-shot API with a given prompt.
///
/// This function takes a string `prompt` and calls the LLM model using the
/// OpenAI configuration. The result is returned as a string.
///
/// # Arguments
///
/// * `prompt`: A string to be used as input for the LLM model.
///
/// # Returns
///
/// A `Result` containing the response from the LLM model, or an error if the call fails.
pub async fn call_llm(prompt: &str) -> Result<String> {
    let req_args = CreateChatCompletionRequestArgs::default()
        .model("gpt-4o-mini")
        .messages([ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessage {
                content: prompt.into(),
                name: None,
            },
        )])
        .build()?;
    let text = OpenAIClient
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

/// Calls the LLM to improve a recipe with the default prompt
pub async fn improve_recipe_with_llm(content_text: &str) -> Result<String> {
    tracing::info!("Improving recipe ..");
    let prompt_template = include_str!("../prompts/cleanup-ocr.md");
    let prompt = prompt_template.replace("{content}", content_text);
    tracing::debug!("Prompt: {}", prompt);
    call_llm(&prompt).await
}

/// Calls the LLM to improve a recipe with the default prompt
pub async fn freestyle(recipe_name: &str) -> Result<String> {
    tracing::info!("Creating a new recipe ..");
    let prompt_template = include_str!("../prompts/freestyle.md");
    let prompt = prompt_template.replace("{name}", recipe_name);
    tracing::debug!("Prompt: {}", prompt);
    call_llm(&prompt).await
}

/// Calls an LLM to depict the scene of a recipe after it's done (for image generation)
pub async fn generate_recipe_scene(best_recipe_text: &str) -> Result<String> {
    let prompt_template = include_str!("../prompts/generate-scenes.md");
    let prompt = prompt_template.replace("{content}", best_recipe_text);
    tracing::debug!("Prompt: {}", prompt);
    call_llm(&prompt).await
}

use anyhow::anyhow;
use anyhow::Result;
use async_openai::{
    config::OpenAIConfig, types::ChatCompletionRequestMessage,
    types::ChatCompletionRequestUserMessage, types::CreateChatCompletionRequestArgs,
};
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
pub async fn call_llm(prompt: &str, api_base: Option<&str>) -> Result<String> {
    let config = OpenAIConfig::new().with_api_base(api_base.unwrap_or("http://localhost:11434/v1"));
    let client = async_openai::Client::with_config(config);
    let req_args = CreateChatCompletionRequestArgs::default()
        .model("llama3.1")
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

/// Calls the LLM to improve a recipe with the default prompt
pub async fn improve_recipe_with_llm(content_text: &str, api_base: Option<&str>) -> Result<String> {
    tracing::info!("Improving recipe ..");
    let prompt_template = include_str!("../prompts/cleanup-ocr.md");
    call_llm(
        &prompt_template.replace("{content}", content_text),
        api_base,
    )
    .await
}

/// Calls the LLM to improve a recipe with the default prompt
pub async fn freestyle(recipe_name: &str, api_base: Option<&str>) -> Result<String> {
    tracing::info!("Creating a new recipe ..");
    let prompt_template = include_str!("../prompts/freestyle.md");
    call_llm(&prompt_template.replace("{name}", recipe_name), api_base).await
}

use anyhow::Result;
use base64::Engine;
use image::DynamicImage;

use anyhow::anyhow;
use async_openai::types::{
    ChatCompletionRequestMessageContentPartImageArgs,
    ChatCompletionRequestMessageContentPartTextArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs, ImageDetail, ImageUrlArgs,
};

use super::convert_to_webp;
use super::llm::LlmConfig;

/// Convert a webp image to a data URL
///
/// This is duplicated between the client and server, but they are a little different.
/// Thie one operates on borrowed slices because it makes more sense than options needed for minijinja filters.
fn to_data_url(bytes: &[u8]) -> String {
    format!(
        "data:image/webp;base64,{}",
        // For the purpose of data urls, you do NOT need to use the URL_SAFE variant
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

pub async fn read_text_from_image(config: &LlmConfig, img: &DynamicImage) -> Result<String> {
    // This API supports DataURI images. So first we need to make this into WebP format, then base64, then wrap in a DataURI.
    if (img.height() * img.width()) > 2 << 20 {
        tracing::warn!(
            "Image is probably larger than it needs to be. ({h}x{w}) Consider resizing.",
            h = img.height(),
            w = img.width()
        );
    }
    let img_webp = convert_to_webp(img, 0.8)?;
    let img_data_url = to_data_url(&img_webp);

    let client = config.create_client()?;
    let model = config.get_model();

    let request = CreateChatCompletionRequestArgs::default()
        .model(&model)
        .messages([ChatCompletionRequestUserMessageArgs::default()
            .content(vec![
                ChatCompletionRequestMessageContentPartTextArgs::default()
                    .text("Read the text from this image.")
                    .build()?
                    .into(),
                ChatCompletionRequestMessageContentPartImageArgs::default()
                    .image_url(
                        ImageUrlArgs::default()
                            .url(img_data_url)
                            .detail(ImageDetail::High)
                            .build()?,
                    )
                    .build()?
                    .into(),
            ])
            .build()?
            .into()])
        .build()?;
    client
        .chat()
        .create(request)
        .await?
        .choices[0]
        .message
        .content
        .take()
        .ok_or(anyhow!("No message content when requesting OCR."))
}

use anyhow::{anyhow, ensure, Result};
use image::DynamicImage;
use tokio::process::Command;
pub mod illustrate;
pub mod llm;
mod ocr;
mod webcam;

pub use illustrate::illustrate_recipe;
pub use llm::call_llm;
pub use llm::freestyle;
pub use llm::improve_recipe_with_llm;
pub use ocr::read_text_from_image;
pub use webcam::take_picture;

/// Transcribes spoken audio from a file.
///
/// This function uses the `whisper` command-line tool to transcribe speech
/// from an input audio file. The transcription is returned as a string.
///
/// # Arguments
///
/// * None
///
/// # Returns
///
/// A `Result` containing the transcription as a string, or an error if transcription fails.
pub async fn take_dictation() -> Result<String> {
    // TODO: This should use models directly rather than calling the command-line tool.
    let temp_dir = tempfile::tempdir()?;
    let temp_wav = temp_dir.path().join("speech.wav");
    let status = Command::new("rec")
        .arg(&temp_wav)
        .args("rate 16k silence 1 0.1 3% 1 3.0 3%".split_whitespace())
        .status()
        .await?;
    ensure!(status.success(), "Dictation recording failed.");

    let text = Command::new("whisper")
        .args(["-nt", "-m", "models/ggml-small.bin"])
        .arg(temp_wav)
        .output()
        .await?
        .stdout;

    Ok(String::from_utf8_lossy(&text).into_owned())
}

/// Save a DynamicImage as a webp vector.
///
/// This is useful because the image crate only supports lossless webp,
/// but the webp crate supports lossy webp.
pub fn convert_to_webp(img: &DynamicImage, quality: f32) -> Result<Vec<u8>> {
    let img_webp = webp::Encoder::from_image(img)
        .map_err(|st| anyhow!("Webp encoder error: {}", st))?
        .encode(quality);
    Ok(img_webp.to_vec())
}

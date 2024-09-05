use std::ops::Range;
use std::path::{Path, PathBuf};

use anyhow::anyhow;
use anyhow::ensure;
use anyhow::Result;
use async_openai::config::OpenAIConfig;
use async_openai::types::ChatCompletionRequestMessage;
use async_openai::types::ChatCompletionRequestUserMessage;
use async_openai::types::CreateChatCompletionRequestArgs;
use glob::glob;
use image::DynamicImage;
use image::GenericImageView;
use imageproc::contrast::ThresholdType;
use imageproc::distance_transform::Norm;
use lazy_static::lazy_static;
use ocrs::ImageSource;
use tokio::process::Command;
use tokio_retry::Retry;

fn default_retry() -> impl Iterator<Item = std::time::Duration> {
    tokio_retry::strategy::ExponentialBackoff::from_millis(100)
        .map(tokio_retry::strategy::jitter)
        .take(3)
}

lazy_static! {
    static ref OCR_ENGINE: ocrs::OcrEngine =
        ocrs::OcrEngine::new(Default::default()).expect("Unable to load OCR engine");
}

// #!/usr/bin/env python3
// from subprocess import check_call, check_output, SubprocessError
// from pathlib import Path
// from tempfile import NamedTemporaryFile
// from urllib.parse import quote as url_escape
// import numpy as np
// import cv2
// import os
// import subprocess

/// Scans images and returns a list of `PathBuf` instances.
///
/// This function attempts to scan a batch of images using the `scanimage` command,
/// which will be interactiev. Scanimage typically fails at least once to connect to
/// remote scanners, so this will retry the batch up to three times.
///
/// # Arguments
///
/// * `description`: A string used as part of the batch prompt and in file names.
///
/// # Returns
///
/// A list of `PathBuf` instances representing the scanned images, or an error if scanning fails.
async fn scan_images(description: &str) -> Result<Vec<PathBuf>> {
    Retry::spawn(default_retry(), || async {
        let status = Command::new("scanimage")
            .args(["-d", "airscan:e0:BigFatInkTank"])
            .args(["--format", "png"])
            .args(["--mode", "color"])
            .args(["--resolution", "300"])
            .arg("--batch-prompt")
            .arg(format!("--batch=scratch/{}-%d.png", description))
            .status()
            .await?;
        ensure!(status.success(), "Failed to execute Scanimage");
        anyhow::Ok(())
    })
    .await?;

    let images_by_glob = glob(&format!("scratch/{}-*.png", description))?
        .collect::<std::result::Result<_, _>>()
        .map_err(anyhow::Error::from)?;
    Ok(images_by_glob)
}

/// Performs OCR on a single image.
///
/// This function takes a `DynamicImage` instance, converts it to RGB8 format,
/// prepares an input for the OCR engine, and then uses the engine to extract text
/// from the image. The extracted text is returned as a string.
///
/// # Arguments
///
/// * `img`: A `DynamicImage` instance representing the image to perform OCR on.
///
/// # Returns
///
/// A `Result` containing the extracted text as a string, or an error if OCR fails.
pub fn ocr_one_image(img: DynamicImage) -> Result<String> {
    let img = img.into_rgb8();
    let src = ImageSource::from_bytes(img.as_raw(), img.dimensions())?;
    let ocr_input = OCR_ENGINE.prepare_input(src)?;
    OCR_ENGINE.get_text(&ocr_input)
}

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

pub async fn ocr_images(
    name: &str,
    image_list: &[PathBuf],
    category_path: &Path,
) -> Result<String> {
    let mut ocr_text = vec![];
    let mut markdown_images = vec![];

    for (i, old_png) in image_list.iter().enumerate() {
        let image_mat = image::open(old_png)?;

        let new_webp = category_path
            .join("images")
            .join(old_png.file_name().unwrap())
            .with_extension("webp");

        // The image crate only supports image-webp (a rust reimplementation)
        // But we want lossy compression, which requires the original implementation for now
        // And the webp crate is a binding to it. Luckily, that's super easy!
        let webp_bytes = webp::Encoder::from_image(&image_mat)
            .map_err(|x| anyhow!("Webp encoder failure: {}", x))?
            .encode(75.0);
        std::fs::write(&new_webp, &webp_bytes[..])?;

        ocr_text.push(ocr_one_image(image_mat)?);
        markdown_images.push(format!(
            "![Recipe scan {}](images/{})",
            i + 1,
            url_escape::encode_path(&new_webp.file_name().unwrap().to_string_lossy())
        ));
    }

    let mut ocr_text = ocr_text.join("\n\n");
    print!("{}\n\nIs the OCR corrupted? [Y/n]>> ", ocr_text);
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    if buf.to_lowercase().starts_with('y') || buf.trim().is_empty() {
        ocr_text = take_dictation().await?;
    }
    ocr_text = call_llm_one_shot(
        &include_str!("prompts/cleanup-ocr.md")
            .replace("{content}", &ocr_text)
            .replace("{name}", name),
    )
    .await?;

    Ok("".into())
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
pub async fn call_llm_one_shot(prompt: &str) -> Result<String> {
    let config = OpenAIConfig::new().with_api_base("http://localhost:11434/v1");
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
        .choices.first()
        .ok_or(anyhow!("No response from LLM"))?
        .clone()
        .message
        .content
        .ok_or(anyhow!("No response from LLM"))?;
    Ok(text)
}

type NDImageViewMut<'t> = ndarray::ArrayViewMut3<'t, u8>;

pub fn map_image_as_ndarray<F: FnOnce(ndarray::ArrayViewMut3<u8>) -> Result<()>>(
    img: &mut image::DynamicImage,
    mapper: F,
) -> Result<()> {
    let (height, width) = img.dimensions();
    let depth = img.color().channel_count();
    let bytes = match img.color() {
        image::ColorType::L8 => img.as_mut_luma8().unwrap().as_flat_samples_mut().samples,
        image::ColorType::Rgb8 => img.as_mut_rgb8().unwrap().as_flat_samples_mut().samples,
        _ => unimplemented!(),
    };

    let ndarr = ndarray::ArrayViewMut3::from_shape(
        [height as usize, width as usize, depth as usize],
        bytes,
    )?;
    mapper(ndarr)?;
    Ok(())
}

fn stretch_brights(mut img_u8: ndarray::ArrayViewMut3<u8>, power: f32) -> Result<()> {
    let mut img = img_u8.mapv(|v| v as f32);
    img.mapv_inplace(|v| (255.0 - v).powf(power));
    // This is ugly because Rust wants you to handle NaN, but we can't have them
    let white_point = *img.iter().max_by(|x, y| x.partial_cmp(y).unwrap()).unwrap();
    img.mapv_inplace(|v| {
        let rescaled = v * (255.0 / white_point);
        (255.0 - rescaled).clamp(0.0, 255.0)
    });
    img_u8.assign(&img.mapv(|v| v as u8));
    Ok(())
}

/// Remove the white scan area outside a recipe, using an advanced image processing algorithm.
///
/// The algorithm works by:
/// 1. Applying a brightness stretch to emphasize the bright end of the scale.
/// 2. Removing specks using morphological operations.
/// 3. Thresholding the image using Otsu's method to get binary markers for the foreground and background.
/// 4. Dilating the foreground marker to be conservative about what is considered foreground.
/// 5. Eroding the background marker to be conservative about what is considered background.
/// 6. Using the Watershed algorithm with the foreground and background markers as inputs, to segment the image into foreground and background regions.
/// 7. Finding the largest contour in the foreground region, which represents the black area (foreground) of the recipe.
///
/// The cropped image is then returned, with the white scan area outside the recipe removed.
///
/// Args:
///     color (np.ndarray): A 3D NumPy array representing the input image.
///
/// Returns:
///   np.ndarray: The cropped image.
///
///
pub fn auto_crop_scan(color: image::DynamicImage) -> Result<image::DynamicImage> {
    let (ow, oh) = color.dimensions();
    //let color_nd: ColorND = image_to_ndarray(&color)?.to_shared();

    let mut pipe = DynamicImage::from(color.into_luma8());
    for _ in 0..2 {
        map_image_as_ndarray(&mut pipe, |im| stretch_brights(im, 0.75))?;
        pipe = image::imageops::blur(&pipe, 9.0).into();
        imageproc::morphology::close_mut(pipe.as_mut_luma8().unwrap(), Norm::L1, 9);
    }
    let otsu_threshold = imageproc::contrast::otsu_level(pipe.as_luma8().unwrap());
    imageproc::contrast::threshold_mut(
        pipe.as_mut_luma8().unwrap(),
        otsu_threshold,
        ThresholdType::Binary,
    );

    let mut conservative_foreground =
        imageproc::morphology::dilate(pipe.as_luma8().unwrap(), Norm::L1, 9);
    image::imageops::invert(&mut conservative_foreground);

    let conservative_background =
        imageproc::morphology::erode(pipe.as_luma8().unwrap(), Norm::L1, 9);

    let cross = |color: usize, y_range: Range<_>, x_range: Range<_>| {
        y_range.flat_map(move |y| {
            x_range
                .clone()
                .map(move |x| (color, (y as usize, x as usize)))
        })
    };

    let mut markers = vec![];
    // Include a square at the top left
    markers.extend(cross(1, 20..(oh / 10), 20..(ow / 20)));
    markers.extend(
        conservative_foreground
            .enumerate_pixels()
            .filter(|(_, _, p)| p.0[0] > 0)
            .map(|(x, y, _)| (1, (y as usize, x as usize))),
    );

    // Include two rectangles at the bottom and right
    markers.extend(cross(2, (oh / 9 * 10)..oh, 0..ow));
    markers.extend(cross(2, 0..oh, (ow / 9 * 10)..ow));
    markers.extend(
        conservative_background
            .enumerate_pixels()
            .filter(|(_, _, p)| p.0[0] > 0)
            .map(|(x, y, _)| (2, (y as usize, x as usize))),
    );

    // let segmenting = rustronomy_watershed::TransformBuilder::new()
    //     .set_wlvl_hook(|wlvl|)
    //     .enable_edge_correction()
    //     .build_segmenting()
    //     .unwrap();

    // let mut segments = image::GrayImage::new(1, 1);
    // let segmented = map_image_as_ndarray(&mut pipe, |img_view| {
    //     segments = segmenting.transform_with_hook_and_colours(
    //         img_view.remove_axis(ndarray::Axis(2)).view(),
    //         &markers[..],
    //     );
    //     Ok(())
    // })?;
    todo!();
    Ok(image::DynamicImage::new_rgb8(1, 1))
}

// def auto_crop_scan(color: np.ndarray) -> np.ndarray:
//     """
//
//     """
//     oh, ow = color.shape[:2]

//     # Emphasize the bright end of the scale, but don't threshold
//     def stretch_brights(img: np.ndarray, power: float) -> np.ndarray:
//         dark = 255 - img.astype(np.float16)
//         dark = dark**power
//         dark = dark * (255 / np.max(dark))
//         return (255 - dark).clip(0, 255).astype(np.uint8)

//     # Now try to remove specks using morphological operations
//     kernel = np.ones((9, 9), dtype=np.uint8)
//     pipe = color
//     for _ in range(2):
//         pipe = stretch_brights(color, 0.75)
//         pipe = cv2.GaussianBlur(pipe, (9, 9), 0)
//         pipe = cv2.morphologyEx(pipe, cv2.MORPH_CLOSE, kernel, iterations=4)

//     # Now also threshold the pipe using Otsu's method so we can get some markers for watershed
//     # This will be our markers for the watershed algorithm
//     marker_pipe = cv2.cvtColor(pipe, cv2.COLOR_BGR2GRAY)
//     _, marker_pipe = cv2.threshold(
//         marker_pipe, 0, 255, cv2.THRESH_BINARY + cv2.THRESH_OTSU
//     )
//     # Dilate it a lot so we can be conservative about what is a foreground
//     conservative_foreground = cv2.dilate(marker_pipe, kernel, iterations=4) == 0
//     # Erode it so we can be conservative about what is background
//     conservative_background = cv2.erode(marker_pipe, kernel, iterations=4) == 255

//     watershed_input = pipe

//     # Use Otsu's thresholding to get a binary image
//     # ret, thresh = cv2.threshold(inverted, 0, 255, cv2.THRESH_OTSU)
//     # Apply the Watershed algorithm to find the black area (foreground)
//     # and the black area (foreground), then find the markers for each
//     markers = np.zeros((oh, ow, 1), dtype=np.int32)
//     # The top left corner is always the foreground
//     markers[20 : oh // 10, 20 : ow // 10] = 1
//     markers[conservative_foreground] = 1
//     # The bottom and right edges are always the background
//     markers[-(oh // 10) :] = 2
//     markers[:, -(ow // 10) :] = 2
//     markers[conservative_background] = 2

//     # Perform the Watershed algorithm to segment the image
//     cv2.watershed(watershed_input, markers)

//     # The result of the watershed algorithm is stored in markers
//     # We can now find the largest contour which represents the black area (foreground)
//     contours, _ = cv2.findContours(
//         (markers == 1).astype(np.uint8),
//         cv2.RETR_EXTERNAL,
//         cv2.CHAIN_APPROX_SIMPLE,
//     )
//     largest_contour = max(contours, key=cv2.contourArea)

//     x, y, w, h = cv2.boundingRect(largest_contour)
//     cropped_image = color[y : y + h, x : x + w]
//     return cropped_image

// def test_auto_crop_scan():
//     image_names = ["Banana Split Cake-1", "Apricot Nectar Cake-1"]
//     for image_name in image_names:
//         source_image = cv2.imread(f"src/tests/{image_name}.webp")
//         reference_cropped_image = cv2.imread(f"src/tests/{image_name} cropped.webp")
//         cropped_image = auto_crop_scan(source_image)
//         # Save the cropped image for visual inspection
//         cv2.imwrite(f"src/tests/{image_name} cropped test.webp", cropped_image)
//         # This isn't expected to be exactly the same, but should be close enough for visual inspection
//         # 1. Be within a few pixels of the same dimensions
//         assert (
//             abs(cropped_image.shape[0] - reference_cropped_image.shape[0]) < 25
//         ), f"Height difference too large: expected {reference_cropped_image.shape}, got {cropped_image.shape}"
//         assert (
//             abs(cropped_image.shape[1] - reference_cropped_image.shape[1]) < 25
//         ), f"Width difference too large: expected {reference_cropped_image.shape}, got {cropped_image.shape}"

// if __name__ == "__main__":
//     description = input("Recipe name: ")
//     description_safe = url_escape(description)
//     category = input("Category: ")
//     category_path = Path("book/src") / category
//     category_path.mkdir(parents=True, exist_ok=True)

//     image_list = scan_images(description)
//     for image_path in image_list:
//         source_image = cv2.imread(image_path.as_posix())
//         cropped_image = auto_crop_scan(source_image)
//         cv2.imwrite(image_path.as_posix(), cropped_image)

//     ocr_markdown = ocr_images(description, image_list, category_path)

//     destination = category_path / f"{description}.md"
//     destination.write_text(ocr_markdown)
//     print(f"Saved result to {destination.as_posix()}")

//     Path("book/src/SUMMARY.md").open("at").write(
//         f"- [{description}](./{category}/{description_safe}.md)\n"
//     )

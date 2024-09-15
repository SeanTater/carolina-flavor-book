use anyhow::Result;
use image::DynamicImage;
use ocrs::ImageSource;

lazy_static::lazy_static! {
    static ref OCR_ENGINE: ocrs::OcrEngine = default_ocr_engine();
}

/// Load the default OCR engine. The models for this engine are embedded in the binary,
/// so this function does not require any arguments or IO.
fn default_ocr_engine() -> ocrs::OcrEngine {
    let detector = include_bytes!("../../../models/text-detection.rten");
    let recognizer = include_bytes!("../../../models/text-recognition.rten");
    let detection_model =
        rten::Model::load_static_slice(detector).expect("Failed to load detector model");
    let recognition_model =
        rten::Model::load_static_slice(recognizer).expect("Failed to load recognizer model");
    let engine_params = ocrs::OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    };
    ocrs::OcrEngine::new(engine_params).expect("Unable to load OCR engine")
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
pub fn read_text_from_image(img: &DynamicImage) -> Result<String> {
    let img = img.clone().into_rgb8();
    let src = ImageSource::from_bytes(img.as_raw(), img.dimensions())?;
    let ocr_input = OCR_ENGINE.prepare_input(src)?;
    OCR_ENGINE.get_text(&ocr_input)
}

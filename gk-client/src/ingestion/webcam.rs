use anyhow::anyhow;
use anyhow::Result;
use image::DynamicImage;
use itertools::Itertools;
use tokio::time;
use v4l::format::quantization;
use v4l::format::Flags;
use v4l::frameinterval::{FrameIntervalEnum, Stepwise};
use v4l::io::traits::CaptureStream;
use v4l::prelude::MmapStream;
use v4l::video::Capture;
use v4l::FourCC;
use v4l::{Device, Fraction, FrameInterval};

/// Captures an image from a webcam using v4l2 (a rust library)
///
/// This function captures an image from a webcam using the v4l2 library and returns the image.
/// The image is captured in the highest resolution supported by the webcam.
pub fn take_picture(device_number: usize) -> Result<DynamicImage> {
    let mut device = Device::new(device_number)?;

    device.set_format(&v4l::Format {
        width: 1920,
        height: 1080,
        fourcc: FourCC::new(b"MJPG"),
        field_order: v4l::format::FieldOrder::Progressive,
        stride: 0,
        size: 5 << 20,
        flags: Flags::empty(),
        colorspace: v4l::format::Colorspace::JPEG,
        quantization: quantization::Quantization::Default,
        transfer: v4l::format::TransferFunction::Default,
    })?;

    let mut stream = MmapStream::with_buffers(&mut device, v4l::buffer::Type::VideoCapture, 3)
        .expect("Failed to create buffer stream");

    let wait_until = time::Instant::now() + time::Duration::from_secs(1);
    let mut buffer = vec![];
    while let Ok((frame, _meta)) = stream.next() {
        // Typically it can take a second or two for the camera to adjust white balance and exposure
        if time::Instant::now() > wait_until {
            buffer = frame.to_vec();
            break;
        }
    }
    let img = image::load_from_memory_with_format(&buffer, image::ImageFormat::Jpeg)?;
    Ok(img)
}

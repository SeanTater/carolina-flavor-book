pub fn mat_to_ndarray(in_mat: Mat) -> ColorND {
    let shape = in_mat.mat_size();
    let shape = [
        shape[0] as usize,
        shape[1] as usize,
        in_mat.channels() as usize,
    ];
    let len = shape[0] * shape[1] * shape[2];
    let ptr = in_mat
        .ptr(0)
        .expect("Couldn't get pointer to beginning of OpenCV image");
    assert!(shape[2] == 3, "Expected OpenCV image to be RGB");
    assert!(in_mat.is_continuous(), "The OpenCV image is not continuous");
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };

    let array = ndarray::ArrayView3::from_shape(shape, slice)
        .expect("Could not read OpenCV Mat into ndarray");
    array.mapv(|v| v as f32).to_shared()
}

pub fn ndarray_to_mat(in_arr: ColorND) -> Mat {
    let (height, width, depth) = in_arr.dim();
    assert_eq!(depth, 3);
    Mat::new_nd_with_data(
        &[height as i32, width as i32],
        in_arr
            .mapv(|f| f as u8)
            .as_standard_layout()
            .as_slice()
            .expect("ColorND should have been in standard layout"),
    )
    .expect("Could not create Mat fron Ndarray")
    .try_clone()
    .expect("Could not clone OpenCV Mat")
}

/// Converts an image into a mutable numpy array.
///
/// This function takes an `image::RgbImage` and returns a mutable view into a 3D numpy array, where each pixel is represented as an unsigned byte (u8).
///
/// # Arguments
///
/// * `img`: The input image to be converted.
///
/// # Returns
///
/// A `Result` containing the mutable numpy array, or an error if the conversion fails.
pub fn image_to_ndarray(img: &image::RgbImage) -> Result<ColorND> {
    let (height, width) = img.dimensions();
    Ok(ndarray::ArrayView3::from_shape(
        [height as usize, width as usize, 3],
        img.as_flat_samples().samples,
    )?
    .mapv(|b| f32::from(b))
    .to_shared())
}

/// Converts a mutable numpy array into an image.
///
/// This function takes a mutable view into a 3D numpy array and returns an `image::RgbImage`.
///
/// # Arguments
///
/// * `arr`: The input array to be converted.
///
/// # Returns
///
/// A `Result` containing the output image, or an error if the conversion fails.
pub fn ndarray_to_image(arr: ColorND) -> Result<image::RgbImage> {
    let (height, width, depth) = arr.dim();
    ensure!(depth == 3, "The array should have two dimensions plus RGB");
    let bytes = arr
        .mapv(|v| v as u8)
        .as_standard_layout()
        .as_slice()
        .ok_or(anyhow!("Failed to get ndarray as a 3d slice"))?
        .to_vec();
    image::RgbImage::from_raw(width as u32, height as u32, bytes)
        .ok_or(anyhow!("Failed to convert image data from bytes to image"))
}

type ColorND = ndarray::ArrayBase<ndarray::OwnedArcRepr<f32>, ndarray::Dim<[usize; 3]>>;

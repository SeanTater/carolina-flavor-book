#!/usr/bin/env python3
from subprocess import check_call, check_output, SubprocessError
from pathlib import Path
from urllib.parse import quote as url_escape
import numpy as np
import cv2
import os


def scan_images(description: str) -> list[Path]:
    """
    Scan images using the 'scanimage' command and save them in a batch.

    Args:
        description (str): A string to be used in the filename of each saved image.
            The format will be '{description}-%d.png', where '%d' is an integer
            incremented for each image scanned.

    Returns:
        list[Path]: A sorted list of Path objects, representing the saved images.

    Raises:
        SubprocessError: If all three attempts at scanning images fail.
    """
    for i in range(3):
        try:
            check_call(
                [
                    "scanimage",
                    "-d",
                    "airscan:e0:BigFatInkTank",
                    "--format",
                    "png",
                    f"--batch=scratch/{description}-%d.png",
                    "--mode",
                    "color",
                    "--batch-prompt",
                    "--resolution=300",
                ]
            )
            break
        except SubprocessError as e:
            if i == 2:
                raise e

    return sorted(Path("scratch").glob(f"{description}-*.png"))


def ocr_images(image_list: list[Path], category_path: Path) -> str:
    """
    Convert a list of PNG images to WebP, run OCR on them, and format the resulting text into Markdown.

    Args:
        image_list (list[Path]): A list of Path objects representing the input image files.

    Returns:
        str: The formatted Markdown string containing the OCR results.
    """
    ocr_text: list[str] = []
    markdown_images: list[str] = []

    for i, old_png in enumerate(image_list):
        new_webp = (category_path / "images" / old_png.name).with_suffix(".webp")
        print(f"Converting {old_png} to {new_webp}")
        check_call(
            ["cwebp", "-preset", "text", old_png.as_posix(), "-o", new_webp.as_posix()]
        )
        old_png.unlink()
        print(f"Running OCR on {new_webp}")
        ocr_text.append(check_output(["ocrs", new_webp]).decode())
        markdown_images.append(
            f"![Recipe scan {i+1}](images/{url_escape(new_webp.name)})"
        )

    print(f"Cleaning up OCR:\n\n{ocr_text}\n\n")

    markdown_images = "\n".join(markdown_images)
    ocr_text = "\n\n".join(ocr_text)
    clean_ocr_text = clean_ocr_or_ask_for_input(ocr_text)

    return f"{clean_ocr_text}\n\n{markdown_images}"


def clean_ocr_or_ask_for_input(ocr_text: str) -> str:
    """Try to clean up the OCR using Llama and if it fails, ask for user input."""

    def ollama(prompt: str) -> str:
        print(f"Ollama input: {prompt}")
        output = check_output(
            ["ollama", "run", "llama3.1"],
            input=prompt.encode(),
            env={"OLLAMA_HOST": "http://localhost:11435", **os.environ},
        ).decode()
        print(f"Ollama output: {output}")
        return output

    def clean_this_up(test: str) -> str:
        prompt = f"""Clean up the following OCR of a scan of a recipe.
Reformat it into Ingredients and Directions like a standard recipe, in Markdown.

Add a paragraph or two of introduction describing the recipe, and at the bottom add suggested variations or adjustments. Be positive!

Use common sense to fix any errors (like 60 eggs or 3 gallons of vanilla extract).

---

{ocr_text}

---
"""
        return ollama(prompt)

    while True:
        if input("Is this OCR corrupted? [y/n]> ").lower().startswith("y"):
            print(
                "The OCR is too messy. Please provide a more clear version. End with an empty line."
            )
            ocr_text = []
            while next_line := input("> "):
                ocr_text.append(next_line)
            ocr_text = "\n".join(ocr_text)
        else:
            return clean_this_up(ocr_text)


def auto_crop_scan(color: np.ndarray) -> np.ndarray:
    """
    Remove the white scan area outside a recipe, using an advanced image processing algorithm.

    The algorithm works by:

    1. Applying a brightness stretch to emphasize the bright end of the scale.
    2. Removing specks using morphological operations.
    3. Thresholding the image using Otsu's method to get binary markers for the foreground and background.
    4. Dilating the foreground marker to be conservative about what is considered foreground.
    5. Eroding the background marker to be conservative about what is considered background.
    6. Using the Watershed algorithm with the foreground and background markers as inputs, to segment the image into foreground and background regions.
    7. Finding the largest contour in the foreground region, which represents the black area (foreground) of the recipe.

    The cropped image is then returned, with the white scan area outside the recipe removed.

    Args:
        color (np.ndarray): A 3D NumPy array representing the input image.

    Returns:
        np.ndarray: The cropped image.
    """
    oh, ow = color.shape[:2]

    # Emphasize the bright end of the scale, but don't threshold
    def stretch_brights(img: np.ndarray, power: float) -> np.ndarray:
        dark = 255 - img.astype(np.float16)
        dark = dark**power
        dark = dark * (255 / np.max(dark))
        return (255 - dark).clip(0, 255).astype(np.uint8)

    # Now try to remove specks using morphological operations
    kernel = np.ones((9, 9), dtype=np.uint8)
    pipe = color
    for _ in range(2):
        pipe = stretch_brights(color, 0.75)
        pipe = cv2.GaussianBlur(pipe, (9, 9), 0)
        pipe = cv2.morphologyEx(pipe, cv2.MORPH_CLOSE, kernel, iterations=4)

    # Now also threshold the pipe using Otsu's method so we can get some markers for watershed
    # This will be our markers for the watershed algorithm
    marker_pipe = cv2.cvtColor(pipe, cv2.COLOR_BGR2GRAY)
    _, marker_pipe = cv2.threshold(
        marker_pipe, 0, 255, cv2.THRESH_BINARY + cv2.THRESH_OTSU
    )
    # Dilate it a lot so we can be conservative about what is a foreground
    conservative_foreground = cv2.dilate(marker_pipe, kernel, iterations=4) == 0
    # Erode it so we can be conservative about what is background
    conservative_background = cv2.erode(marker_pipe, kernel, iterations=4) == 255

    watershed_input = pipe

    # Use Otsu's thresholding to get a binary image
    # ret, thresh = cv2.threshold(inverted, 0, 255, cv2.THRESH_OTSU)
    # Apply the Watershed algorithm to find the black area (foreground)
    # and the black area (foreground), then find the markers for each
    markers = np.zeros((oh, ow, 1), dtype=np.int32)
    # The top left corner is always the foreground
    markers[20 : oh // 10, 20 : ow // 10] = 1
    markers[conservative_foreground] = 1
    # The bottom and right edges are always the background
    markers[-(oh // 10) :] = 2
    markers[:, -(ow // 10) :] = 2
    markers[conservative_background] = 2

    # Perform the Watershed algorithm to segment the image
    cv2.watershed(watershed_input, markers)

    # The result of the watershed algorithm is stored in markers
    # We can now find the largest contour which represents the black area (foreground)
    contours, _ = cv2.findContours(
        (markers == 1).astype(np.uint8),
        cv2.RETR_EXTERNAL,
        cv2.CHAIN_APPROX_SIMPLE,
    )
    largest_contour = max(contours, key=cv2.contourArea)

    x, y, w, h = cv2.boundingRect(largest_contour)
    cropped_image = color[y : y + h, x : x + w]
    return cropped_image


def test_auto_crop_scan():
    image_names = ["Banana Split Cake-1", "Apricot Nectar Cake-1"]
    for image_name in image_names:
        source_image = cv2.imread(f"src/tests/{image_name}.webp")
        reference_cropped_image = cv2.imread(f"src/tests/{image_name} cropped.webp")
        cropped_image = auto_crop_scan(source_image)
        # Save the cropped image for visual inspection
        cv2.imwrite(f"src/tests/{image_name} cropped test.webp", cropped_image)
        # This isn't expected to be exactly the same, but should be close enough for visual inspection
        # 1. Be within a few pixels of the same dimensions
        assert (
            abs(cropped_image.shape[0] - reference_cropped_image.shape[0]) < 25
        ), f"Height difference too large: expected {reference_cropped_image.shape}, got {cropped_image.shape}"
        assert (
            abs(cropped_image.shape[1] - reference_cropped_image.shape[1]) < 25
        ), f"Width difference too large: expected {reference_cropped_image.shape}, got {cropped_image.shape}"


if __name__ == "__main__":
    description = input("Recipe name: ")
    description_safe = url_escape(description)
    category = input("Category: ")
    category_path = Path("book/src") / category
    category_path.mkdir(parents=True, exist_ok=True)

    image_list = scan_images(description)
    for image_path in image_list:
        source_image = cv2.imread(image_path.as_posix())
        cropped_image = auto_crop_scan(source_image)
        cv2.imwrite(image_path.as_posix(), cropped_image)

    ocr_markdown = ocr_images(image_list, category_path)

    destination = category_path / f"{description}.md"
    destination.write_text(ocr_markdown)
    print(f"Saved result to {destination.as_posix()}")

    Path("book/src/SUMMARY.md").open("at").write(
        f"- [{description}](./{category}/{description_safe}.md)\n"
    )

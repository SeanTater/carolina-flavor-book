from __future__ import annotations

import argparse
import base64
import mimetypes
import shutil
from pathlib import Path
from typing import Optional

import requests


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Upload an image + prompt to the diffusion server's /api/image-edit endpoint."
    )
    parser.add_argument("--server", default="http://127.0.0.1:8000", help="Server base URL.")
    parser.add_argument(
        "--image",
        type=Path,
        required=True,
        help="Path to the local image that should be edited.",
    )
    parser.add_argument(
        "--prompt",
        required=True,
        help="Edit prompt to send to the server.",
    )
    parser.add_argument(
        "--negative-prompt",
        default="",
        help="Optional negative prompt.",
    )
    parser.add_argument("--max-width", type=int, default=1664, help="Maximum canvas width (pixels).")
    parser.add_argument("--max-height", type=int, default=928, help="Maximum canvas height (pixels).")
    parser.add_argument("--steps", type=int, default=30, help="Diffusion steps.")
    parser.add_argument("--cfg-scale", type=float, default=1.0, help="Classifier-free guidance.")
    parser.add_argument("--sampler", default="euler", help="Sampler name.")
    parser.add_argument("--scheduler", default="simple", help="Scheduler name.")
    parser.add_argument(
        "--denoise",
        type=float,
        default=1.0,
        help="Denoise strength (0 keeps the original, 1 fully regenerates).",
    )
    parser.add_argument("--seed", type=int, default=None, help="Optional random seed.")
    parser.add_argument(
        "--save-to-disk",
        action="store_true",
        help="Ask the server to persist the PNG under --output-dir.",
    )
    parser.add_argument(
        "--filename",
        default=None,
        help="Optional filename when --save-to-disk is set.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Write to a new file instead of replacing the input image.",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=120.0,
        help="HTTP timeout in seconds.",
    )
    return parser.parse_args()


def _backup_original(path: Path) -> Path:
    suffix = path.suffix or ".png"
    backup = path.with_name(f"{path.stem}.backup{suffix}")
    counter = 1
    while backup.exists():
        backup = path.with_name(f"{path.stem}.backup{counter}{suffix}")
        counter += 1
    shutil.move(str(path), str(backup))
    return backup


def _download_image(server: str, image_url: str, timeout: float) -> bytes:
    response = requests.get(f"{server.rstrip('/')}{image_url}", timeout=timeout)
    response.raise_for_status()
    return response.content


def run() -> None:
    args = parse_args()
    image_path: Path = args.image.resolve()
    if not image_path.exists():
        raise SystemExit(f"Image file {image_path} does not exist.")
    if not image_path.is_file():
        raise SystemExit(f"Image path {image_path} is not a regular file.")

    endpoint = f"{args.server.rstrip('/')}/api/image-edit"

    data: dict[str, str] = {
        "prompt": args.prompt,
        "negative_prompt": args.negative_prompt,
        "max_width": str(args.max_width),
        "max_height": str(args.max_height),
        "steps": str(args.steps),
        "cfg_scale": str(args.cfg_scale),
        "sampler": args.sampler,
        "scheduler": args.scheduler,
        "denoise": str(args.denoise),
        "save_to_disk": "true" if args.save_to_disk else "false",
        "include_base64": "true",
    }
    if args.seed is not None:
        data["seed"] = str(args.seed)
    if args.filename:
        data["filename"] = args.filename

    mime_type, _ = mimetypes.guess_type(str(image_path))
    mime_type = mime_type or "application/octet-stream"

    with image_path.open("rb") as handle:
        files = {"image": (image_path.name, handle, mime_type)}
        response = requests.post(endpoint, data=data, files=files, timeout=args.timeout)
    try:
        response.raise_for_status()
    except requests.HTTPError as exc:
        raise SystemExit(f"Server error: {exc.response.text}") from exc

    payload = response.json()
    job_id = payload["job_id"]
    base64_png: Optional[str] = payload.get("base64_png")
    if base64_png:
        image_bytes = base64.b64decode(base64_png)
    else:
        image_bytes = _download_image(args.server, payload["image_url"], args.timeout)

    target_path = args.output.resolve() if args.output else image_path
    backup_path: Optional[Path] = None
    if args.output:
        target_path.parent.mkdir(parents=True, exist_ok=True)
    else:
        backup_path = _backup_original(target_path)

    with target_path.open("wb") as output_handle:
        output_handle.write(image_bytes)

    if backup_path:
        print(f"Original image moved to {backup_path}")
    print(f"Edited image written to {target_path} (job_id={job_id})")
    if payload.get("saved_path"):
        print(f"Server saved PNG at {payload['saved_path']}")


if __name__ == "__main__":
    run()

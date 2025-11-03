import argparse
import logging
import warnings
from pathlib import Path

import torch

from diffusion_example import comfy_bridge


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate a sample image via the Comfy bridge.")
    parser.add_argument(
        "--lora",
        action="append",
        default=[],
        metavar="ID[:strength_model[:strength_clip]]",
        help="Apply a LoRA (name from Comfy loras directory or explicit path). "
        "Specify multiple times to stack LoRAs.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("example.png"),
        help="Where to save the generated PNG (default: example.png).",
    )
    return parser.parse_args()


def main():
    args = parse_args()

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(levelname)s - %(message)s",
    )
    logger = logging.getLogger(__name__)

    warnings.filterwarnings("ignore", message=".*TF32.*")

    if not torch.cuda.is_available():
        raise RuntimeError("CUDA is required for this model. CPU would be too slow.")

    logger.info("GPU: %s", torch.cuda.get_device_name(0))
    logger.info("Bootstrapping ComfyUI components...")

    try:
        lora_specs = [comfy_bridge.parse_lora_spec(value) for value in args.lora]
    except ValueError as exc:
        raise SystemExit(f"Invalid --lora argument: {exc}") from exc

    if lora_specs:
        logger.info(
            "Preparing to apply %d LoRA(s): %s",
            len(lora_specs),
            ", ".join(spec.identifier for spec in lora_specs),
        )

    components = comfy_bridge.load_qwen_components(loras=lora_specs)

    positive_magic = {
        "en": ", Ultra HD, 4K, cinematic composition.",
        "zh": ", 超清，4K，电影级构图.",
    }
    prompt = (
        'A coffee shop entrance features a chalkboard sign reading "Qwen Coffee 😊 $2 per cup," '
        'with a neon light beside it displaying "通义千问". Next to it hangs a poster showing a '
        'beautiful Chinese woman, and beneath the poster is written '
        '"π≈3.1415926-53589793-23846264-33832795-02384197".'
    )
    positive_prompt = prompt + positive_magic["en"]
    negative_prompt = " "

    width, height = 1664, 928
    steps = 50
    cfg_scale = 4.0
    seed = 42
    sampler_name = "euler"
    scheduler = "simple"

    with torch.inference_mode():
        logger.info("Encoding prompts with Comfy CLIP...")
        positive = comfy_bridge.encode_prompt(components.clip, positive_prompt)
        negative = comfy_bridge.encode_prompt(components.clip, negative_prompt)

        latent = comfy_bridge.make_empty_latent(width, height)
        logger.info(
            "Sampling latents (steps=%d cfg=%.2f sampler=%s scheduler=%s seed=%d)",
            steps,
            cfg_scale,
            sampler_name,
            scheduler,
            seed,
        )
        sampled = comfy_bridge.sample_latent(
            components,
            positive=positive,
            negative=negative,
            latent=latent,
            seed=seed,
            steps=steps,
            cfg_scale=cfg_scale,
            sampler_name=sampler_name,
            scheduler=scheduler,
        )

        samples = sampled["samples"]
        logger.info(
            "Latent stats - min %.4f max %.4f mean %.4f any_nan=%s",
            samples.min().item(),
            samples.max().item(),
            samples.mean().item(),
            torch.isnan(samples).any().item(),
        )

        logger.info("Decoding latents with Comfy VAE...")
        decoded = comfy_bridge.decode_latent(components, sampled)

    logger.info(
        "Decoded tensor stats - min %.4f max %.4f mean %.4f any_nan=%s",
        decoded.min().item(),
        decoded.max().item(),
        decoded.mean().item(),
        torch.isnan(decoded).any().item(),
    )
    logger.info("Decoded tensor shape: %s", list(decoded.shape))

    image = comfy_bridge.tensor_to_pil(decoded)
    output_path = args.output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    image.save(output_path)
    logger.info("Image saved to %s", output_path.resolve())


if __name__ == "__main__":
    main()

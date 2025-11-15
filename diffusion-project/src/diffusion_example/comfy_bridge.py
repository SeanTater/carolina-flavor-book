from __future__ import annotations

import logging
import math
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Sequence

import numpy as np
import torch
from PIL import Image

logger = logging.getLogger(__name__)

_DEFAULT_COMFY_ROOT = Path(
    os.environ.get("COMFYUI_ROOT", "/home/sean-gallagher/sandbox/ComfyUI-2025")
)

if _DEFAULT_COMFY_ROOT.exists():
    root_str = str(_DEFAULT_COMFY_ROOT)
    if root_str not in sys.path:
        sys.path.insert(0, root_str)

import comfy
import comfy.cli_args  # noqa: F401
import comfy.model_management
import comfy.options
import comfy.sd
import comfy.utils
import folder_paths
import nodes
import node_helpers
from comfy_extras import nodes_cfg, nodes_model_advanced


@dataclass
class QwenEditContext:
    images_vl: list[torch.Tensor]
    ref_latents: list[torch.Tensor]

_BOOTSTRAPPED = False
_COMFY_ROOT: Optional[Path] = None


@dataclass
class ComfyComponents:
    model: "comfy.model_patcher.ModelPatcher"
    clip: "comfy.sd.CLIP"
    vae: "comfy.sd.VAE"
    loras: tuple["LoraSpec", ...] = ()


@dataclass(frozen=True)
class LoraSpec:
    identifier: str
    strength_model: float = 1.0
    strength_clip: Optional[float] = None

    def clip_strength(self) -> float:
        return self.strength_model if self.strength_clip is None else self.strength_clip


def _resolve_comfy_root(custom_root: Optional[Path | str]) -> Path:
    if custom_root is not None:
        root = Path(custom_root).expanduser().resolve()
    else:
        root = _DEFAULT_COMFY_ROOT
    if not root.exists():
        raise FileNotFoundError(
            f"ComfyUI root '{root}' does not exist. Set COMFYUI_ROOT or pass comfy_root."
        )
    return root


def bootstrap_comfy(comfy_root: Optional[Path | str] = None) -> Path:
    global _BOOTSTRAPPED, _COMFY_ROOT
    if _BOOTSTRAPPED:
        return _COMFY_ROOT  # type: ignore[return-value]

    root = _resolve_comfy_root(comfy_root)
    if str(root) not in sys.path:
        sys.path.insert(0, str(root))

    comfy.options.enable_args_parsing(False)

    _BOOTSTRAPPED = True
    _COMFY_ROOT = root
    return root


def load_qwen_components(
    comfy_root: Optional[Path | str] = None,
    diffusion_name: str = "qwen_image_fp8_e4m3fn.safetensors",
    text_encoder_name: str = "qwen_2.5_vl_7b_fp8_scaled.safetensors",
    vae_name: str = "qwen_image_vae.safetensors",
    aura_shift: float = 3.0,
    cfg_norm_strength: float = 1.0,
    loras: Optional[Sequence[LoraSpec]] = None,
    clip_type_name: str = "QWEN_IMAGE",
) -> ComfyComponents:
    bootstrap_comfy(comfy_root)

    model_options: dict[str, object] = {"dtype": torch.float8_e4m3fn}
    diffusion_path = folder_paths.get_full_path_or_raise(
        "diffusion_models", diffusion_name
    )
    model = comfy.sd.load_diffusion_model(diffusion_path, model_options=model_options)
    model = nodes_model_advanced.ModelSamplingAuraFlow().patch_aura(model, aura_shift)[
        0
    ]
    model = nodes_cfg.CFGNorm.execute(model, cfg_norm_strength)[0]

    clip_path = folder_paths.get_full_path_or_raise("text_encoders", text_encoder_name)
    clip_type = getattr(comfy.sd.CLIPType, clip_type_name)
    clip = comfy.sd.load_clip(
        ckpt_paths=[clip_path],
        embedding_directory=folder_paths.get_folder_paths("embeddings"),
        clip_type=clip_type,
    )

    vae_path = folder_paths.get_full_path_or_raise("vae", vae_name)
    vae_state = comfy.utils.load_torch_file(vae_path)
    vae = comfy.sd.VAE(sd=vae_state)

    comfy.model_management.load_models_gpu([model])  # moves fp8 UNet to GPU immediately

    applied_loras: list[LoraSpec] = []
    if loras:
        for spec in loras:
            resolved_path = _resolve_lora_identifier(spec.identifier, folder_paths)
            logger.info(
                "Applying LoRA %s (model %.3f / clip %.3f)",
                resolved_path.name,
                spec.strength_model,
                spec.clip_strength(),
            )
            lora_state = comfy.utils.load_torch_file(
                resolved_path.as_posix(), safe_load=True
            )
            model, clip = comfy.sd.load_lora_for_models(
                model,
                clip,
                lora_state,
                spec.strength_model,
                spec.clip_strength(),
            )
            applied_loras.append(spec)

    return ComfyComponents(model=model, clip=clip, vae=vae, loras=tuple(applied_loras))


def encode_prompt(
    clip: "comfy.sd.CLIP",
    prompt: str,
    llama_template: Optional[str] = None,
) -> "comfy_api.latest.io.Conditioning":
    tokens = clip.tokenize(prompt, llama_template=llama_template)
    return clip.encode_from_tokens_scheduled(tokens)


def make_empty_latent(
    width: int, height: int, batch_size: int = 1
) -> dict[str, torch.Tensor]:
    if width % 8 != 0 or height % 8 != 0:
        raise ValueError(
            "Width and height must be divisible by 8 for SD3/Qwen latents."
        )
    device = comfy.model_management.intermediate_device()
    latent = torch.zeros(
        batch_size,
        16,
        height // 8,
        width // 8,
        device=device,
    )
    return {"samples": latent}


def sample_latent(
    components: ComfyComponents,
    positive: "comfy_api.latest.io.Conditioning",
    negative: "comfy_api.latest.io.Conditioning",
    latent: dict[str, torch.Tensor],
    *,
    seed: int,
    steps: int,
    cfg_scale: float,
    sampler_name: str = "euler",
    scheduler: str = "simple",
    denoise: float = 1.0,
) -> dict[str, torch.Tensor]:
    sampled = nodes.common_ksampler(
        components.model,
        seed=seed,
        steps=steps,
        cfg=cfg_scale,
        sampler_name=sampler_name,
        scheduler=scheduler,
        positive=positive,
        negative=negative,
        latent=latent,
        denoise=denoise,
    )[0]
    return sampled


def decode_latent(
    components: ComfyComponents, latent: dict[str, torch.Tensor]
) -> torch.Tensor:
    with torch.inference_mode():
        return components.vae.decode(latent["samples"])


def tensor_to_pil(tensor: torch.Tensor) -> Image.Image:
    if tensor.ndim == 5:
        tensor = tensor[:, 0]
    if tensor.ndim == 4 and tensor.shape[-1] not in (1, 3):
        tensor = tensor.permute(0, 2, 3, 1)
    if tensor.ndim != 4 or tensor.shape[0] < 1:
        raise ValueError(
            f"Expected tensor shape [B, H, W, C]; received {tensor.shape}."
        )
    slice_ = torch.clamp(tensor[0], 0.0, 1.0)
    if slice_.shape[-1] == 1:
        slice_ = slice_.repeat(1, 1, 3)
    array = slice_.mul(255).round().to(torch.uint8).cpu()
    return Image.fromarray(array.numpy())


def generate_image(
    prompt: str,
    *,
    negative_prompt: str = "",
    width: int = 1472,
    height: int = 832,
    steps: int = 50,
    cfg_scale: float = 4.0,
    seed: int = 42,
    sampler_name: str = "euler",
    scheduler: str = "simple",
    comfy_components: Optional[ComfyComponents] = None,
    comfy_root: Optional[Path | str] = None,
) -> tuple[Image.Image, dict[str, torch.Tensor]]:
    components = comfy_components or load_qwen_components(comfy_root=comfy_root)

    positive = encode_prompt(components.clip, prompt)
    negative = encode_prompt(components.clip, negative_prompt or " ")

    latent = make_empty_latent(width, height)
    sampled = sample_latent(
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

    decoded = decode_latent(components, sampled)
    image = tensor_to_pil(decoded)
    return image, sampled


def load_qwen_image_edit_components(
    comfy_root: Optional[Path | str] = None,
    diffusion_name: str = "qwen_image_edit_2509_fp8_e4m3fn.safetensors",
    text_encoder_name: str = "qwen_2.5_vl_7b_fp8_scaled.safetensors",
    vae_name: str = "qwen_image_vae.safetensors",
    aura_shift: float = 3.0,
    cfg_norm_strength: float = 1.0,
    loras: Optional[Sequence[LoraSpec]] = None,
) -> ComfyComponents:
    """
    Convenience wrapper that loads the Qwen Image Edit checkpoints.
    """
    return load_qwen_components(
        comfy_root=comfy_root,
        diffusion_name=diffusion_name,
        text_encoder_name=text_encoder_name,
        vae_name=vae_name,
        aura_shift=aura_shift,
        cfg_norm_strength=cfg_norm_strength,
        loras=loras,
    )


def prepare_image_for_edit(
    image: Image.Image,
    max_width: int,
    max_height: int,
) -> Image.Image:
    """
    Resize and clamp images for the edit pipeline.
    """
    if max_width < 16 or max_height < 16:
        raise ValueError("max_width and max_height must be at least 16 pixels.")

    image = image.convert("RGB")
    width, height = image.size
    scale = min(max_width / width, max_height / height, 1.0)
    scaled_width = max(16, int(width * scale))
    scaled_height = max(16, int(height * scale))

    def clamp_multiple(value: int) -> int:
        remainder = value % 16
        clamped = value if remainder == 0 else value - remainder
        if clamped <= 0:
            clamped = 16
        return clamped

    target_width = clamp_multiple(scaled_width)
    target_height = clamp_multiple(scaled_height)
    if target_width != scaled_width or target_height != scaled_height:
        logger.debug(
            "Adjusted edit canvas from %dx%d to %dx%d to satisfy 16px boundaries.",
            scaled_width,
            scaled_height,
            target_width,
            target_height,
        )

    if (target_width, target_height) != image.size:
        resampling = getattr(Image, "Resampling", None)
        resample_filter = (
            resampling.LANCZOS if resampling else Image.LANCZOS  # type: ignore[attr-defined]
        )
        image = image.resize(
            (target_width, target_height),
            resample_filter,
        )

    logger.info("Prepared edit image at %dx%d.", target_width, target_height)
    return image


_QWEN_LLAMA_TEMPLATE = (
    "<|im_start|>system\nDescribe the key features of the input image (color, shape, "
    "size, texture, objects, background), then explain how the user's text instruction "
    "should alter or modify the image. Generate a new image that meets the user's "
    "requirements while maintaining consistency with the original input where appropriate."
    "<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n"
)


def prepare_qwen_edit_context(
    components: ComfyComponents, image: Image.Image
) -> QwenEditContext:
    samples = pil_to_tensor(image).movedim(-1, 1)
    images_vl = []
    ref_latents = []

    with torch.no_grad():
        # Vision-language scaling (approx 384x384 total pixels).
        total_vl = 384 * 384
        base_pixels = samples.shape[3] * samples.shape[2]
        scale_vl = math.sqrt(total_vl / max(base_pixels, 1))
        width_vl = max(32, round(samples.shape[3] * scale_vl))
        height_vl = max(32, round(samples.shape[2] * scale_vl))
        scaled_vl = comfy.utils.common_upscale(samples, width_vl, height_vl, "area", "disabled")
        images_vl.append(scaled_vl.movedim(1, -1))

        # Reference latent scaling (approx 1024x1024 total pixels, snapped to /8).
        total_latent = 1024 * 1024
        scale_latent = math.sqrt(total_latent / max(base_pixels, 1))
        width_latent = max(64, round(samples.shape[3] * scale_latent / 8.0) * 8)
        height_latent = max(64, round(samples.shape[2] * scale_latent / 8.0) * 8)
        scaled_latent = comfy.utils.common_upscale(samples, width_latent, height_latent, "area", "disabled")
        ref_latents.append(
            components.vae.encode(scaled_latent.movedim(1, -1)[:, :, :, :3]).detach()
        )

    return QwenEditContext(images_vl=images_vl, ref_latents=ref_latents)


def encode_qwen_edit_conditioning(
    components: ComfyComponents,
    prompt: str,
    context: QwenEditContext,
) -> "comfy_api.latest.io.Conditioning":

    image_prompt = "Picture 1: <|vision_start|><|image_pad|><|vision_end|>"
    tokens = components.clip.tokenize(
        image_prompt + prompt,
        images=context.images_vl,
        llama_template=_QWEN_LLAMA_TEMPLATE,
    )
    conditioning = components.clip.encode_from_tokens_scheduled(tokens)
    conditioning = node_helpers.conditioning_set_values(
        conditioning,
        {"reference_latents": context.ref_latents},
        append=True,
    )
    return conditioning


def pil_to_tensor(image: Image.Image) -> torch.Tensor:
    array = np.asarray(image, dtype=np.float32) / 255.0
    tensor = torch.from_numpy(array)
    if tensor.ndim == 2:
        tensor = tensor.unsqueeze(-1)
    return tensor.unsqueeze(0)


def image_to_latent(
    components: ComfyComponents,
    image: Image.Image,
) -> dict[str, torch.Tensor]:
    tensor = pil_to_tensor(image)
    device = comfy.model_management.intermediate_device()
    tensor = tensor.to(device=device, dtype=torch.float32)
    with torch.inference_mode():
        samples = components.vae.encode(tensor)
    return {"samples": samples}


def parse_lora_spec(spec: str) -> LoraSpec:
    """
    Parse CLI-style LoRA descriptors: "name", "name:0.8", or "name:0.8:0.5".
    """
    parts = spec.split(":")
    if not parts or not parts[0]:
        raise ValueError("LoRA identifier cannot be empty.")
    identifier = parts[0]
    strength_model = 1.0
    strength_clip: Optional[float] = None
    if len(parts) > 1 and parts[1] != "":
        try:
            strength_model = float(parts[1])
        except ValueError as exc:
            raise ValueError(f"Invalid model strength for LoRA '{spec}'.") from exc
    if len(parts) > 2 and parts[2] != "":
        try:
            strength_clip = float(parts[2])
        except ValueError as exc:
            raise ValueError(f"Invalid clip strength for LoRA '{spec}'.") from exc
    return LoraSpec(
        identifier=identifier,
        strength_model=strength_model,
        strength_clip=strength_clip,
    )


def _resolve_lora_identifier(identifier: str, folder_paths_module) -> Path:
    candidate = Path(identifier).expanduser()
    if candidate.is_file():
        return candidate.resolve()
    return Path(
        folder_paths_module.get_full_path_or_raise("loras", identifier)
    ).resolve()

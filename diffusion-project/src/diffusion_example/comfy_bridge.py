from __future__ import annotations

import logging
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Sequence

import torch
from PIL import Image

logger = logging.getLogger(__name__)

_DEFAULT_COMFY_ROOT = Path(
    os.environ.get("COMFYUI_ROOT", "/home/sean-gallagher/sandbox/ComfyUI-2025")
)

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

    import comfy.options

    comfy.options.enable_args_parsing(False)

    import comfy.cli_args  # noqa: F401  # ensures args are initialised
    import folder_paths  # noqa: F401  # registers model folders

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
) -> ComfyComponents:
    bootstrap_comfy(comfy_root)

    import comfy.model_management
    import comfy.sd
    import comfy.utils
    import folder_paths
    from comfy_extras import nodes_cfg, nodes_model_advanced

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
    clip = comfy.sd.load_clip(
        ckpt_paths=[clip_path],
        embedding_directory=folder_paths.get_folder_paths("embeddings"),
        clip_type=comfy.sd.CLIPType.QWEN_IMAGE,
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
    import comfy.model_management

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
    import nodes

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

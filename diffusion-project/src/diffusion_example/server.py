from __future__ import annotations

import argparse
import asyncio
import base64
import io
import logging
import os
import signal
import time
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Literal, Optional, Sequence

import uvicorn
from fastapi import FastAPI, File, Form, HTTPException, Request, Response, UploadFile
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import HTMLResponse, StreamingResponse
from fastapi.staticfiles import StaticFiles
from PIL import Image
from pydantic import BaseModel, Field, field_validator

from diffusion_example import comfy_bridge

LOGGER = logging.getLogger("diffusion_server")

STATIC_DIR = Path(__file__).resolve().parent / "static"
DEFAULT_OUTPUT_DIR = Path(os.environ.get("DIFFUSION_OUTPUT_DIR", "outputs"))


def _setup_logging(verbose: bool) -> None:
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s - %(levelname)s - %(name)s - %(message)s",
    )


class GenerationRequest(BaseModel):
    prompt: str = Field(..., min_length=1, max_length=2048)
    negative_prompt: str = Field(default="")
    width: int = Field(default=1664, ge=256, le=2048)
    height: int = Field(default=928, ge=256, le=2048)
    steps: int = Field(default=50, ge=1, le=200)
    cfg_scale: float = Field(default=4.0, ge=0.0, le=20.0)
    seed: Optional[int] = Field(default=None, ge=0, le=2**63 - 1)
    sampler: str = Field(default="euler")
    scheduler: str = Field(default="simple")
    save_to_disk: bool = Field(default=False, description="Persist image to disk automatically.")
    filename: Optional[str] = Field(default=None, description="Optional file name when saving to disk.")
    include_base64: bool = Field(default=False, description="Embed a base64 PNG in the response.")

    @field_validator("width", "height")
    @classmethod
    def multiples_of_eight(cls, value: int) -> int:
        if value % 8 != 0:
            raise ValueError("Width and height must be divisible by 8.")
        return value

    @field_validator("filename")
    @classmethod
    def no_path_traversal(cls, value: Optional[str]) -> Optional[str]:
        if value and ("/" in value or "\\" in value):
            raise ValueError("Filename must not contain directory separators.")
        return value


class SaveRequest(BaseModel):
    filename: str = Field(..., min_length=1, max_length=255)

    @field_validator("filename")
    @classmethod
    def no_path_traversal(cls, value: str) -> str:
        if "/" in value or "\\" in value:
            raise ValueError("Filename must not contain directory separators.")
        return value


class GenerationResponse(BaseModel):
    job_id: str
    image_url: str
    saved_path: Optional[str] = None
    base64_png: Optional[str] = None


@dataclass
class GeneratedImage:
    job_id: str
    image_bytes: bytes
    saved_path: Optional[Path]


@dataclass
class ImageEditJobRequest:
    prompt: str
    negative_prompt: str
    max_width: int
    max_height: int
    steps: int
    cfg_scale: float
    seed: Optional[int]
    sampler: str
    scheduler: str
    denoise: float
    save_to_disk: bool
    filename: Optional[str]
    image_bytes: bytes


class ResultStore:
    def __init__(self, ttl_seconds: int = 900):
        self._ttl = ttl_seconds
        self._store: dict[str, tuple[bytes, float]] = {}
        self._lock = asyncio.Lock()

    async def add(self, job_id: str, data: bytes) -> None:
        async with self._lock:
            self._store[job_id] = (data, time.monotonic())

    async def get(self, job_id: str) -> Optional[bytes]:
        async with self._lock:
            item = self._store.get(job_id)
            if item is None:
                return None
            data, timestamp = item
            if self._ttl and time.monotonic() - timestamp > self._ttl:
                self._store.pop(job_id, None)
                return None
            return data

    async def cleanup(self) -> None:
        if not self._ttl:
            return
        async with self._lock:
            expire_before = time.monotonic() - self._ttl
            stale = [job_id for job_id, (_, ts) in self._store.items() if ts < expire_before]
            for job_id in stale:
                self._store.pop(job_id, None)


class ComfyService:
    def __init__(self, output_dir: Path, loras: Optional[Sequence[comfy_bridge.LoraSpec]] = None):
        self._output_dir = output_dir
        self._output_dir.mkdir(parents=True, exist_ok=True)
        self._loras = tuple(loras or ())
        LOGGER.info("Loading ComfyUI components...")
        self._components = comfy_bridge.load_qwen_components(loras=self._loras)
        self._lock = asyncio.Lock()
        self._last_used = time.monotonic()

    def touch(self) -> None:
        self._last_used = time.monotonic()

    def last_used(self) -> float:
        return self._last_used

    def loras(self) -> tuple[comfy_bridge.LoraSpec, ...]:
        return self._loras

    async def generate(self, request: GenerationRequest) -> GeneratedImage:
        async with self._lock:
            self.touch()
            job_id = uuid.uuid4().hex

            def _generate() -> GeneratedImage:
                image, _latent = comfy_bridge.generate_image(
                    prompt=request.prompt,
                    negative_prompt=request.negative_prompt,
                    width=request.width,
                    height=request.height,
                    steps=request.steps,
                    cfg_scale=request.cfg_scale,
                    seed=request.seed or 0,
                    sampler_name=request.sampler,
                    scheduler=request.scheduler,
                    comfy_components=self._components,
                )
                buffer = io.BytesIO()
                image.save(buffer, format="PNG")
                image_bytes = buffer.getvalue()
                saved_path = None
                if request.save_to_disk:
                    filename = request.filename or f"{job_id}.png"
                    saved_path = self._write_image(image_bytes, filename)
                return GeneratedImage(job_id=job_id, image_bytes=image_bytes, saved_path=saved_path)

            result = await asyncio.to_thread(_generate)
            self.touch()
            return result

    def save_existing(self, job_id: str, data: bytes, filename: str) -> Path:
        self.touch()
        return self._write_image(data, filename, suffix=f"-{job_id}")

    def _write_image(self, image_bytes: bytes, filename: str, suffix: str = "") -> Path:
        safe_filename = filename
        if not safe_filename.lower().endswith(".png"):
            safe_filename = f"{safe_filename}{suffix}.png" if suffix else f"{safe_filename}.png"
        target = (self._output_dir / safe_filename).resolve()
        if self._output_dir.resolve() not in target.parents and target != self._output_dir.resolve():
            raise HTTPException(status_code=400, detail="Filename escapes output directory.")
        target.parent.mkdir(parents=True, exist_ok=True)
        with open(target, "wb") as handle:
            handle.write(image_bytes)
        LOGGER.info("Saved image to %s", target)
        return target


class ImageEditService:
    def __init__(self, output_dir: Path, loras: Optional[Sequence[comfy_bridge.LoraSpec]] = None):
        self._output_dir = output_dir
        self._output_dir.mkdir(parents=True, exist_ok=True)
        self._loras = tuple(loras or ())
        LOGGER.info("Loading ComfyUI components for image edit...")
        self._components = comfy_bridge.load_qwen_image_edit_components(loras=self._loras)
        self._lock = asyncio.Lock()
        self._last_used = time.monotonic()

    def touch(self) -> None:
        self._last_used = time.monotonic()

    def last_used(self) -> float:
        return self._last_used

    def loras(self) -> tuple[comfy_bridge.LoraSpec, ...]:
        return self._loras

    async def generate(self, request: ImageEditJobRequest) -> GeneratedImage:
        async with self._lock:
            self.touch()
            job_id = uuid.uuid4().hex

            def _generate() -> GeneratedImage:
                with Image.open(io.BytesIO(request.image_bytes)) as pil_image:
                    pil_image.load()
                    prepared = comfy_bridge.prepare_image_for_edit(
                        pil_image,
                        request.max_width,
                        request.max_height,
                    )
                context = comfy_bridge.prepare_qwen_edit_context(self._components, prepared)
                positive = comfy_bridge.encode_qwen_edit_conditioning(
                    self._components,
                    request.prompt,
                    context,
                )
                negative = comfy_bridge.encode_prompt(
                    self._components.clip,
                    request.negative_prompt or " ",
                )
                latent = comfy_bridge.image_to_latent(self._components, prepared)
                sampled = comfy_bridge.sample_latent(
                    self._components,
                    positive=positive,
                    negative=negative,
                    latent=latent,
                    seed=request.seed or 0,
                    steps=request.steps,
                    cfg_scale=request.cfg_scale,
                    sampler_name=request.sampler,
                    scheduler=request.scheduler,
                    denoise=request.denoise,
                )
                decoded = comfy_bridge.decode_latent(self._components, sampled)
                image = comfy_bridge.tensor_to_pil(decoded)

                buffer = io.BytesIO()
                image.save(buffer, format="PNG")
                image_bytes = buffer.getvalue()
                saved_path = None
                if request.save_to_disk:
                    filename = request.filename or f"{job_id}.png"
                    saved_path = self._write_image(image_bytes, filename)
                return GeneratedImage(
                    job_id=job_id,
                    image_bytes=image_bytes,
                    saved_path=saved_path,
                )

            result = await asyncio.to_thread(_generate)
            self.touch()
            return result

    def save_existing(self, job_id: str, data: bytes, filename: str) -> Path:
        self.touch()
        return self._write_image(data, filename, suffix=f"-{job_id}")

    def _write_image(self, image_bytes: bytes, filename: str, suffix: str = "") -> Path:
        safe_filename = filename
        if not safe_filename.lower().endswith(".png"):
            safe_filename = f"{safe_filename}{suffix}.png" if suffix else f"{safe_filename}.png"
        target = (self._output_dir / safe_filename).resolve()
        if self._output_dir.resolve() not in target.parents and target != self._output_dir.resolve():
            raise HTTPException(status_code=400, detail="Filename escapes output directory.")
        target.parent.mkdir(parents=True, exist_ok=True)
        with open(target, "wb") as handle:
            handle.write(image_bytes)
        LOGGER.info("Saved image to %s", target)
        return target


def create_app(
    service: ComfyService | ImageEditService,
    store: ResultStore,
    pipeline: Literal["generate", "image-edit"],
) -> FastAPI:
    app = FastAPI(title="Diffusion Comfy Bridge", version="0.2.0")

    if STATIC_DIR.exists():
        app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")

    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    @app.middleware("http")
    async def record_usage(request: Request, call_next):
        service.touch()
        response: Response = await call_next(request)
        service.touch()
        return response

    @app.get("/", response_class=HTMLResponse)
    async def landing_page() -> HTMLResponse:
        if not STATIC_DIR.exists():
            return HTMLResponse("<html><body><h1>Diffusion server running</h1></body></html>")
        return HTMLResponse((STATIC_DIR / "index.html").read_text(encoding="utf-8"))

    if pipeline == "generate":
        @app.post("/api/generate", response_model=GenerationResponse)
        async def generate_image(request: GenerationRequest) -> GenerationResponse:
            result = await service.generate(request)  # type: ignore[arg-type]
            await store.add(result.job_id, result.image_bytes)
            base64_png = (
                base64.b64encode(result.image_bytes).decode("ascii") if request.include_base64 else None
            )
            saved_path = str(result.saved_path) if result.saved_path else None
            return GenerationResponse(
                job_id=result.job_id,
                image_url=f"/api/image/{result.job_id}",
                saved_path=saved_path,
                base64_png=base64_png,
            )

    if pipeline == "image-edit":
        @app.post("/api/image-edit", response_model=GenerationResponse)
        async def edit_image(
            prompt: str = Form(..., min_length=1, max_length=2048),
            negative_prompt: str = Form("", max_length=2048),
            max_width: int = Form(1664, ge=16, le=2048),
            max_height: int = Form(928, ge=16, le=2048),
            steps: int = Form(30, ge=1, le=200),
            cfg_scale: float = Form(4.0, ge=0.0, le=20.0),
            seed: Optional[int] = Form(None, ge=0, le=2**63 - 1),
            sampler: str = Form("euler"),
            scheduler: str = Form("simple"),
            denoise: float = Form(1.0, ge=0.0, le=1.0),
            save_to_disk: bool = Form(False),
            filename: Optional[str] = Form(None, max_length=255),
            include_base64: bool = Form(False),
            image: UploadFile = File(...),
        ) -> GenerationResponse:
            if filename and ("/" in filename or "\\" in filename):
                raise HTTPException(status_code=400, detail="Filename must not contain directory separators.")
            payload = await image.read()
            if not payload:
                raise HTTPException(status_code=400, detail="Uploaded image is empty.")

            request = ImageEditJobRequest(
                prompt=prompt,
                negative_prompt=negative_prompt,
                max_width=max_width,
                max_height=max_height,
                steps=steps,
                cfg_scale=cfg_scale,
                seed=seed,
                sampler=sampler,
                scheduler=scheduler,
                denoise=denoise,
                save_to_disk=save_to_disk,
                filename=filename,
                image_bytes=payload,
            )
            result = await service.generate(request)  # type: ignore[arg-type]
            await store.add(result.job_id, result.image_bytes)
            base64_png = (
                base64.b64encode(result.image_bytes).decode("ascii") if include_base64 else None
            )
            saved_path = str(result.saved_path) if result.saved_path else None
            return GenerationResponse(
                job_id=result.job_id,
                image_url=f"/api/image/{result.job_id}",
                saved_path=saved_path,
                base64_png=base64_png,
            )

    @app.get("/api/image/{job_id}")
    async def get_image(job_id: str) -> StreamingResponse:
        data = await store.get(job_id)
        if data is None:
            raise HTTPException(status_code=404, detail="Image not found or expired.")
        return StreamingResponse(io.BytesIO(data), media_type="image/png")

    @app.post("/api/save/{job_id}")
    async def save_image(job_id: str, request: SaveRequest) -> dict[str, str]:
        data = await store.get(job_id)
        if data is None:
            raise HTTPException(status_code=404, detail="Image not found or expired.")
        path = service.save_existing(job_id, data, request.filename)
        return {"job_id": job_id, "saved_path": str(path)}

    @app.get("/api/status")
    async def status() -> dict[str, object]:
        lora_data = [
            {
                "identifier": spec.identifier,
                "strength_model": spec.strength_model,
                "strength_clip": spec.clip_strength(),
            }
            for spec in service.loras()
        ]
        return {
            "start_time": app.state.start_time,
            "last_used": service.last_used(),
            "uptime": time.monotonic() - app.state.start_time,
            "pipeline": pipeline,
            "loras": lora_data,
        }

    @app.on_event("startup")
    async def note_startup() -> None:
        app.state.start_time = time.monotonic()

    return app


def _ensure_signal_handlers(server: uvicorn.Server) -> None:
    loop = asyncio.get_event_loop()

    def handle_shutdown(signame: str):
        LOGGER.info("Received signal %s, shutting down...", signame)
        server.should_exit = True

    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, handle_shutdown, sig.name)


async def _run_server(args: argparse.Namespace, loras: Sequence[comfy_bridge.LoraSpec]) -> None:
    output_dir = Path(args.output_dir)
    if args.pipeline == "generate":
        service: ComfyService | ImageEditService = ComfyService(output_dir=output_dir, loras=loras)
    else:
        service = ImageEditService(output_dir=output_dir, loras=loras)
    store = ResultStore(ttl_seconds=args.result_ttl)
    app = create_app(service, store, args.pipeline)
    config = uvicorn.Config(
        app,
        host=args.host,
        port=args.port,
        log_level="info" if not args.verbose else "debug",
    )
    server = uvicorn.Server(config)
    _ensure_signal_handlers(server)

    async def enforce_time_limit() -> None:
        await asyncio.sleep(args.time_limit)
        LOGGER.info("Time limit reached (%.1f seconds). Exiting.", args.time_limit)
        server.should_exit = True

    async def enforce_idle_timeout() -> None:
        while not server.should_exit:
            await asyncio.sleep(1)
            idle = time.monotonic() - service.last_used()
            if idle >= args.idle_timeout:
                LOGGER.info("Idle timeout reached (%.1f seconds). Exiting.", args.idle_timeout)
                server.should_exit = True
                break

    watchers = []
    if args.time_limit > 0:
        watchers.append(asyncio.create_task(enforce_time_limit()))
    if args.idle_timeout > 0:
        watchers.append(asyncio.create_task(enforce_idle_timeout()))

    try:
        await server.serve()
    finally:
        for task in watchers:
            task.cancel()
        await asyncio.gather(*watchers, return_exceptions=True)


def cli(argv: Optional[list[str]] = None) -> None:
    parser = argparse.ArgumentParser(description="Run the Comfy diffusion FastAPI server.")
    parser.add_argument("--host", default="127.0.0.1", help="Host to bind (default: 127.0.0.1).")
    parser.add_argument("--port", type=int, default=8000, help="Port to bind (default: 8000).")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=DEFAULT_OUTPUT_DIR,
        help="Directory for saved images (default: ./outputs).",
    )
    parser.add_argument(
        "--lora",
        action="append",
        default=[],
        metavar="ID[:strength_model[:strength_clip]]",
        help="Apply a LoRA (by name in the Comfy loras directory or explicit path). "
        "Can be specified multiple times.",
    )
    parser.add_argument(
        "--time-limit",
        type=float,
        default=0.0,
        help="Maximum runtime in seconds (0 disables).",
    )
    parser.add_argument(
        "--idle-timeout",
        type=float,
        default=0.0,
        help="Graceful shutdown after this many idle seconds (0 disables).",
    )
    parser.add_argument(
        "--result-ttl",
        type=int,
        default=900,
        help="Seconds to keep generated images in memory for download (default: 900).",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Enable debug logging.",
    )
    parser.add_argument(
        "--pipeline",
        choices=("generate", "image-edit"),
        default="generate",
        help="Select which Comfy pipeline to load (default: generate).",
    )

    args = parser.parse_args(argv)
    try:
        lora_specs = [comfy_bridge.parse_lora_spec(entry) for entry in args.lora]
    except ValueError as exc:
        parser.error(str(exc))

    _setup_logging(args.verbose)

    try:
        asyncio.run(_run_server(args, lora_specs))
    except KeyboardInterrupt:
        LOGGER.info("Server interrupted by user.")

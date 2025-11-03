# Diffusion Comfy Bridge API Reference

This document describes the HTTP interface exposed by the FastAPI server
(`uv run diffusion-server`). Unless otherwise noted, all endpoints live under
the base URL you start the process with (default: `http://127.0.0.1:8000`).

---

## Server Lifecycle & CLI

```bash
uv run diffusion-server [options]
```

| Option | Default | Description |
| --- | --- | --- |
| `--host` | `127.0.0.1` | network interface to bind |
| `--port` | `8000` | TCP port for the HTTP server |
| `--output-dir` | `./outputs` | directory where saved images are written |
| `--lora` | — | apply a LoRA (`name[:strength_model[:strength_clip]]`); repeatable |
| `--time-limit` | `0` (disabled) | total runtime limit in seconds; server exits once reached |
| `--idle-timeout` | `0` (disabled) | shutdown after this many idle seconds (no requests) |
| `--result-ttl` | `900` | seconds to keep generated PNGs in memory for re-download |
| `--verbose` | off | enable debug-level logging |

The model weights are loaded on startup and kept “warm” until the process
terminates, the time limit elapses, or the idle timeout is reached.

---

## Static UI

- `GET /`  
  Returns the bundled single-page client that can drive the API from a browser.
- `GET /static/*`  
  Serves supporting assets (currently only `index.html`).

---

## Generate Image

`POST /api/generate`

Launches a text-to-image run using the pre-loaded Comfy components.

### Request Body

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `prompt` | `string` | — | Required; main text prompt (max 2048 chars). |
| `negative_prompt` | `string` | `""` | Optional negative prompt. |
| `width` | `int` | `1664` | In pixels; must be divisible by 8 (range 256–2048). |
| `height` | `int` | `928` | In pixels; must be divisible by 8 (range 256–2048). |
| `steps` | `int` | `50` | Diffusion steps (1–200). |
| `cfg_scale` | `float` | `4.0` | Classifier-free guidance (0–20). |
| `seed` | `int` | `0` (random) | Explicit random seed; omit/`null` for stochastic runs. |
| `sampler` | `string` | `"euler"` | Comfy sampler name. |
| `scheduler` | `string` | `"simple"` | Comfy scheduler name. |
| `save_to_disk` | `bool` | `false` | Persist PNG to `--output-dir`. |
| `filename` | `string` | — | Optional filename (no path separators). |
| `include_base64` | `bool` | `false` | Embed base64 PNG in response payload. |

### Response

```jsonc
{
  "job_id": "8c37a43bdb3a44c5b820f04579fd89f0",
  "image_url": "/api/image/8c37a43bdb3a44c5b820f04579fd89f0",
  "saved_path": "/absolute/path/outputs/custom.png",
  "base64_png": "iVBORw0KGgoAAAANSUhEUgAA..."
}
```

- `job_id` – opaque identifier for subsequent download/save calls.
- `image_url` – relative path to fetch the PNG bytes.
- `saved_path` – only present when `save_to_disk=true`.
- `base64_png` – only present when `include_base64=true`.

### Errors

- `400` – validation failure (e.g., width not divisible by 8).
- `500` – unexpected generation failure (model load/logs will show cause).

---

## Download Generated Image

`GET /api/image/{job_id}`

Returns the PNG bytes associated with a prior job. Images remain available until
`--result-ttl` expires or the process restarts.

- **Success** – HTTP `200` with `Content-Type: image/png`.
- **Failure** – HTTP `404` if the job ID is unknown or expired.

---

## Save Image After Generation

`POST /api/save/{job_id}`

Allows consumers to persist a previously generated (in-memory) result to disk.

### Request Body

```json
{ "filename": "my_prompt.png" }
```

- `filename` must not contain directory separators and `.png` is appended if
  missing. Files are saved beneath `--output-dir`.

### Response

```json
{
  "job_id": "8c37a43bdb3a44c5b820f04579fd89f0",
  "saved_path": "/absolute/path/outputs/my_prompt.png"
}
```

- `404` if the job ID is unknown or expired.
- `400` if the requested filename would escape the output directory.

---

## Server Status

`GET /api/status`

Returns monotonic timestamps useful for health monitoring.

```json
{
  "start_time": 123456.78,
  "last_used": 123789.01,
  "uptime": 332.23,
  "loras": [
    {
      "identifier": "my-style-lora.safetensors",
      "strength_model": 0.75,
      "strength_clip": 0.75
    }
  ]
}
```

All values are in seconds from Python’s `time.monotonic()` and primarily let you
detect idleness remotely.

---

## Example Workflow

```bash
# Start the server with a LoRA applied and run until idle for 30 minutes
uv run diffusion-server --idle-timeout 1800 \
  --lora mystyle_lora.safetensors:0.75
```

```bash
# Generate an image
curl -s http://127.0.0.1:8000/api/generate \
  -H 'content-type: application/json' \
  -d '{
        "prompt": "A neon cyberpunk alley at dusk, rain-slick cobblestones",
        "width": 1472,
        "height": 832,
        "steps": 40,
        "save_to_disk": true,
        "filename": "alley.png"
      }'
```

```bash
# Download raw PNG and write to disk
curl -s http://127.0.0.1:8000/api/image/<job_id> --output result.png
```

```bash
# Persist another copy after the fact
curl -s -X POST http://127.0.0.1:8000/api/save/<job_id> \
  -H 'content-type: application/json' \
  -d '{ "filename": "archived.png" }'
```

Use the UI at `http://127.0.0.1:8000/` for manual testing or quick prompt
iterations.

# Diffusion Model Memory Optimization - Debugging Log

## Problem Statement
Attempting to run Qwen/Qwen-Image diffusion model (20B transformer + 7B text encoder + 200M VAE) on RTX 5090 with 32GB VRAM and 60GB system RAM.

**Model sizes in fp16:**
- Text encoder: 7B params = ~14GB
- Transformer: 20B params = ~40GB
- VAE: 200M params = ~0.4GB
- **Total: ~54.4GB** (doesn't fit in 32GB VRAM or 60GB RAM)

---

## Attempt 1: optimum-quanto fp8 Quantization

**Rationale:** Use fp8 quantization to halve memory usage (54GB → 27GB)

**Changes:**
- Used `optimum-quanto` library's `qfloat8` quantization
- Quantized transformer and text encoder before moving to GPU

**Result:** ❌ **FAILED**
- `optimum-quanto` required JIT compilation of CUDA kernels
- RTX 5090 (compute_120/Blackwell) not supported by CUDA 12.6
- Error: `nvcc fatal: Unsupported gpu architecture 'compute_120'`

---

## Attempt 2: torchao fp8 Quantization

**Rationale:** PyTorch's official quantization library might have better pre-compiled support

**Changes:**
- Switched from `optimum-quanto` to `torchao`
- Used `Float8WeightOnlyConfig()` for quantization

**Result:** ❌ **FAILED**
- `torchao`'s `Float8Tensor` incompatible with `device_map` auto-placement
- Error: `TypeError: Float8Tensor.__new__() got an unexpected keyword argument 'requires_grad'`
- Accelerate's device placement couldn't handle custom quantized tensors

---

## Attempt 3: Native PyTorch fp8 (torch.float8_e4m3fn)

**Rationale:** Use PyTorch's built-in fp8 dtype directly

**Changes:**
- `torch_dtype=torch.float8_e4m3fn` in `from_pretrained()`

**Result:** ❌ **FAILED**
- fp8 not supported as default dtype for model loading
- Error: `TypeError: couldn't find storage object Float8_e4m3fnStorage`
- PyTorch can't create tensors with fp8 as a default dtype

---

## Attempt 4: Load Everything in float16 to GPU

**Rationale:** Just load the full model in fp16 and see what happens

**Changes:**
- Simple `from_pretrained(torch_dtype=torch.float16).to(device)`

**Result:** ❌ **FAILED - OOM during load**
- Model loading tried to load all 54GB into CPU RAM first
- System OOM before even reaching GPU

---

## Attempt 5: Sequential CPU Offload

**Rationale:** Load components one at a time, only active component on GPU

**Changes:**
- `enable_sequential_cpu_offload()` after loading
- Supposed to keep only active component on GPU during inference

**Result:** ❌ **FAILED - Still OOM during initial load**
- Initial `from_pretrained()` still loaded everything into RAM
- Sequential offload only helps during inference, not loading

---

## Attempt 6: Model CPU Offload (enable_model_cpu_offload)

**Rationale:** Load all components, swap between CPU/GPU as needed

**Changes:**
- `enable_model_cpu_offload()` for automatic swapping

**Result:** ❌ **FAILED - System RAM OOM**
- Required all 54GB to fit in system RAM (you have 60GB)
- 54GB model + OS + overhead exceeded 60GB
- System became unresponsive

---

## Attempt 7: Balanced device_map

**Rationale:** Let HuggingFace automatically distribute model across GPU/CPU

**Changes:**
- `device_map="balanced"` in `from_pretrained()`

**Result:** ❌ **FAILED - Same issue**
- Still tried to load everything to determine split
- CPU RAM OOM before device distribution could happen

---

## Attempt 8: Int8 Quantization with BitsAndBytes

**Rationale:** Int8 reduces to 1 byte per param (54GB → 27GB), might fit in RAM

**Changes:**
- Used `BitsAndBytesConfig(load_in_8bit=True)`
- Applied to full pipeline

**Result:** ❌ **FAILED - Wrong API**
- `DiffusionPipeline` doesn't accept `BitsAndBytesConfig`
- Error: `quantization_config must be an instance of PipelineQuantizationConfig`

---

## Attempt 9: Load Components Individually

**Rationale:** Load one component at a time to avoid peak RAM usage

**Changes:**
```python
text_encoder = AutoModel.from_pretrained(...).to(device)
transformer = QwenImageTransformer2DModel.from_pretrained(...).to(device)
vae = AutoencoderKL.from_pretrained(...).to(device)
```

**Result:** ❌ **FAILED - Wrong VAE class**
- Used `AutoencoderKL` but model needs `AutoencoderKLQwenImage`
- Error: Shape mismatch in VAE weights

---

## Attempt 10: Correct VAE + Individual Loading + Int8 Transformer

**Rationale:** Load components individually with transformer in int8

**Changes:**
- Text encoder: GPU, fp16
- Transformer: GPU, int8 (with BitsAndBytes)
- VAE: GPU, fp32 (fixed to `AutoencoderKLQwenImage`)
- Scheduler: CPU

**Result:** ❌ **FAILED - VAE conv3d Error**
- Diffusion completed successfully!
- VAE decode failed with cryptic error
- Error: `NotImplementedError: Could not run 'aten::slow_conv3d_forward' with arguments from the 'CUDA' backend`

---

## Attempt 11: Disable VAE Tiling

**Rationale:** Error mentioned `tiled_decode()` which uses conv3d

**Changes:**
- Added `pipe.disable_vae_tiling()`
- Added `pipe.vae.disable_tiling()`

**Result:** ❌ **FAILED - Same conv3d error**
- Still tried to use conv3d operations
- Tiling wasn't the root cause

---

## Attempt 12: Move VAE to CPU

**Rationale:** conv3d error might be device mismatch, not missing op

**Changes:**
- Keep VAE on CPU where conv3d is supported

**Result:** ⚠️ **PARTIAL SUCCESS**
- Generation worked!
- But VAE decode took 30+ minutes on CPU
- Too slow to be practical

---

## Attempt 13: All Components on GPU (Device Consistency)

**Rationale:** Error might be from mixing CPU/GPU tensors (user's Stack Overflow research)

**Changes:**
- Text encoder: GPU fp16
- Transformer: GPU int8
- VAE: GPU fp32
- All on same device

**Result:** ❌ **FAILED - VRAM OOM during loading**
- Text encoder (14GB) + Transformer (30GB int8 with overhead) = 44GB
- Exceeded 32GB VRAM during transformer load

---

## Attempt 14: Text Encoder on CPU, Offload After Encoding

**Rationale:** Text encoder only needed once at start, can encode then offload

**Changes:**
- Load text encoder on CPU
- Load transformer on GPU (int8)
- Move text encoder to GPU temporarily for encoding
- Move back to CPU before inference

**Result:** ❌ **FAILED - VRAM OOM when moving text encoder**
- Transformer already using 30.5GB
- Can't fit 14GB text encoder even temporarily
- Int8 transformer has more overhead than expected

---

## Attempt 15: Encode on CPU, Move Only Embeddings (CURRENT)

**Rationale:** Don't move 14GB text encoder, only move tiny embeddings (few MB)

**Changes:**
```python
# Text encoder stays on CPU entire time
text_encoder = AutoModel.from_pretrained(...) # No .to(device)

# Encode on CPU
prompt_embeds = pipe.text_encoder(**prompt_inputs)[0]

# Move only the small embeddings to GPU
prompt_embeds = prompt_embeds.to(device)
```

**Result:** 🚀 **PENDING TEST**
- Memory usage:
  - GPU: ~30.5GB (transformer int8 only)
  - CPU: ~14GB (text encoder) + ~0.5GB (VAE)
- Should fit in 32GB VRAM
- VAE will decode on CPU (slow but works)

---

## Key Learnings

1. **CUDA Toolkit Issue:** RTX 5090 (Blackwell/compute_120) not supported by CUDA 12.6
2. **fp8 Limitations:** fp8 quantization libraries have poor support for newer architectures
3. **Int8 Overhead:** Int8 quantization has significant memory overhead beyond param size
4. **Device Mismatch:** Cryptic CUDA errors often mean CPU/GPU tensor mismatch
5. **Loading vs Inference:** Memory optimization APIs (offload, etc) often only help during inference, not loading
6. **Component-wise Loading:** Loading components individually is key for large models
7. **Embedding Transfer:** Can encode on CPU and transfer only embeddings to save VRAM

---

## Next Steps if Current Approach Works

If VAE decode is too slow on CPU:
1. After diffusion completes, offload transformer to CPU
2. Move VAE to GPU
3. Decode latents on GPU
4. This frees up 30GB for the <1GB VAE

Alternative: Find or create an fp16/fp8 variant of the model that's pre-quantized

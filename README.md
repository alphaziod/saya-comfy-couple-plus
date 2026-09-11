# Saya Comfy Couple Plus

**V1 · 1.0.0** — regional prompting and model routing for two-character ComfyUI workflows.

Give the scene and each character their own prompt. Couple combines the shared
scene with each person's identity and routes the resulting conditioning to that
person's region. MASTER defines the layout; COPY reuses it in later passes.
The V1 example includes an adult woman and an adult man sharing a book in an apartment.

## Features

- Two-character regional conditioning and SDXL Forge Couple attention.
- HiDream support through **Architecture E**: native masked conditionings without a MODEL attention wrapper.
- Deferred HiDream text encoding and a persistent conditioning cache.
- HiDream diffusion LoRA support, including the ComfyUI-GGUF patcher.
- Automatic, editable HiDream trigger injection into all three positive prompts.
- Independent Naturalize conditioning and late cleanup routing.
- Six automatic phase boundaries with review, checkpoint save/load and optional model offload.
- Regional detailer routing and optional hires/upscale passes.

## Installation

Use the Python environment belonging to your ComfyUI installation:

```bash
cd /path/to/ComfyUI/custom_nodes
git clone https://github.com/alphaziod/saya-comfy-couple-plus.git
```

Restart ComfyUI and refresh the browser. Keep only one installed copy of this
plugin: an older folder named `comfy_saya_couple` registers the same public node IDs.
ComfyUI supplies PyTorch and its normal runtime dependencies; `requirements.txt`
lists NumPy and Pillow. Do not replace ComfyUI's CUDA/ROCm PyTorch build.

The plugin's SDXL/HiDream conditioning nodes use ComfyUI itself. Additional nodes
have these requirements:

| Feature | External dependency |
| --- | --- |
| Crop detailers | ComfyUI-Impact-Pack |
| Detector providers in the example | Impact Pack / Impact Subpack with UltralyticsDetectorProvider |
| USDU bridge | ComfyUI_UltimateSDUpscale providing UltimateSDUpscaleCustomSample |
| GGUF HiDream MODEL | ComfyUI-GGUF |

Stock UltimateSDUpscale works with `structure_preservation = 0`. A nonzero value
requires an Identity Safe V2.1 backend exposing that input; the bridge reports an
explicit error if it is unavailable. That extension is not required for the public defaults.

### Full example workflow dependencies

The example retains the real multi-phase graph. To open all its nodes, install
these packs as well (including nodes that start bypassed):

- **rgthree-comfy**: group toggles, image comparers, switches and Power Puter.
- **ComfyUI-KJNodes**: Set/Get routing, constants, size helpers and LazySwitchKJ.
- **ComfyUI-Lora-Manager**: the five empty SDXL LoRA slots.
- **ComfyUI_JPS-Nodes**: sampler/scheduler settings and arithmetic.
- **ComfyUI-Crystools**: primitive float.
- **ComfyUI-Detail-Daemon** and **RES4LYF**: optional sampling modifiers.
- **WAS Node Suite**, **ComfyUI-WLSH-Nodes**, **ComfyUI-Image-Saver** and
  **ComfyUI-DaSiWa-Nodes**: upscale and image/save utilities.
- **efficiency-nodes-comfyui**: the final Naturalize sampler.
- **ComfyUI-EasyColorCorrector**: optional color correction.

Use a current ComfyUI/frontend supporting subgraphs, CustomCombo and QuadrupleCLIPLoader.
The example was checked against the locally installed ComfyUI 0.34 / frontend 1.45 family.
These example-only dependencies are not all required for a small custom Couple graph.
No model weights are distributed or downloaded by this project.

## Example Workflow

Download and open **[workflows/Ilust-Simple-V1.json](workflows/Ilust-Simple-V1.json)**.
This replaces the previous example; it is the single public V1 template.

Before queuing:

1. In **Models**, select your SDXL base and refiner checkpoints and compatible
   custom SDXL VAEs. Prefer the same checkpoint in both slots for a simple start.
2. Replace every `SELECT_...` placeholder on any branch you enable. Placeholders
   keep loader widgets present; they are not downloadable model names and will
   fail validation until you make a real selection.
3. Select an upscale model for the final output and enabled upscale passes.
4. Edit **Base Prompt**, **Person 1 Prompt**, **Person 2 Prompt**, **Negative Prompt**.
   Naturalize has its own four example prompts; update those too if you change identities.
5. The five **LoRA 1–5** slots are empty. Add your own compatible SDXL LoRAs if wanted.
6. Leave the optional passes off for a first check. After Phase 01, inspect the
   review image and select Continue to run the remaining phases.

All 13 numbered detailers, USDU passes, Detail Daemon, CFG Zero, epsilon scaling
and color correction start bypassed. **Enable HiDream** starts false. Phase
boundaries remain active so skipping a refinement still produces the checkpoint
required by the next phase. Hires and final upscale routes remain available;
bypass image-to-image refinement nodes to skip them, without bypassing LOAD/STOP nodes.

The VAE selectors default to **Main Model VAE**. The graph retains its custom VAE
inputs; because ordinary selectors may evaluate all connected inputs, choose
valid compatible VAEs for those slots even when the main VAE is selected.
For a detailer you enable, select a detector and SAM model, then use the matching
**Detailer N** group toggle. Assign them to faces, eyes, hands or other useful
regions. Every numbered slot starts inactive and has no personal detector selection.

## The four prompts

| Prompt | Responsibility |
| --- | --- |
| Base Prompt | Shared scene, interaction, framing, lighting and background |
| Person 1 Prompt | First adult's appearance, clothing and pose; left region by default |
| Person 2 Prompt | Second adult's appearance, clothing and pose; right region by default |
| Negative Prompt | Shared exclusions |

In SDXL, the Forge engine patches a cloned MODEL with the existing regional
attention logic. Each region receives Base + its person prompt. Geometry and
masks come from MASTER and are reused by COPY. Regional prompting guides
identity separation; image quality still depends on the model and prompts.

## HiDream phase

Open **Phase 03**, select a HiDream GGUF diffusion model and its compatible VAE,
then turn on **Enable HiDream**. The default off position uses a lazy image
switch: it carries the previous image forward without executing HiDream MODEL
loading or text encoding. Do not bypass the outer Phase 03 or its STOP node.

The deferred self-loader expects these standard encoder filenames in ComfyUI's
`text_encoders` search paths:

```text
clip_l_hidream.safetensors
clip_g_hidream.safetensors
t5xxl_fp8_e4m3fn.safetensors
llama_3.1_8b_instruct_fp8_scaled.safetensors
```

Supply compatible files. The unused QuadrupleCLIPLoader retained in Models is not
connected to the Phase-01 prompt encoders. Wiring a CLIP directly into HiDream
COPY is supported for custom graphs, but loads that upstream CLIP before COPY's
cache check; leave it disconnected to preserve the no-TE-load cache-hit path.

Architecture E builds native per-region HiDream conditionings. MODEL and
conditioning travel separately:

```text
HiDream MODEL → HiDream LoRA Loader → ModelSamplingSD3 → Phase 03 sampler MODEL
Base/P1/P2 HiDream conditionings → Couple E COPY → Phase 03 sampler positive
Negative HiDream conditioning → Couple E COPY → Phase 03 sampler negative
```

### HiDream LoRA and trigger prompt

The settings panel exposes `enabled`, `lora_name`, `strength_model` and an editable
multiline `trigger_text`. Public defaults are **false / none / 1.0 / empty**.
The dedicated loader uses ComfyUI's model-only LoRA API and preserves the GGUF
patcher subclass. It rejects non-HiDream models and files with no matching MODEL weights.

For a LoRA whose documented trigger is `exampleStyle`, set that text in the panel:

```text
Base HiDream:     exampleStyle, <Base original>
Person 1 HiDream: exampleStyle, <Person 1 original>
Person 2 HiDream: exampleStyle, <Person 2 original>
Negative HiDream: <Negative original>
```

The trigger is prepended before encoding and Couple regional assembly. Simple
comma-separated duplicates are avoided. SDXL and Naturalize prompts are unchanged.
Clear the text to remove automatic injection while keeping the LoRA. Disable the
panel, select none or set strength to zero to disable both patching and injection.
A trigger manually written in an original prompt is not removed by disabling the panel.

### Lazy loading, cache and memory

- Phase 01 encodes SDXL and Naturalize only; HiDream remains deferred.
- Phase 03 cache hit: read conditionings; no self-loaded HiDream text encoder.
- Cache miss: load the quad CLIP once, encode four prompts, save the CPU
  conditionings and unload the self-loaded TE before diffusion sampling.
- The key includes the **final injected texts**, TE file fingerprints and encode
  recipe. Diffusion-only LoRA filename/strength do not invalidate text conditioning.
- Public STOP nodes enable `unload_after_phase`: ComfyUI's existing model manager
  offloads loaded weights after each accepted phase boundary. Patchers needed for
  later phases remain reusable on CPU; the disk conditioning cache survives.
- Phase 06 performs the existing final unload and clears the cross-phase payload.
  Existing private workflows retain the previous boundary policy unless they opt in.

This controls accelerator residency, not a promise to erase every cached CPU
object. ComfyUI remains responsible for model caching and available memory.

## Validation and development

V1 includes CPU integration tests for native/GGUF LoRA patches, trigger injection,
cache hit/miss behavior, lazy encoding and graph structure. No full generation or
image-quality benchmark is required to run these checks:

```bash
cd /path/to/ComfyUI
/path/to/comfy-python /path/to/saya-comfy-couple-plus/tests/validate_hidream_lora.py
python /path/to/saya-comfy-couple-plus/tests/validate_public_workflow.py
```

For the first command, use ComfyUI's Python environment and install ComfyUI-GGUF.
You can alternatively set `COMFYUI_PATH`. The public workflow test uses only the
Python standard library. [ARCHITECTURE.md](ARCHITECTURE.md) explains implementation
boundaries; [HIDREAM_LORA_REPORT.md](HIDREAM_LORA_REPORT.md) records the LoRA/cache design.

Active development continues. Include your ComfyUI version, enabled node packs,
model family and a minimal workflow when reporting an issue. Existing public
node IDs and output ordering are preserved in V1.

## Credits and licensing

The Forge Couple engine derives from Haoming02's sd-forge-couple. Its GPL-3.0
license and notices are retained in [forge/LICENSE](forge/LICENSE) and the Forge
source files. Third-party node packs and model weights have their own licenses.

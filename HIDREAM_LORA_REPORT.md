# HiDream LoRA — V1

## IMPLEMENTATION

`SayaHiDreamLoraSettings` emits a lightweight MODEL patch configuration and the
effective trigger STRING. `SayaHiDreamLoraLoader` clones and patches only a
HiDream MODEL using ComfyUI's native LoRA API. The GGUF subclass is preserved.
Wrong model families and LoRAs producing zero compatible MODEL patches fail explicitly.

## WORKFLOW CHANGES

The public `workflows/Ilust-Simple-V1.json` contains the settings panel and loader
inside Phase 03. Defaults: disabled, none, strength 1.0, empty trigger. A separate
lazy image switch disables the entire HiDream refinement by default.

## TRIGGER PATH

Settings → HiDream COPY → prepend to Base/P1/P2 original strings → cache-first
quad-CLIP encode → existing Couple E regional conditioning assembly.
Negative, SDXL and Naturalize are unchanged. Clearing the trigger preserves the
LoRA patch; disabling LoRA also disables automatic trigger injection.

## LORA MODEL PATH

HiDream GGUF → HiDream LoRA Loader → ModelSamplingSD3 → Phase 03 KSampler.
Couple E separately supplies positive/negative conditioning; it does not patch MODEL.

## CACHE BEHAVIOR

The unchanged key format hashes the final injected strings, TE fingerprints and
encoder recipe. Changing effective text invalidates the cache. Diffusion-only
LoRA identity/strength do not. Empty-trigger original-text cache entries remain usable.
On a hit no self-loaded TE is created; a miss encodes, saves and unloads the TE.

## VALIDATION

CPU integration tests exercise enable/disable, empty/duplicate/changed triggers,
unchanged Negative and SDXL text, Phase-01 deferral, Phase-03 cache reuse, native
and GGUF patcher cloning, and quantized-weight application/restoration. Local
format verification matched all 496 target names and dimensions of an available
HiDream LoRA to a HiDream Q5_K_M GGUF, without loading the full diffusion model.
No model weights, private filenames or prompts are included in the public example.

## STATUS

V1 implementation with a sanitized public example. Validation covers code, CPU
integration and graph structure; full GPU generation and visual quality depend
on the user's installed models and chosen settings.

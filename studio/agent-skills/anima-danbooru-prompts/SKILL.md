---
name: anima-danbooru-prompts
description: Write prompts and settings for the Anima model family (CircleStone Labs / Comfy Org, 2B, Danbooru-tag and natural-language trained, Qwen3 text encoder) in ComfyUI or Saya Studio. Use when the checkpoint is Anima (base, aesthetic or turbo) or the user mentions it.
---

# Anima prompts

Answer in the user's language. Default to SFW; adult content only for clearly adult characters.
Source: the official model card `huggingface.co/circlestone-labs/Anima` — re-check it, versions move.

## 1. Files (ComfyUI, native support)

- Diffusion model `anima-base-v1.0.safetensors` (or aesthetic/turbo) → `models/diffusion_models`
- Text encoder `qwen_3_06b_base.safetensors` → `models/text_encoders`
- VAE `qwen_image_vae.safetensors` (Qwen-Image VAE) → `models/vae`
- Not an SDXL checkpoint: SDXL LoRAs and SDXL VAEs do not apply.

## 2. Settings

Sampler, steps, CFG, scheduler and resolution are the user's choice (setups differ, no convention):
ask which workflow holds their good settings, read them from it,
never modify that workflow, never prescribe.

## 3. Prompt

- Positive prefix: `masterpiece, best quality, score_7, safe, ` (safety tag: `safe`, `sensitive`,
  `nsfw`, `explicit`). For the **Aesthetic** variant leave `score_*` out of both prompts.
- Order: `[quality/meta/year/safety] [1girl/1boy…] [character] [series] [artist] [general tags]`.
- Lowercase tags with spaces, no underscores — except `score_*`. When Danbooru and Gelbooru differ,
  use the Gelbooru tag.
- Artists need `@`: `@artist name` (without it the effect is very weak). Artist knowledge stops at
  the training cutoff (Sept. 2025); renamed artists may not work.
- Time: `year 2025`, `year 2024`…, or `newest, recent, mid, early, old`.
- Natural language works and mixes with tags in any order; give at least two sentences when you
  use it (very short prompts behave unpredictably). For several characters, name each with series,
  then describe each one's appearance.
- Weights need larger values than SDXL: `(chibi:2)` where SDXL would use 1.2.
- Negative: `worst quality, low quality, score_1, score_2, score_3, artist name, blurry, jpeg
  artifacts, chromatic aberration`.

## 4. Limits

Weak at realism (by design) and at long text in images (single words are fine). Without artist or
quality tags the base style is plain. Short or vague prompts can drift into unwanted content: be
explicit, including the safety tag.

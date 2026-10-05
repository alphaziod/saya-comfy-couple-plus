# comfy_saya_couple

ComfyUI custom node pack for illustrations rendered in six separately queued
phases, with an optional two-person ("Couple") regional pipeline running
alongside the single-subject ("Solo") path. Each phase saves a checkpoint
image; the next phase reloads it, so a long pipeline can be reviewed, redone
or resumed without keeping models, conditionings or latents in memory between
phases.

Loaded by ComfyUI through `__init__.py`, which exposes `NODE_CLASS_MAPPINGS`
and `NODE_DISPLAY_NAME_MAPPINGS` from `registry.py`, registers the HTTP routes
and serves `web/`.

## Installation — no ComfyUI core modification

Since M1 (2026-10-05) the Couple engine (`src/engine/dual_attention.py`: forced attn2 path, `main_locked_delta`
fusion, dynamic P1/P2 ownership v22) is injected into MODEL_1 with the official `ModelPatcher.add_object_patch`
API (`src/nodes/saya_dual_attention.py`): every cross-attention block of the clone is replaced, for the duration of
the sampling, by a shallow copy whose `attn2` runs the engine when the `saya_dual` payload is present and the stock
cross-attention otherwise. ComfyUI installs the copies in `patch_model()` and restores the originals in
`unpatch_model()`; the ON_DETACH callback restores them too when a clone is dropped without unpatching.
**No file under `comfy/` has to be patched.**

Fresh install check (reads the real sources, no import): `python tools/check_comfyui_compat.py --comfyui <root>`.
Required: a ComfyUI whose `BasicTransformerBlock.forward` calls `self.attn2(n, context=..., value=...,
transformer_options=...)` and whose `ModelPatcher` has `add_object_patch` / `object_patches_backup` / `ON_DETACH`
(every 2026 release), plus the `MultiMaskCouple` custom node (Phase 1 Sampler 2 and the reconstruct passes).
Optional, needed at run time only: Impact Pack + subpack (Detailers), UltimateSDUpscale with
`tools/core_patches/ultimate_sd_upscale_saya_couple_crop.patch` (per-tile masks in USDU Couple), the AMD VRAM patch.

## Rules the code relies on

- **Data-only persistence.** Checkpoints and imprints hold plain JSON and PNG
  pixels only: never MODEL, CLIP, CONDITIONING, LATENT, MASK or hooks. Every
  runtime object (regional conditioning, coupling patch, mask raster) is
  rebuilt locally in the phase that uses it, from the imprint plus that
  phase's own prompt/geometry data.
- **Model isolation.** A MODEL is `clone()`d before any patch (LoRA, attention
  couple). There is no module-level mutable runtime state.
- **Current resolution.** Crops, tiles and mask rasters are rebuilt for the
  resolution of the pass that uses them, never reused from an earlier phase.
- **PERSON_2 stays absent when absent.** The couple imprint never fabricates a
  second person from MAIN, and never treats an empty string as "present".
- **Couple vs Solo.** Couple Mode ON regionally couples MAIN + PERSON_1 +
  PERSON_2 (two independent per-model attention-couple patches, masks and
  strengths applied per region). Couple Mode OFF (Solo) uses MAIN + PERSON_1
  globally, skips the region split and the coupling patch entirely, and
  returns the model(s) unpatched.

## Node groups (see `registry.py` for the exact class -> display name table)

| group | files | role |
|---|---|---|
| Phase Load/Stop/Review | `src/nodes/image_phases.py` | queues and checkpoints phases 1-6 |
| Couple imprint | `couple_imprint.py`, `couple_imprint_v2.py`, `couple_imprint_resolve.py` | pack/unpack/resolve the DATA-ONLY couple imprint (`saya.couple.imprint`, v1 legacy / v2 current) |
| Couple reconstruction | `couple_reconstruct.py`, `couple_hidream_reconstruct.py`, `region_masks.py` | rebuild regional conditioning, masks and identities for Phase 2 (SDXL/PPM) and Phase 3 (HiDream) from a resolved imprint |
| MAIN prompt | `main_prompt.py` (+ `data/main_prompt_tags.json`), `background_picker.py` (+ `data/backgrounds*.json`) | `SayaMainPrompt` = prefix (verbatim, LoRA triggers untouched) + background from the stock (locked / random / free, shared history, ultra) + one tag per drop-down menu + extra (verbatim); `SayaBackgroundPicker` = the background alone; `SayaMainPromptFR` = the same node with French labels (inputs, categories, history, tag menus; tags sent in English) |
| Couple context (P-A, 2026-10-05) | `couple_context.py` | `SayaCoupleContextLoad` = Resolve/Load + CheckpointIdentities + Retarget in one node: resolves and validates the imprint once per phase and hands `SayaCoupleReconstruct` a `SAYA_COUPLE_CONTEXT` per model (dict: imprint, identities, source chain, retarget, phase, debug); `SayaCoupleContextInspect` prints it. The historic nodes still work. |
| Deprecated (P-C) | `deprecated.py` | 13 types kept loadable under `saya/deprecated` (one warning per process), see `registry.DEPRECATED` |
| Regional coupling | `saya_multi_couple.py`, `saya_attention_couple.py`, `src/ppm_vendor/` | apply the per-model attention-couple patch; `ppm_vendor` is a vendored copy of pamparamm/ComfyUI-ppm |
| HiDream refiner | `hidream_lora.py`, `hidream_safe_scale.py`, `hidream_shadow_control.py` | HiDream-only LoRA loader, a deterministic token-budget resolution ceiling, and a luminance-based refine mask |
| Duo geometry / upscale | `src/duo_geometry/` | per-pass latent shape, tiled upscale, USDU pass |
| Resolution / upscale calculators | `saya_resolution_scale.py`, `saya_upscale_mode.py`, `hires_fix_resize.py`, `hires_fix_target.py` | resolution buckets, upscale presets, the single coherent Hires Fix upscale |
| Masks | `saya_split_mask.py`, `ppm_masks.py` | exact-complement binary split, PPM layout masks |
| Finishing | `chroma_anchor.py`, `naturalize_postprocess.py`, `detailer.py` | chroma cap against a reference, fine grain/dither, identity-safe SEGS detailing |
| Misc | `sampling_config.py`, `warmup_gate.py` | sampler config with live COMBO types, discarded warmup frame |
| Lazy selects | `couple_reconstruct.py` (`SayaLazyModelSelect`, `SayaLazyBooleanSelect`), `image_phases.py` (`SayaLazyCheckpointLoader`) | evaluate only the chosen branch, so an unused Model 2 or checkpoint is never loaded |

## Phase sequence

1. base render (Couple: regionally coupled MAIN + P1 + P2; Solo: MAIN + P1)
2. tiled upscale (USDU)
3. HiDream refiner (its own model, so phase-1 shared nodes stay muted)
4. pre-detail (upscale by model)
5. detailers, which always restart from phase 4
6. final upscale, Naturalize post-process and save

Workflow nodes are tagged with `properties.saya_phase` (and optionally
`saya_shared_base`). The frontend controller keeps only the nodes of the
active phase enabled. Each `Load` node reads the previous phase's *validated*
checkpoint under `output/<checkpoint_root>/`. Each `Stop` node writes a
*candidate*, then promotes it and emits `saya_image_phase_complete` with the
next phase number (`0` = finished).

## Layout

| path | role |
|---|---|
| `registry.py` | the node table (class -> display name) |
| `src/duo_geometry/` | per-pass geometry: latent shape, tiled upscale, USDU pass |
| `src/nodes/` | node implementations (see the table above) |
| `src/ppm_vendor/` | vendored attention-couple implementation (see its own README) |
| `src/services/image_phases.py` | checkpoint files: candidate -> validated promotion under `promote_lock`, manifests (v0 legacy / v1), PNG + JSON sidecar |
| `src/routes/image_phases.py` | `/saya/image-phases/*` routes (validate, redo), file work runs off the event loop |
| `web/image_phase_controller.js` | queues phases 1-6 in sequence by switching node modes per phase, and injects `checkpoint_root` |
| `web/saya_node_ux.js` | hides widgets that have no effect in the current node state (hidden, never removed, so values still serialize) |
| `web/saya_hub_ui.js` | presentation-only labels, minimum sizes and colors for the MAIN USDU / Detailers / Phase 6 / Couple Mode hubs; never touches links, values or positions |
| `web/saya_resolution_ratio_filter.js` | small UI helper for the resolution presets |

## Tests

```bash
PYTHONDONTWRITEBYTECODE=1 /path/to/ComfyUI/.venv/bin/python tests/run_all.py
```

`run_all.py` runs every `tests/test_*.py` and every `tests/*.cjs` (Node VM
tests of the frontend, including mutation checks). The workflow tests expect
the workflow at `../WORKFLOW/Ilust simple.json` relative to the pack.
`tests/` is not deployed to `custom_nodes`.

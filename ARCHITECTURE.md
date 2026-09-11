# comfy_saya_couple — Architecture (for maintainers)

This is the "how it hangs together and why" document. For the public surface see
`README.md` and the public example in `workflows/Ilust-Simple-V1.json`.

---

## 1. Plugin entry

`VERSION.txt` identifies the public release as 1.0.0.

`__init__.py` is what ComfyUI imports. It:

1. re-exports `NODE_CLASS_MAPPINGS` / `NODE_DISPLAY_NAME_MAPPINGS` from
   `registry.py` (the only place ComfyUI reads for node discovery),
2. calls `src.routes.image_phases.register_image_phase_routes()` (registers the
   `/saya/image-phases/*` aiohttp endpoints on `PromptServer.instance`),
3. sets `WEB_DIRECTORY = "./web"` (loads `web/image_phase_controller.js`).

`registry.py` imports every node class and builds the two dicts. **Node IDs,
display names, socket names, output order and categories are the compatibility
contract** — changing them breaks saved workflows.

## 2. Package layout

```
forge/                 SDXL Forge-Couple attn2 engine (GPL-3.0, vendored + adapted)
src/nodes/             ComfyUI node classes, grouped by feature
src/nodes/detailers/   crop-aware Impact Pack detailer wrappers
src/nodes/regional_attention/  legacy per-block cross-attention engine
src/routes/            aiohttp HTTP endpoints
src/services/          reusable logic with no ComfyUI node registration
web/                   the frontend extension
```

Import direction is strictly `registry → nodes → services`; `services` never
imports a node module. `forge/` and `regional_attention/` do not import each
other. No import cycles.

## 3. The two couple engines

| | Legacy: `SayaComfyCouple` | Current: `SayaComfyCoupleForge` (+ Copy) |
|---|---|---|
| File | `src/nodes/couple_conditioning.py` + `regional_attention/` | `forge/node.py` + `forge/attention_couple.py` |
| Technique | replaces every attn2 block on the cloned UNet with a Python `RegionalAttentionReplacement` that recomputes regional attention + a "standard attention" blend | one `set_model_attn2_patch` + one `set_model_attn2_output_patch` (Forge style): the input hook rewrites k/v with the concatenated regional conds; the output hook composites by a resized ownership mask |
| Architectures | SD1.5 / SDXL / ANIMA transformer | SDXL (any model ComfyUI patches) |
| Status | kept for old workflows; **not modified** — numerically sensitive, deeply defensive | the one used by current workflows |
| Contact band | no | yes — a MAIN-only smoothstep bump on the seam (`build_forge_routing_masks`), plus a coarse-resolution MAIN share (`coarse_main_strength` ≤ `coarse_max_tokens` tokens) |

`forge/node.py` structure: `normalize_couple_config` (schema version 10,
`SAYA_COUPLE_CONFIG`) → `build_character_masks` (soft complementary ownership) →
`build_forge_routing_masks` → `AttentionCouple.patch_unet` per connected model
(`main`, `dual_sampling`, `support_1..3`; deduplicated by `id(model)` in
`_patch_branches`) → returns patched models + `positive_final` (the MAIN list,
or concatenated in SOLO) + `detailer_positive` (per-person, pixel masks) +
`negative` + the two public pixel masks + `couple_config`.

`SOLO` (couple attention off) and `FALLBACK` (a person/negative missing, or
multi-entry conds) degrade to plain concatenated conditioning lists so nothing is
silently dropped.

## 4. Forge attn2 patch (`forge/attention_couple.py`)

* `patch_unet` builds `mask_fine` (stacked, normalised to sum 1 per pixel) and,
  when the coarse-MAIN share is on, `mask_coarse` (MAIN gets a bounded slice of
  the private budget at ≤ `coarse_max_tokens` tokens).
* `attn2_patch` (input): asserts `k == v` (a hard Forge precondition — the return
  tuple deliberately uses `ks` for **both** key and value), chunks q/k by
  `cond_or_uncond`, LCM-repeats token dims, concatenates the regional conds onto
  the cond chunk, pads an odd batch.
* `attn2_output_patch` (output): picks `mask_coarse`/`mask_fine` by the live token
  count; if a `saya_couple_crop` context is present it slices the mask to the
  detailer crop; if the pass is much smaller with no crop context it routes the
  base branch only; then `get_mask` (Forge resize formula in
  `attention_masks.py`) downsamples the mask to the token grid and the branch
  outputs are mask-weighted and summed.

**Do not "optimise" this file.** The `k == v` assumption and the padding/trim
dance are load-bearing.

## 5. HiDream — Architecture E

`src/nodes/couple_conditioning_hidream.py` + geometry in
`regional_attention/hidream_masks.py`.

* **No MODEL socket.** The node only transforms CONDITIONING. Wire the HiDream
  MODEL through the optional HiDream LoRA loader and `ModelSamplingSD3` into `KSampler.model`, and this
  node's `positive` into `KSampler.positive`. The sampler
  (`comfy/samplers.py:_calc_cond_batch`) runs one forward per regional entry and
  composites by the per-entry `mask`.
* `_entry_list` **family-guards** every input: it must be a list of
  `[rank-3 T5 tensor, dict]` where the dict has tensor `pooled_output` and
  `conditioning_llama3`; otherwise a hard `RuntimeError` (silent degradation is
  the project's #1 historical risk).
* Context modes: `main_plus_person_concat` (default; token-concat the separately
  encoded MAIN + person; pooled = MAIN), `person_only`, `main_plus_person_reencoded`
  (truly re-encode "`<main>. <person>`" — needs the `clip` + `*_text` sockets).
* Masks: regional masks on CONDITIONING are **latent-space** (`routing_masks`,
  `H_px/8`); the public `MASK` outputs and detailer masks are **pixel-space**
  (`pixel_masks`). These two spaces are kept strictly separate on purpose.
* Optional MAIN contact band (`include_main_contact`) and unmasked global MAIN
  entry (`add_global_main_entry`).

### HiDream LoRA path and trigger injection

`SayaHiDreamLoraSettings` owns the visible `enabled`, `lora_name`,
`strength_model` and multiline `trigger_text` controls. It emits a lightweight
LoRA config and the effective trigger STRING. Disable, `none`, or zero strength
emits an empty trigger; clearing only the text keeps the diffusion LoRA active.
The separate settings node avoids making text encoding depend on MODEL loading.

In `workflows/Ilust-Simple-V1.json`, inside **Phase 03**:

* MODEL: `UnetLoaderGGUF (10147) → SayaHiDreamLoraLoader (12422)
  → ModelSamplingSD3 (10139) → KSampler (10143)`.
* Text: settings `(12421).hidream_trigger → Couple HiDream COPY (10383)`.
  COPY reads the original Phase-01 bundle, prepends the trigger to Base/P1/P2,
  then materializes the four conditionings cache-first. Negative is unchanged.
  The regional conditioning assembly follows the encode; no regional maths change.

The loader uses `comfy.sd.load_lora_for_models(model, None, lora, strength, 0)`.
The installed `GGUFModelPatcher.clone()` preserves its subclass; quantized weights
receive patches through `patch_weight_to_device`, and `GGMLLayer.get_weight`
applies them after dequantization. No CLIP is patched. Non-HiDream MODEL inputs
and LoRA files that produce zero compatible MODEL patches fail explicitly.
The original MODEL patcher is unchanged; all subsequent phases keep their routes.

The trigger is configurable, never hardcoded in runtime Python. A trigger already
at the start is not repeated; a simple comma-delimited occurrence elsewhere is
moved to the start. An empty trigger leaves the original strings verbatim.
The four central `SayaDualCLIPTextEncode` nodes and the Phase-01 bundle stay
unchanged, so neither SDXL nor Naturalize receives the automatic trigger.

**Cache interaction:** `make_key` receives the final injected strings. Changing
the effective text changes the key; identical final texts and TE configuration
reuse the cache. LoRA filename/strength are deliberately absent from that key:
this loader patches diffusion only. Old cache entries with original text remain
usable when the trigger is empty. HIT never self-loads the TE; MISS keeps the
existing encode → persist → TE unload sequence before diffusion sampling.
Legacy pre-encoded inputs remain supported with an empty trigger; with a trigger,
COPY requires original text and re-encodes cache-first rather than editing tensors.

### Deferred encode sequence

`SayaComfyCoupleHiDreamCopy.run_copy`:

1. `_has_entries` on the four conditioning inputs. If all four are real,
   non-empty conditionings and no trigger is supplied → legacy path, unchanged.
2. Otherwise (phased workflow: `SayaDualCLIPTextEncode` left them empty and
   `get_hidream_payload()` returns four empties) → read the four prompt strings
   from `prompt_bundle_json` (the Phase-1 bundle, forwarded by
   `SayaImagePhase3Load.positive_prompt`) and call
   `materialize_hidream_conditionings`:
   * `hidream_cache.make_key(texts, clip_files, {"recipe": "quad_clip_v1"})`;
   * `hidream_cache.load(key)` → HIT: return the four conditionings, **no CLIP**;
   * MISS: `clip` socket if wired else `_load_quad_clip(_HIDREAM_TE_DEFAULTS)`
     (mirrors the stock `QuadrupleCLIPLoader`); encode the four prompts back to
     back; `hidream_cache.save(key, conds)`; if self-loaded,
     `_unload_self_loaded_clip` removes just that CLIP patcher from
     `comfy.model_management.current_loaded_models` + `gc` + `soft_empty_cache`.
3. The rest of `_run` (regional entries, contact band, pixel masks, detailer
   positives) is identical for both paths.

## 6. HiDream conditioning cache (`src/services/hidream_cache.py`)

* Dir: `folder_paths.get_user_directory()/saya_hidream_conditioning_cache/`
  (persists across restarts); fallback to a package-local `_cache/` if
  `folder_paths` is unavailable.
* `make_key`: `sha256` of canonical JSON
  `{v, texts:[base,p1,p2,neg], clip:[(basename,size,mtime)×4], cfg}`.
* `save`: `torch.save` of `{version, saved_at, conditionings:[4 CPU lists]}` to
  `<key>.pt.<pid>.tmp` then `os.replace` (atomic same-fs). `_conditioning_to_cpu`
  moves every context tensor and every tensor-valued metadata entry
  (`pooled_output`, `conditioning_llama3`, …) to CPU.
* `load`: `torch.load(map_location="cpu", weights_only=False)`; any exception or
  a payload missing `conditionings` of length 4 → treated as a MISS. `os.utime`
  touches the file for LRU.
* `_prune`: keep the newest 24 `.pt` by mtime.
* Concurrency: a module `RLock`; ComfyUI runs one prompt at a time.

## 7. SDXL / Naturalize / HiDream separation

`SayaDualCLIPTextEncode` (`src/nodes/text_and_model_routing.py`) has three
independent branches, each with its own CLIP, and they never convert into one
another:

| Output slot | Input | Behaviour |
|---|---|---|
| 0 `conditioning` | `clip` + `text` | SDXL, encoded eagerly |
| 2 `naturalize_conditioning` | `naturalize_clip` + `naturalize_text` | encoded eagerly (same SDXL CLIP → no model transition) |
| 1 `hidream_conditioning` | `hidream_clip` + `text` | **deferred**: if `hidream_clip` is unwired → `[]` (phased workflow); if wired → an inert `_HiDreamEncodeRequest` token resolved by the Couple MASTER (see §8) |

Naturalize conditioning is fed to the MASTER's `naturalize_*` sockets; the MASTER
builds a second Forge branch (main model only) and stores
`(model, positive, negative)` in `couple_runtime._STATE["naturalize"]`; Phase 6
reads it via `SayaImagePhase6Load → get_naturalize_payload()`. Naturalize text
never enters the HiDream branch and vice-versa.

## 8. The deferred HiDream token (legacy in-Phase-01 path)

When a workflow *does* wire `hidream_clip` into the four prompt nodes,
`SayaDualCLIPTextEncode` returns `_HiDreamEncodeRequest` (a plain object — **not**
a `list` subclass, so ComfyUI's RAM-pressure output-cache scanner does not
iterate it; it exposes `_comfy_cache_tensors() -> []`). `SayaComfyCoupleForge.run`
calls `resolve_hidream_conditioning()` on its four HiDream inputs at the top of
`run()`. ComfyUI cannot schedule the MASTER until all four prompt nodes have
produced outputs, so the first resolve flushes the whole batch → the quad CLIP
loads **once** instead of ping-ponging with the SDXL CLIP.

The current phased workflow doesn't use this path (the wire is cut and the encode
happens in Phase 3), but the machinery is kept for compatibility.

## 9. The phase state machine

`src/nodes/image_phases.py` (nodes) + `src/services/image_phases.py` (plumbing).

* Phases 1..6, each a self-contained ComfyUI prompt run.
* `SayaImagePhaseCheckpointLoad` / `…Stop` are the general nodes;
  `SayaImagePhaseNLoad` / `SayaImagePhaseNStop` are hidden fixed-phase
  subclasses (`_FixedPhaseLoad` / `_FixedPhaseStop`, `PHASE = N`). `Phase3Load`
  appends the four HiDream CONDITIONING outputs; `Phase6Load` appends the three
  Naturalize outputs.
* `save_candidate` → `image_tensor_to_png_bytes` (manifest embedded as a PNG text
  chunk) → `_atomic_write_bytes` (mkstemp in the same dir, fsync, `os.replace`,
  dir fsync). `promote_candidate` moves candidate → validated with `.rollback.*`
  backups and full rollback on failure. Everything is crash-safe.
* `checkpoint_directory` forces `checkpoint_root` to be **relative and under
  ComfyUI's output dir** (rejects absolute paths and `..`).
* `SayaImagePhaseCheckpointStop.stop` writes/promotes, then: keeps caches warm
  between phases, `unload_everything()` **only** at Phase 6, `clear_master_payload()`
  at Phase 6, and `emit_phase_complete` (a `PromptServer.send_sync` event) to
  kick the frontend.
* `SayaImageGenerationReview` (Phase 1, `OUTPUT_NODE`) saves the candidate and
  returns a `ui.saya_review` payload the frontend turns into the Continue /
  Restart popup.
* Settings-only nodes (`SayaImageModelHubSettings`, `SayaImageVAERouteSettings`)
  return `()` — they only hold widget values in the workflow.

## 10. Backend routes (`src/routes/image_phases.py`)

`POST /saya/image-phases/{validate,redo,unload,status}`. `validate` promotes the
candidate for the given phase; `redo` discards it; `status` returns checkpoint +
memory info; `unload` is a deliberate **no-op stub**. All errors are caught and
returned as HTTP 400 JSON — a route never 500s.

## 11. Frontend (`web/image_phase_controller.js`)

* Registers on `SayaImageGenerationReview` (`beforeRegisterNodeDef` +
  `loadedGraphNode`, guarded by `node.__sayaReviewInstalled`).
* Monkeypatches (guarded by `app.__sayaImageAutoNoStartNode`):
  * `app.graphToPrompt`: for the active phase, set each phase-tagged node's mode
    (ACTIVE / NEVER / BYPASS) from `node.properties.saya_phase` /
    `saya_shared_base` / `saya_original_mode`, then restore after the prompt is
    built.
  * `app.queuePrompt`: a user-initiated queue (not `internalQueue`) resets the
    sequence to phase 1.
* Reacts to the `saya_image_phase_complete` event (`scheduleAfterPhase`) and to
  `execution_error` / `execution_interrupted` (re-arm from phase 1). A
  `sequenceGeneration` counter invalidates stale timers and popups.
* Review popup: **Continue** → `/validate` + queue phase 2; **Restart** →
  `/redo` + new master seed + fast re-queue of phase 1.

## 12. Crop-aware detailers (`src/nodes/detailers/`)

`schemas.build_detailer_input_schema` (shared Impact Pack schema, +`max_retries`
for the retry node) → `standard.py` / `retry.py` build a frozen `DetailerOptions`
→ `pipeline.process_detected_regions` iterates segments →
`segment_processing.clone_models_for_segment` attaches
`transformer_options['saya_couple_crop']` (crop region + full image size) so the
Forge / regional attn2 output hook re-maps ownership to that crop instead of
squeezing the whole-image gradient →
`segment_processing.enhance_segment` calls Impact Pack `core.enhance_detail`,
retries on `detailer_hook.should_retry_patch`, pastes with a gaussian-blurred
mask. `runtime.initialize_impact_pack_runtime` locates the loaded Impact Pack
module and injects `core / utils / wildcards / SEG / …` into this package's
`runtime` module namespace (Impact Pack is not importable as a normal package).

## 13. Model & VRAM lifecycle

The pack cooperates with ComfyUI's model manager. The public template sets
`unload_after_phase=True` on Phase 02–06 STOP nodes; the existing
`unload_everything()` offloads accelerator-resident weights at the boundary.
The optional input defaults false for older workflows. Phase 06 always unloads
and clears the cross-phase payload. Model patchers can remain cached on CPU for
later reuse; this is not wholesale deletion of ComfyUI's execution cache.

The deferred HiDream self-loader unloads only its own text encoder after a miss.
The disk conditioning cache survives phase boundaries. Phase 03 has a default-off
`LazySwitchKJ` carrying the previous image around the entire HiDream branch.
`graphToPrompt` aliases the unused input of literal LazySwitchKJ switches to the
selected input in the API prompt. This prevents ComfyUI's recursive validation
from demanding model placeholders on disabled branches. Dynamic boolean links
are unchanged, and both branches remain wired in the saved workflow.

## 14. Shared helpers (`src/services/`)

| Module | Used by |
|---|---|
| `conditioning.py` | `copy_conditioning`, `describe_conditioning_error` — one definition for both couple nodes. |
| `couple_runtime.py` | the single in-memory `_STATE` dict; Naturalize (Phase 6) + legacy HiDream (Phase 3) payloads; replaced on every MASTER eval, cleared at Phase 6. |
| `hidream_cache.py` | the on-disk encoded-conditioning cache (§6). |
| `models.py` | `resolve_registered_model_path` (handles `%20` vs space, case-insensitive fallback), `load_vae_or_fallback`, `build_model_choice_list`. |
| `image_phases.py` | all filesystem / manifest / memory / event plumbing for phases. |

## 15. Environment flags

* `SAYA_COUPLE_DEBUG=1` — verbose couple / Forge / regional-attention / detailer
  traces. All heavy trace strings are guarded so `.item()` GPU syncs are only
  paid when the flag is on.
* No other env config. The HiDream TE filenames default in
  `couple_conditioning_hidream._HIDREAM_TE_DEFAULTS` and can be overridden by
  wiring a `QuadrupleCLIPLoader` into `SayaComfyCoupleHiDreamCopy.clip`.

## 16. Things a maintainer must not change casually

* Any `NODE_CLASS_MAPPINGS` key, `RETURN_TYPES` / `RETURN_NAMES` order, socket
  name, or `CATEGORY` — the workflow compatibility contract.
* `SAYA_COUPLE_CONFIG` schema / `CONFIG_VERSION` (10).
* `forge/attention_couple.py` maths and the `k == v` precondition.
* `regional_attention/*` maths (the legacy engine — kept working, not optimised).
* `hidream_cache.make_key` format (changing it silently invalidates every user's
  cache — one extra MISS per prompt set).
* The phase manifest schema, checkpoint filenames, and the `output/image/checkpoints`
  layout (the frontend and disk state depend on them).
* The frontend monkeypatch guards.

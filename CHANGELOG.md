# Changelog

## 2.1.3 — 2026-10-08

### Background Picker

- **80 new backgrounds marked `unsafe`** in the stock (`"unsafe": "figures"` 48, `"captivity"` 22, `"gore"` 10): places
  whose scenery itself carries figures, captivity or gore. Like the fairground rides, they are kept out of random draws
  unless the `unsafe` switch is on (Picker, Main Prompt, Prompt MAIN FR); the switch tooltips now name all four kinds.
- Locked seeds keep their background (same redraw rule as 2.1.2). Test `background_picker_unsafe` covers a marked entry.

## 2.1.2 — 2026-10-08

### Background Picker

- **Fairground rides are out of random draws by default.** Backgrounds whose name or main place is a carousel, a
  fairground or an amusement park (33 in the stock) are no longer drawn by `all` or by a category. They stay in the stock.
- **New `unsafe` switch** (off by default) to draw them too: `unsafe` on *Saya Background Picker*,
  `unsafe_backgrounds` on *Saya Main Prompt*, `decors_unsafe` on *Saya Prompt MAIN (FR)*. It is the last input of each
  node, so saved workflows keep their settings.
- **Locked seeds keep their background.** The draw still runs on the whole stock; only a seed that lands on a
  fairground ride with the switch off is redrawn among the others, with the same seed. Test `background_picker_unsafe`.

## 2.1.1 — 2026-10-08

### Backgrounds

- **The background stock grows from 7 409 to 8 799** (1 390 new, same format: `bg` + hand-detailed `ultra`, 41 categories unchanged).
  - 32 hand-written places that were missing: pastel patisserie, cat cafe, royal marble baths, empty public bathhouse, art deco hotel pool, gamer bedroom, streamer studio, jazz lounge, snowy pine forest, dojo, fairy domain, infernal throne hall…
  - 193 style × time-of-day combinations for the most requested kinds of places: 8 living room styles, 8 bedroom styles, luxury lounges, gamer rooms, sport halls, pastel places, royal baths, infernal halls.
  - 1 164 places from a large list of ideas (urban corners, craft workshops, transport, landscapes, gardens, heritage, industry, science, spa and leisure, uncanny and dreamlike places, near future, weather moods, horror, medieval, modern city, nostalgia, dystopia, places of worship, cosmic horror, oppressive architecture), translated into prompt tags and filtered: no person or silhouette in the scenery, no object that creates an extra figure or an inset (mirror, statue, portrait, mannequin, mask, screen), nothing related to childhood, no gore, no place of captivity.
  - A creepy dismantled carousel in the fog (horror category).
- Background Picker tests pass on the larger stock.

### Prompt guides

- `PROMPTS_GUIDE_EN.md` / `PROMPTS_GUIDE_FR.md` gain **§8, lessons from a 2 500-character solo batch**: where a tag lands in the CLIP chunks matters more than its weight, framing first, light weights for age tags, colour words that bleed, a flat chest on an adult body still to be proven, how original characters keep their identity, creatures that stay about 80 % human, the sticky liquid nobody asked for, matched backgrounds, LoRA triggers, weak concepts, describing what must look good, locking a written size, colours of small details, per-character negatives, Sampler 2 drawing the details, species materials on the whole body.

## 2.1.0 — 2026-10-07

### Fixes

- **Solo Phase 1 follows the prompt again.** In Solo, `SayaMultiCouple` combined MAIN + ACTION and P1 with
  `ConditioningCombine`, which averages two separate predictions: half of the guidance came from the scene and the
  pose alone, with no character in it (wrong clothes, wrong person, censored or deformed results). Solo now sends
  **one** conditioning, MAIN ++ ACTION ++ P1 (token concatenation), exactly like the phases rebuilt from the imprint
  (one text "MAIN, ACTION, PERSON 1"). Couple mode is not touched. Test updated (`multi_couple_solo`).
- **Naturalize colour lock no longer paints colour fringes.** `SayaNaturalizePostProcess` took the refine's
  lightness and the reference's colour pixel by pixel. Even at denoise 0.1 the refine redraws lines by a pixel or
  two, so the old line colour landed next to the new line: coloured halos on eyes, lashes and mouth, then amplified
  by every later pass. The lock now corrects only the **blurred (area) colour** drift and keeps the refine's own
  line colour. On a real image with lines moved by 1-3 px, the largest change drops from 75-84 to 6-21 levels (8 bit)
  and no pixel moves by more than 20 levels any more. `color_lock` keeps its range and meaning for areas.
- **Highlight taming off in the Full workflow.** It also darkened the light side of every line and the eye
  catchlights (7 % of the pixels touched on a test image). The Full workflow ships it at 0; the tooltip says why.

### New

- **Saya LoRA Family Filter · Model 2** (`SayaLoraFamilyFilter`). Applies the LoRAs of the Model 1 stack to
  Model 2, keeping only those its family can take, from the `base_model` LoRA Manager stores for each file: SDXL
  model 2 -> SDXL LoRAs only; Illustrious model 2 -> Illustrious and SDXL; any other family -> everything (for now);
  a LoRA without a known family passes and is reported. The checkpoint family comes from its metadata, or from its
  folder when the metadata says "Unknown". UNet only (Model 2 keeps the Model 1 CLIP). Inputs: the five
  `loaded_loras` outputs of LoRA Manager loaders, the Model 2 checkpoint name (text input that follows the loader),
  free extra LoRAs; output `report` lists PASS / BLOCK / SKIP. Wired in the Full workflow; Model 2 used to run
  without any LoRA. Tests: `lora_family_filter`.
- **Pubic hair is an anatomy anchor of its own.** In `phrase_anatomy` ownership, "green pubic hair" or
  "green hairy penis" is now a separate part next to the organ's size / state, so a hair colour that differs between
  the two characters anchors each organ to its owner. Prompting tip: write `<hair colour> pubic hair` (the Danbooru
  tag, known to Illustrious) rather than "hairy penis".

### Workflows

- **Demo and Full rebuilt from my current workflow** with `tools/build_demo_workflow.py` and
  `tools/build_full_workflow.py` (same public-safe rules: demo prompts, `SELECT_*` placeholders, empty LoRA stacks,
  no personal names). The Full workflow follows my own placement of the Hires + USDU block: the *MAIN · USDU
  Position* selector of 2.0 is gone. Detailer slots are named **Detailer 1 … Detailer 13**. The Full builder also
  empties the per-character anatomy fields under their current titles. The demo only gains the `person_anchor`
  input; `Saya_Couple_Demo_api.json` is regenerated (unchanged).
- The workflow check `test_usdu_position_switch` skips a workflow that has no USDU position selector.

### AMD

- **AMD desktop VRAM reserve counted once.** The optional safety patch already excludes 1 GiB from the
  allocator cap. `extra_reserved_memory()` now reserves only the part of `--reserve-vram` above that amount
  while the cap is active. Uncapped devices keep their previous behavior; the hard cap, full unloads,
  inference reserve and restriction on partial-model growth remain in place.
- The installer upgrades the previous AMD patch using a checked three-line delta and preserves its original
  backup. CPU checks cover fresh install, upgrade, idempotence, conflict refusal, reversal and five reserve cases.
  HiDream completed two local GPU runs with full model loading (warm and cold conditioning cache); no general
  speed or image-quality improvement is claimed. Workflows and artistic settings are unchanged.

## 2.0.1 — 2026-10-05

### Fix

- **Solo keeps the ACTION in Phase 1.** With *Couple / Solo* OFF, `SayaMultiCouple` encoded MAIN + P1 only and
  dropped the `action` input; the phases rebuilt from the imprint (2 to 6) already used MAIN + ACTION + P1, so the
  pose could change between Phase 1 and Phase 2. Solo now reads exactly MAIN (+ ACTION, concatenated as for the
  characters in Couple) + P1 and the negative; P2 is never read. Couple mode unchanged (bit for bit: the Couple
  path is not touched). Test added (`multi_couple_solo`). Solo renders with a connected ACTION change on purpose.

## 2.0.0 — 2026-10-05

The big one. 2.0 is the result of a full audit of the pack (bug classes, invariants, fresh install, architecture),
done with the generations as the judge: every change below was tested, and the ones that touch the image were
compared **pixel for pixel** with the previous state before being kept. Images at the same seed are unchanged
unless stated.

### No ComfyUI core modification any more

- **The couple engine moved from `comfy/ldm/modules/attention.py` into the pack**
  (`src/engine/dual_attention.py`, moved verbatim: forced attn2 path, `main_locked_delta` fusion, gain 0.78,
  dynamic ownership). It is attached to a clone of MODEL_1 with ComfyUI's official
  `ModelPatcher.add_object_patch`: each cross-attention block of the clone is a shallow copy whose `attn2` runs the
  engine when the Saya payload is present and the stock cross-attention otherwise. ComfyUI installs the copies in
  `patch_model()` and restores the originals in `unpatch_model()`; an ON_DETACH callback restores them too when a
  clone is dropped without unpatching, so every clone puts back exactly what it found. State dict keys, LoRA
  patches and weights are those of the stock block. **Phase 1 images are bit-identical** to the 1.x core patch at
  the same seed (GPU parity, both models, after warm-up).
- **Nothing under `comfy/` is shipped or patched.** `files/comfy/` and the required patch are gone; the reference
  copy `patches/legacy/saya_dual_attention_1.x.patch` only serves the upgrade. The optional AMD VRAM patch is the
  only core change the installer can still make.
- The pack no longer checks for a core patch at startup; the fail-closed checks moved to the attachment itself
  (object patch replacing a block class, another object patch on a block, model wrappers, attn2 patches /
  replacements, attention overrides → clear error).
- The engine's identity is traceable: the test harnesses record `engine_sha256` with every image.

### Upgrading from 1.x

- `./saya install` on a 1.x install **removes the 1.x core patch** (exact reverse of the same patch file, refused
  if the file does not match), keeps the original pre-1.x backup for `restore`, **removes the obsolete pack files**
  (`core_patch/`, …) and installs 2.0. `verify` reports `core files : stock attention.py` or
  `1.x CORE PATCH STILL APPLIED`. New installer scenario L covers it (upgrade, obsolete file removed, verify OK,
  restore byte-identical to the pre-1.x backup).
- Every 1.x node type still exists; 1.x workflows load and run as they are.

### Dynamic ownership and one map for every pass

- **`SayaMultiCouple` ownership = `dynamic`** (the shipped workflows use it; `static_split` stays the default of the
  node for old workflows): during the first steps of Sampler 1 the engine reads, in every cross-attention layer,
  the affinity of each latent cell to P1's and P2's anchor tokens, clusters them into zones, confirms a zone's owner
  when it is stable across steps and locks it. Unclaimed cells belong to MAIN (**background = MAIN**): the
  background is painted by MAIN alone. No external detector of any kind; the drawn split masks only seed the
  first steps.
- **New node `Saya Ownership Map · Sampler 1 -> Imprint`**: runs right after Sampler 1, reads the final ownership
  once, writes it into the couple imprint (`ownership_map`) and rebuilds MODEL_2 on it. Sampler 2, Hires Fix, USDU
  tiles, HiDream, the detailers and Phase 6 all split P1 / P2 on that same map. The map is **bound to the sampling
  that produced it** (keyed by its P1 conditioning, the engine's own key): a map from another MODEL_1, another
  image or a batch is refused, and the static split is kept with a report that says why.
- **New optional input `person_anchor`** on `SayaMultiCouple` (default `woman` = the validated behaviour, bit for
  bit; `auto` adds `man`; or your own words): the generic "a person is here" anchor the dynamic mode starts from.
  The person word lists now cover women, men, boys, girls and the common hybrid words.
- **Silent fallbacks are not silent any more**: ownership = dynamic without a derivable anchor, or a grid above
  the engine's token budget (presets ≥ 1536 × 832), used to fall back to the static split with no trace. Both now
  log a warning that names the cause.
- `zone_fallback` (give undecided cells to a person by block majority) stays available and off.

### MAIN prompt builder and background stock

- **New node `Saya Main Prompt · Prefix + Background + Tags`** (`SayaMainPrompt`): your prefix copied as written
  (LoRA triggers, weights and spacing untouched), a background from the stock (41 categories, `seed` fixed = locked,
  randomize = a new one each run, `free` = your own text, 5-entry shared history, `ultra_detailed` long form), one
  tag per drop-down menu (lighting, time, weather, particles, palette, detail, rating) and `extra`. Outputs
  `main_prompt`, `background`, `short_name`, `info`.
- **`SayaMainPromptFR`**: the same node with French labels (inputs, categories, history, menus); tags are sent in
  English, output identical to the English node.
- **New node `Saya Background Picker · 40 categories`**: the background alone; stock of **7 409** hand-written
  backgrounds (`data/backgrounds.json` 6 409 + `data/backgrounds_fou.json` 1 000 improbable places, `horror++`
  category added). The stock ships with the pack (1.x's working copy fell back to a path on my machine).

### Couple context (phases 2 to 6)

- **New node `Saya Couple Context · Load`** (`SayaCoupleContextLoad`): Resolve / Load + Checkpoint Identities +
  Retarget in one node. It resolves and validates the Phase 1 imprint **once per phase** and hands
  `SayaCoupleReconstruct` a `SAYA_COUPLE_CONTEXT` per model (imprint, identities, source chain, retarget, phase,
  debug). **`Saya Couple Context · Inspect`** prints it. The full workflow uses it in phases 2, 4, 5 and 6:
  22 fewer executed nodes, imprint parsed 3 times per phase instead of 8, **images identical** (GPU, same seeds).
  `SayaCoupleReconstruct` gained an optional `context` input; `checkpoint_identities` became optional. The historic
  nodes still work.

### Deprecated node types (still loadable)

Thirteen types nothing in the current pipeline uses are kept under `saya/deprecated` with `[DEPRECATED]` in their
title; they run as before and log one warning per session naming the replacement:
`SayaImagePhaseController`, `SayaLazyCheckpointLoader`, `SayaImagePhaseCheckpointLoad`,
`SayaImagePhaseCheckpointStop`, `SayaImagePhase1Stop`, `SayaCoupleImprintDerive`, `SayaWarmupGate`,
`SayaDuoLatentShape`, `SayaDuoTiledUpscale`, `SayaResolutionScaleCalculator`, `SayaNear4KTargetCalculator`,
`SayaKSamplerConfig`, `SayaPPMMasks`.

### Phase review

- **Third button, STOP · KEEP**: the Phase 1 review popup now offers *continue*, *redo (new seed)* and *stop and
  keep this image* (for when the background or the pose is right and you want the image as it is, without the
  later phases).

### Fixes

- **Fresh install was impossible in 1.x**: the distributed core patch did not carry the dynamic engine, and the
  compatibility checker answered NOT COMPATIBLE on a clean ComfyUI (stale patch + a false negative on packages
  without `__init__.py`). Fixed first by regenerating the patch, then made moot by the engine move.
- **Without MultiMaskCouple the whole pack failed to load**, Solo included, because of an import at load time. The
  import is now lazy, with a clear error only when a Couple pass actually needs it.
- **Ownership map capture was not bound to the model that produced it** (it read "the only engine state there
  is"); it is now keyed by the recipe's own P1 conditioning.
- A silently substituted VAE (unreadable file → fallback) is now logged with the cause.
- Installation messages pointed to files that did not exist; one test failed on a maintainer-only file instead of
  skipping.
- **AMD VRAM patch**: the VRAM cap is clamped to a valid fraction, so a second Python process starting while the
  ComfyUI server holds the VRAM no longer dies with `Invalid fraction value`.
- Windows: the debug signal handler the pack registers does not exist there; guarded.
- Mask blur and PNG metadata reading had two implementations each; one is kept, bit-identical output.

### Installer and compatibility

- Compatibility axis A is now the **engine** (nine anchors read from the sources: the `self.attn2(...,
  transformer_options=...)` call, `add_object_patch`, `object_patches_backup`, `ON_DETACH`,
  `deepcopy_list_dict`, `cond_or_uncond` / `activations_shape`, …) instead of "does the patch apply".
- `install` prints `core modification : NONE`; `verify` reports the state of `attention.py`, the AMD patch and the
  four dependencies the workflows need (MultiMaskCouple, RES4LYF, Impact Pack, Ultimate SD Upscale).
- `install` removes the files a previous version shipped that the new one no longer has.
- Smoke test: the engine path is exercised on a real block through the object patch (`dual_path_live`).
- `compatibility.json`: `files_patched` = optional AMD patch only; `files_patched_legacy` = the 1.x patched
  `attention.py` (accepted as tested until the installer removes it).
- The pack's own `tools/check_comfyui_compat.py --comfyui <root>` performs the same check without the installer
  (reads the sources, imports nothing); optional dependencies (Impact Pack, Ultimate SD Upscale and its patch,
  RES4LYF patch, AMD patch) are reported as OPTIONAL, never as failures.

### Third-party patches (optional, by hand)

- `patches/third_party/res4lyf_hidream_attention_split.patch` (new): the local RES4LYF change that lets the
  HiDream pass run on 16 GB (attention split). Optional, documented, not installed by the installer.
- `patches/third_party/ultimatesdupscale_saya_couple_crop.patch`: synced with the version in use.

### Workflows

- **Demo** rebuilt: `Saya Main Prompt` node (prefix + background + tags, free mode with the demo scene), ACTION
  prompt, P1 / P2 text fed to the couple node for the anchors, dynamic ownership, `person_anchor`. Prompts in
  `workflows/demo_prompt.json` follow the 4-field grammar (PREFIX, SCENE, ACTION, P1, P2, NEGATIVE). API version
  regenerated from the server's node definitions.
- **Full** rebuilt from the validated live workflow: `Saya Main Prompt` (English labels), ACTION, the Phase 1
  ownership map node, `Saya Couple Context · Load` in phases 2 / 4 / 5 / 6, STOP · KEEP in the review. Same
  neutralisation as before (checkpoints, VAEs, LoRAs, detectors, notes).

### Documentation

- README rewritten for 2.0 (no core modification, upgrade path, dynamic ownership, prompt grammar).
- **Prompt guides** (new): `docs/PROMPTS_GUIDE_EN.md` and `docs/PROMPTS_GUIDE_FR.md`, the 4-field grammar
  (MAIN = background only, ACTION, P1, P2), the validated tag families, weight syntax, what breaks ownership.

### Tests

- 165 pack tests (was 144), 173 with the full workflow loaded: the engine attachment (restore on unpatch and on
  detach, class / object patch / wrapper refusals), the engine moved verbatim (hash), map capture bound to the
  recipe, lazy MultiMaskCouple import, fallbacks that warn, `person_anchor`, the MAIN prompt node (EN / FR
  equivalence, prefix verbatim, locked / random / free background, history), the context nodes (same output as the
  historic chain, CPU cost), deprecated types, blur / PNG single implementations, workflow link and routing checks.
- `tests/proof_gain.py` runs on the pack engine; `tests/installer_scenarios.py` 27 checks incl. the 1.x upgrade.
- GPU: Phase 1 parity 1.x core patch vs 2.0 attachment bit-identical (both models); context nodes bit-identical on
  3 seeds through phases 1 → 6; `person_anchor` checked on 4 images.

## 1.0.2 — 2026-09-29

### Full workflow

- **Phase 6 diffuses at the SDXL-native size.** Naturalize V2 used to run its single diffusion on the ~2K final image
  (1584 × 2320 ≈ 3.7 MP, one block): SDXL then placed prompt details (extra nipples, a navel) in empty parts of each
  person's half. Phase 6 now refines at the Phase 5 size, then upscales with the model to the same final size, then
  applies soften / grain / dither at that size (the soften and grain nodes moved out of the Naturalize V2 subgraph,
  unchanged). About 2× faster.

### Couple

- **MultiMaskCouple masks on the real grid.** The MultiMaskCouple patch guesses its attention grid from the latent
  size; SDXL rounds every downsample up, so at image sizes that are not a multiple of 32 the guess failed (41.6 % of
  grids between 512 and 2560 px) and the P1 / P2 mask was laid out transposed (1584 × 2320: grid 73 × 50 read as
  50 × 73). Saya now hands it the block's real activation grid. Pixel-identical at multiples of 32 (832 × 1216,
  1664 × 2432).

### Tests

- 144 tests (was 140): the grid at every size from 512 to 2560 px, P1 / P2 placement at 1584 × 2320 with the real
  MultiMaskCouple patch (50 % → 100 % of tokens in the right region), and bit-identity at multiples of 32.

## 1.0.1 — 2026-09-28

### Full workflow

- **Hires Fix 1 / 3 and Phase 6 back on the PPM couple.** Since 0.2.0 these full-frame passes used MultiMaskCouple;
  they now use the same crop-aware PPM couple as USDU and the detailers again (`SayaCoupleReconstruct` output 0
  instead of output 4). Only the origin of three links changes.

### Phase review

- **Resume after an interruption.** Interrupting (or an error in) Phase N used to make the next queue restart
  Phase 1 with a new master seed, so the image you had validated was lost. The next queue now asks to resume at
  Phase N from the validated Phase N-1 (Cancel = new image from Phase 1). A queue error in Phase N also no longer
  resets Phase 1. Phases 2-6 always use the seed saved with the validated Phase 1, not the Master Seed widget.

### Fixes

- The "USDU couple crop unavailable" warning was a false alarm on a correctly patched ComfyUI_UltimateSDUpscale
  (that pack removes its own modules from `sys.modules`); the per-tile crop itself always worked.

## 1.0.0 — 2026-09-28

Bug fixes from a full audit of 0.3.0. Couple and Solo images are unchanged: 23 / 23 Phase 1 images are
pixel-identical before / after (alternating Couple / Solo, 10 repeated renders, 7 edge cases), and 50 renders in one
ComfyUI session showed no memory or speed drift.

### Phase review

- **CONTINUE validates the image you are looking at.** The review popup now sends the transaction it shows; if a
  second Phase 1 render replaced the candidate in the meantime (double click, re-queue), validation is refused
  instead of silently validating the other image.
- **An error no longer deletes your validated Phase 1.** An error or an interruption in Phase 2-6 used to call the
  Phase 1 reset, which deleted the validated Phase 1 checkpoint. It now only drops the failing phase's candidate.

### Couple

- **No more silent MAIN-only Couple.** On a ComfyUI without the Saya core patch, Couple rendered exactly the MAIN
  image (P1 / P2 ignored) with no error. The pack now checks the patch: Couple raises a clear error, and a warning is
  logged at startup. Solo is unaffected.
- **Empty P2 in Couple stops in Phase 1.** It used to render Phase 1, then fail in Phase 2 after validation.
  `SayaCoupleImprintPackV2` has a new optional `solo` input; the Full workflow connects it to the Couple / Solo
  switch. Workflows without that link behave as before.
- **USDU without the optional crop patch is no longer silent.** Couple USDU passes on a stock
  ComfyUI_UltimateSDUpscale log a warning (every tile then gets the full-frame masks).

### Workflows

- Full: `solo` of the couple imprint connected to *MAIN · Couple / Solo*. Demo: unchanged (no imprint node).

### Tests

- 140 tests (was 135): the five bugs above, each checked to fail on 0.3.0 and pass on 1.0.0, plus
  `tests/test_phase_failure_cleanup.cjs` for the review controller.

## 0.3.0 — 2026-09-28

### Full workflow

- **HiDream Couple keeps P1 and P2 apart.** Phase 3 now uses `ClownRegionalConditioning_AB` (P1 / P2) instead of
  `ClownRegionalConditioning3` (P1 / P2 / an always-empty MAIN region). RES4LYF builds HiDream's text-to-text mask as a
  parity checkerboard: with two regions parity equals region, with three it does not, and P1's text tokens attended
  P2's (and back) in all 48 blocks. Checked against RES4LYF's own mask builder; also 23 % fewer attention tokens.
- **Solo mode works end to end.** Switching *MAIN · Couple / Solo* to OFF used to stop Phase 1 with
  `dual_attention_enabled cannot be combined with solo`. Solo is now the authority everywhere: no couple patch, no
  regional conditioning, no crop in any phase; HiDream refines with one global MAIN + P1 prompt.
- **USDU couple crop follows the mode.** The *Use Couple Crop* switch is gone: Couple → per-tile couple crop,
  Solo → none. The crop gate is now forced for each pass, so a `SAYA_USDU_COUPLE_CROP` exported globally in the shell
  can no longer override it (before, "crop OFF" never actually turned it off).
- Cleanup: unused Phase 6 input and GetNode, stale notes and migration markers removed from the published workflows.
- Customised a 0.2.0 workflow? `python3 tools/migrate_workflow_v030.py old.json new.json` applies all of the above
  (it asserts the expected shape and writes nothing otherwise).

### Nodes

- `SayaCoupleHiDreamReconstruct`: output 2 renamed `conditioning_solo` (same slot). Couple: P1 / P2 conditionings and
  masks, no unused global encode (3 encodes instead of 4). Solo: one global conditioning, region outputs `None`, no
  geometry read. P1 / P2 masks must partition the frame (clear error otherwise); P2 absent → region B is MAIN on P1's
  complement.
- `SayaCoupleUSDUPass`: `couple_crop` widget removed, new required `solo` input. Old workflows: the frontend migrates
  the saved widget values on load (`web/saya_usdu_couple_crop_migration.js`) so `restore_to_base` keeps its value;
  connect the new `solo` input to the Couple / Solo switch. Old API prompts carrying `couple_crop` still run (ignored).
- `SayaMultiCouple`: `dual_attention_enabled` removed. Couple always runs the Saya dual attention on MODEL_1 (every
  shipped workflow already had it on), MODEL_2 the MultiMaskCouple patch; Solo never patches. Old workflows keep
  working: the stale widget value is ignored.
- Removed dead code with no caller left (tile-window helpers, unused USDU config helpers, duplicate couple helpers).

### Tests

- 135 tests (was 126): regional attention checked on RES4LYF's real mask builder, USDU crop through ComfyUI's input
  handling (including a globally exported gate), workflow link / routing / anti-double-hook checks, guards against the
  removed paths, and the migration shim. `tests/run_all.py` now reports a crashing test as a failure instead of
  stopping the run.
- The cleanup is pixel-identical on every phase of full Couple and Solo runs (same seeds, before / after).

## 0.2.0 — 2026-09-28

### Full workflow

- **Full-frame passes on MultiMaskCouple.** Hires Fix 1 / Hires Fix 3 and Phase 6 now use the same MultiMaskCouple
  attention as Sampler 2, rebuilt at the current resolution from the couple imprint. USDU 1/2 and the detailers keep
  the crop/tile-aware PPM couple. `SayaCoupleReconstruct` builds both from two independent clones of the input model
  (new output `model_patched_multimask`, appended as slot 4 so existing links keep their indexes).
- **Phase 6 · Naturalize V2** (nested subgraph inside Phase 6). The old chain (RealESRGAN anime upscale → Final Hires
  diffusion → second "naturalize" diffusion with generic prompts replacing P1/P2 → second VAE round trip → automatic
  colour correction) is replaced by: Remacri upscale → the real MAIN / P1 / P2 / negative couple conditioning →
  **one** light diffusion (denoise 0.14, one VAE) → colour lock 0.8 → highlight taming 0.3 → soften 0.10 →
  grain 0.008 / dither 0.5.
  - Measured on 3 seeds against the old Phase 6: colour drift −55 to −63 %, invented face detail halved, blown
    highlights −35 to −53 %, micro-contrast back to the input level, Phase 6 time 165 s → ~115 s.
  - P1's white eyes are kept through the whole pipeline in 3/3 full 1→6 runs (the old Phase 6 turned them pink/red
    in 3 of 4 test seeds).
  - New download: `4x_foolhardy_Remacri.pth` (`models/upscale_models`). EasyColorCorrector is no longer needed.

### Nodes

- `SayaNaturalizePostProcess`: new optional `color_lock` (keeps the refine's OKLab lightness and detail, takes the
  colour of a `reference` image, 0–1) and `highlight_tame` (pulls local glow / specular peaks toward their
  surroundings; midtones and large bright areas such as white eyes are kept, 0–1). Both default to 0 = the previous
  behaviour, bit for bit.
- `SayaDuoSegsDetail`: each detailer passes its real crop position to the couple attention (`saya_couple_crop`, same
  contract as the USDU tiles), so P1 / P2 stay on the right side inside the crop. Only with an empty wildcard; a
  wildcard that can reorder or skip SEGS disables it with one warning, and any size mismatch with Impact Pack's actual
  processing disables it for the rest of the call instead of attaching a wrong region.
- `SayaCoupleReconstruct`: anti-double-hook guard also detects MultiMaskCouple patches (`patches_replace["attn2"]`).
- `SayaMultiCouple`: the couple core is shared with the reconstruction (`apply_multimask_couple`), no behaviour change.

### Tests

- 126 tests (was 101): detailer crop metadata, colour lock / highlight taming (including bit-exact backward
  compatibility), and the phase restart / restore suite now runs in `tests/run_all.py`.

## 0.1.0

First public release.

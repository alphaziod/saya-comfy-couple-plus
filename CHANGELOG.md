# Changelog

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

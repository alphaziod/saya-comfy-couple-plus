# Changelog

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

# Third-party notices and licensing

## Summary

| Part of this repository | Origin | License |
|---|---|---|
| Saya Couple original code (installer, tests, tools, most of `files/custom_nodes/Saya_Couple/`) | this project | **GPL-3.0-or-later** (see `LICENSE`) |
| `patches/saya_dual_attention.patch`, `patches/amd_vram_safety.patch`, `files/comfy/**` | modifications of **ComfyUI** | **GPL-3.0** (ComfyUI's license) |
| `files/custom_nodes/Saya_Couple/src/ppm_vendor/**` | vendored from **pamparamm/ComfyUI-ppm** | **AGPL-3.0-or-later** (`LICENSES/AGPL-3.0.txt`) |
| `files/custom_nodes/Saya_Couple/src/nodes/saya_attention_couple.py` | adapted from ComfyUI-ppm's "Attention Couple (PPM)" node | **AGPL-3.0-or-later** |
| `patches/third_party/ultimatesdupscale_saya_couple_crop.patch` | modification of **ssitu/ComfyUI_UltimateSDUpscale** | **GPL-3.0** |

The repository's **main license is GPL-3.0-or-later** (`LICENSE`). It also **contains components under other licenses**:
the files listed as AGPL-3.0-or-later **remain AGPL** (they do not become GPL by being here) and carry an
`SPDX-License-Identifier: AGPL-3.0-or-later` header; the ComfyUI-derived patches remain under ComfyUI's GPL-3.0. Combining GPLv3 and AGPLv3 code is explicitly allowed by section 13 of both licenses;
each part keeps its own license. In practice: if you run a *modified* version of the AGPL parts as a network
service for other users, the AGPL obliges you to offer those users the corresponding source.

The machine-readable per-file list (origin, license, sha256) is `MANIFEST.json`.

## ComfyUI

- Upstream: https://github.com/comfy-org/ComfyUI, license GPL-3.0 (text in `LICENSES/GPL-3.0.txt`).
- Saya ships **patches** to two ComfyUI files, plus reference copies of those two files as patched for the tested
  upstream commit `41db8f4f` (`files/comfy/**`). Changes are described in `README.md` ("Core files modified").

## pamparamm/ComfyUI-ppm (vendored)

- Upstream: https://github.com/pamparamm/ComfyUI-ppm, commit `6c6c360`, license **AGPL-3.0-or-later**.
- Original Attention Couple / NegPip implementation credited by ComfyUI-ppm to **laksjdjf**, **hako-mikan** and
  **Haoming02**.
- Vendored files: `attention_couple/common.py`, `attention_couple/unet_couple.py`, `attention_couple/cosmos_couple.py`,
  `negpip/anima_negpip.py` (+ `NEGPIP_KEY`/`has_negpip` in `ppm_vendor/__init__.py`).
- **Modified by Saya Couple:** `common.py` (added `crop_mask_to_tile`) and `unet_couple.py` (per-tile mask cropping,
  opt-in through `transformer_options["saya_couple_crop"]`). Each modified file carries a notice at the top.
  `cosmos_couple.py` and `anima_negpip.py` are unmodified.

## Dependencies that are NOT included here

You install these yourself; **their own licenses apply to them**, not this repository's.

| Project | Needed for | License (as found in the project) |
|---|---|---|
| [MultiMaskCouple](https://github.com/tumbowungus/MultiMaskCouple) by tumbowungus | **required** by the pack (MODEL_2 couple) | GPL-3.0 |
| [RES4LYF](https://github.com/ClownsharkBatwing/RES4LYF) by ClownsharkBatwing | both workflows (ClownsharKSampler, DetailBoost, Epsilon Scaling) | AGPL-3.0 text **preceded by an additional clause: using the software or a derivative to provide a commercial service (e.g. a paid AI image generation service) is prohibited without permission / a separate license from the copyright holder**. Read its LICENSE before any commercial use. |
| [ComfyUI_UltimateSDUpscale](https://github.com/ssitu/ComfyUI_UltimateSDUpscale) | full workflow (USDU passes) | GPL-3.0 |
| [ComfyUI-Impact-Pack](https://github.com/ltdrdata/ComfyUI-Impact-Pack) | full workflow (detailers, SEGS) | GPL-3.0 |
| [ComfyUI-Impact-Subpack](https://github.com/ltdrdata/ComfyUI-Impact-Subpack) | full workflow (UltralyticsDetectorProvider) | AGPL-3.0 |
| [ComfyUI-GGUF](https://github.com/city96/ComfyUI-GGUF) | full workflow (HiDream GGUF) | Apache-2.0 |
| [ComfyUI-KJNodes](https://github.com/kijai/ComfyUI-KJNodes) | full workflow (Set/Get, latent presets) | GPL-3.0 |
| [rgthree-comfy](https://github.com/rgthree/rgthree-comfy) | full workflow (seed, group toggles, comparer) | MIT |
| [ComfyUI-Lora-Manager](https://github.com/willmiao/ComfyUI-Lora-Manager) | full workflow (LoRA stacks, shipped empty) | GPL-3.0 |
| [ComfyUI_FearnworksNodes](https://github.com/fearnworks/ComfyUI_FearnworksNodes) | full workflow (HiDream regional) | Apache-2.0 |
| [ComfyUI-DaSiWa-Nodes](https://github.com/darksidewalker/ComfyUI-DaSiWa-Nodes) | full workflow (savers with metadata) | Apache-2.0 |
| [ComfyUI_JPS-Nodes](https://github.com/JPS-GER/ComfyUI_JPS-Nodes) | full workflow (sampler/scheduler settings) | see its repository |
| [ComfyUI-EasyColorCorrector](https://github.com/regiellis/ComfyUI-EasyColorCorrector) | full workflow (final colour correction) | MIT |

No checkpoint, VAE, LoRA or detector model is distributed. Both workflows use `SELECT_*` placeholders; well-known
public component names (HiDream text encoders / GGUF, SAM, RealESRGAN) are kept as they are.

## Points documented as uncertain (not legal advice)

- **ComfyUI "-only" vs "-or-later":** ComfyUI ships the GPLv3 text without choosing. The core patches are therefore
  labelled plainly "GPL-3.0 (ComfyUI)".
- **Attribution of the vendored algorithm:** the original authors' repositories are GPL-3.0; the AGPL of
  ComfyUI-ppm (stricter) is the one applied to the vendored files.
- **AI-assisted code:** most of this project was written with AI assistance. The license above is applied to the
  whole project regardless; the copyright status of AI-assisted code varies between jurisdictions.

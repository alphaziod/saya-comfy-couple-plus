# ppm_vendor

Copy (2026-09-18) of the modules strictly required by the
`SayaAttentionCouplePPM` node, taken from `comfyui-ppm/src/`:

- `attention_couple/common.py`, `unet_couple.py`, `cosmos_couple.py`
- `negpip/anima_negpip.py` (plus `NEGPIP_KEY`/`has_negpip` in `__init__.py`,
  originally in `nodes_ppm/clip_negpip.py`)

Licenses: GPL-3.0 (original authors laksjdjf / hako-mikan / Haoming02) and
AGPL-3.0 (the pamparamm/ComfyUI-ppm pack). No algorithmic changes.


## Modifications

`cosmos_couple.py` and `negpip/anima_negpip.py` are unmodified.
`attention_couple/common.py` and `attention_couple/unet_couple.py` were MODIFIED by Saya Couple to crop the
couple masks per upscale tile (opt-in metadata `saya_couple_crop`); each file carries a notice.
Upstream: https://github.com/pamparamm/ComfyUI-ppm (AGPL-3.0-or-later), vendored from commit 6c6c360.

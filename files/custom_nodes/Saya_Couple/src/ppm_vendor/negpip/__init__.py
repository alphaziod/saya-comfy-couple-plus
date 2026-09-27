# SPDX-License-Identifier: AGPL-3.0-or-later
# Vendored from pamparamm/ComfyUI-ppm (2026-09-18). See ../__init__.py.
# NEGPIP_KEY / has_negpip proviennent de nodes_ppm/clip_negpip.py (AGPL-3.0, laksjdjf & hako-mikan).

NEGPIP_KEY = "ppm_negpip"


def has_negpip(model_options: dict) -> bool:
    return model_options.get(NEGPIP_KEY, False)

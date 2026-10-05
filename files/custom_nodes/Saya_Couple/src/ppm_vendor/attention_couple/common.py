import math
from typing import Any

import torch
import torch.nn.functional as F

COND = 0
UNCOND = 1

CondLike = list[tuple[torch.Tensor, dict[str, Any]]]


def lcm_for_list(numbers: list):
    current_lcm = numbers[0]
    for number in numbers[1:]:
        current_lcm = math.lcm(current_lcm, number)
    return current_lcm


def reshape_mask(mask: torch.Tensor, size: tuple[int, int], bs: int, num_tokens: int) -> torch.Tensor:
    num_conds = mask.shape[0]

    mask_downsample = F.interpolate(mask, size=size, mode="nearest")
    mask_downsample_reshaped = mask_downsample.view(num_conds, num_tokens, 1).repeat_interleave(bs, dim=0)

    return mask_downsample_reshaped


def crop_mask_to_tile(mask: torch.Tensor, crop: dict) -> torch.Tensor:
    """Slice ``mask`` (num_conds, B, H, W) to the CROP window of one upscale tile.

    ``crop`` is the ``saya_couple_crop`` metadata (crop_model_patch.py): ``crop_region``
    (x1,y1,x2,y2) IN CANVAS PIXELS, plus ``full_width``/``full_height`` (canvas size).
    The couple mask lives at the RECONSTRUCT resolution (the phase image), not at the
    USDU upscale canvas (upscale_by != 1), so the window is mapped by FRACTION
    (crop/full), never copied as raw pixels — resolution-independent, like the rest
    of the imprint scheme (regions are normalized fractions).

    Call order matters: this function does the REAL slice; the caller must then pass
    the result to ``reshape_mask`` (NEAREST) — never the other way around (squashing
    the full frame down and only then claiming it was cropped loses the per-tile
    region entirely).
    """
    full_w = float(crop.get("full_width") or 0)
    full_h = float(crop.get("full_height") or 0)
    region = crop.get("crop_region")
    if full_w <= 0 or full_h <= 0 or not (isinstance(region, (list, tuple)) and len(region) == 4):
        return mask  # incomplete metadata -> no crop, degrade to the full mask

    mask_h, mask_w = mask.shape[-2], mask.shape[-1]
    x1, y1, x2, y2 = (float(v) for v in region)

    x0 = min(max(round(x1 / full_w * mask_w), 0), mask_w - 1)
    y0 = min(max(round(y1 / full_h * mask_h), 0), mask_h - 1)
    x1p = min(max(round(x2 / full_w * mask_w), x0 + 1), mask_w)
    y1p = min(max(round(y2 / full_h * mask_h), y0 + 1), mask_h)

    return mask[:, :, y0:y1p, x0:x1p]

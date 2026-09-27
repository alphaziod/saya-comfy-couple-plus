"""SayaHiDreamSafeScale -- a DETERMINISTIC ceiling for the HiDream pass (never decides the final resolution).

Rule (applied BEFORE HiDream; no native try, no post-OOM fallback, no VRAM read):

* ``tokens = ceil(W/16) * ceil(H/16)`` (cost of the masked attention, sensitive to aspect ratio);
* ``tokens <= hard_safe_work_token_budget``: passthrough; otherwise a MANDATORY reduction to the LARGEST
  size (multiples of 16) whose tokens stay <= the ceiling, in the closest HiDream ratio family
  (``HIDREAM_BUCKETS``) if the deviation is <= ``snap_tolerance``, otherwise at the original ratio;
* never an upscale; the chosen size never exceeds the ceiling after rounding.

Default hard ceiling: 4032 tokens = 896x1152 (56x72), an explicit USER DECISION (~99% VRAM usage, known and
accepted). Runtime data (16 GB GPU, GGUF Q5, regional path, RES4LYF patch): 896x1152 completed without
swapping (GTT stable); 1024x1024 (4096 tokens) exceeds the ceiling and is therefore reduced. A successful
run is evidence for CHOOSING the ceiling, never an authorization by resolution: any input above the
ceiling is reduced before HiDream.

The final resolution (original x output_scale) belongs to the downstream Hires Fix, which recovers the
original size from the ``source_file`` of the Phase 3 manifest (the Phase 2 checkpoint).
"""

from __future__ import annotations

import json
import math
from typing import Any

import comfy.utils

PATCH = 16
#: Official HiDream-I1 RESOLUTION_OPTIONS (width, height).
HIDREAM_BUCKETS = (
    (1024, 1024), (768, 1360), (1360, 768), (880, 1168), (1168, 880), (832, 1248), (1248, 832),
)
DEFAULT_HARD_SAFE_WORK_TOKEN_BUDGET = 4032  # user decision: 896x1152 (56x72 tokens), already proven with the RES4LYF patch
DEFAULT_SNAP_TOLERANCE = 0.01  # only snap to a HiDream family when the ratio is practically identical (no distortion)
ASPECT_FIT = 0.02
TARGET_UTILIZATION = 0.90  # quality target (never a permission to exceed the ceiling)
STRONG_REDUCTION = 0.5     # linear factor below which we warn


def tokens_of(width: int, height: int) -> int:
    return math.ceil(width / PATCH) * math.ceil(height / PATCH)


def nearest_bucket(width: int, height: int) -> tuple[tuple[int, int], float]:
    """HiDream bucket at the closest ratio, plus the relative ratio deviation."""
    ratio = width / height
    bucket = min(HIDREAM_BUCKETS, key=lambda b: abs(math.log(ratio / (b[0] / b[1]))))
    return bucket, abs(ratio / (bucket[0] / bucket[1]) - 1.0)


def largest_size(aspect: float, budget: int, max_width: int, max_height: int) -> tuple[int, int]:
    """Largest size (multiples of 16) under the token ceiling, ratio ~ aspect, bounded by the original."""
    best = None
    for w in range(PATCH, max_width + 1, PATCH):
        ideal = w / aspect
        for h in {int(ideal) // PATCH * PATCH, -(-int(ideal) // PATCH) * PATCH}:
            if h < PATCH or h > max_height or tokens_of(w, h) > budget:
                continue
            error = abs(w / h / aspect - 1.0)
            if error > ASPECT_FIT:
                continue
            key = (tokens_of(w, h), -error)
            if best is None or key > best[0]:
                best = (key, (w, h))
    if best is not None:
        return best[1]
    # ratio unreachable at a 16 px granularity (extreme aspect ratios): scale uniformly, then shrink to the ceiling
    scale = math.sqrt(budget * PATCH * PATCH / (max_width * max_height))
    w, h = max(PATCH, int(max_width * scale) // PATCH * PATCH), max(PATCH, int(max_height * scale) // PATCH * PATCH)
    while tokens_of(w, h) > budget and (w > PATCH or h > PATCH):
        if w >= h and w > PATCH:
            w -= PATCH
        elif h > PATCH:
            h -= PATCH
        else:
            w -= PATCH
    return w, h


def select_size(width: int, height: int, budget: int, snap_tolerance: float) -> dict[str, Any]:
    """Complete decision (no tensors involved): working size + report."""
    tokens = tokens_of(width, height)
    bucket, deviation = nearest_bucket(width, height)
    family = f"{bucket[0]}x{bucket[1]}"
    report: dict[str, Any] = {
        "original_width": width, "original_height": height, "original_pixels": width * height, "original_tokens": tokens,
        "hard_safe_work_token_budget": budget, "hard_safe_work_pixel_budget": budget * PATCH * PATCH,
        "nearest_hidream_family": family, "family_ratio_deviation": round(deviation, 4),
    }
    if tokens <= budget:
        selected_w, selected_h, was_scaled, aspect_source = width, height, False, "none"
        reason = f"passthrough: {tokens} tokens <= hard safe ceiling {budget} tokens"
    else:
        snapped = deviation <= snap_tolerance
        selected_w, selected_h = largest_size(bucket[0] / bucket[1] if snapped else width / height, budget, width, height)
        was_scaled = True
        aspect_source = f"hidream family {family}" if snapped else "original ratio"
        reason = (f"hard safe ceiling: {tokens} tokens > {budget} tokens; largest 16-multiple size under the ceiling "
                  f"({'ratio of HiDream family ' + family if snapped else 'original ratio, no family within tolerance'})")
    selected_tokens = tokens_of(selected_w, selected_h)
    linear = math.sqrt(selected_w * selected_h / (width * height))
    utilization = selected_tokens / budget
    warnings = []
    if was_scaled and linear < STRONG_REDUCTION:
        warnings.append(f"STRONG REDUCTION: Phase 3 works at {linear:.0%} of the original linear resolution "
                        f"({selected_w}x{selected_h} for {width}x{height}); the safety ceiling has priority")
    if was_scaled and utilization < TARGET_UTILIZATION:
        warnings.append(f"ceiling utilization {utilization:.0%} < {TARGET_UTILIZATION:.0%}: ratio / multiples of 16 do not allow a closer fit")
    return {**report, "selected_width": selected_w, "selected_height": selected_h,
            "selected_pixels": selected_w * selected_h, "selected_tokens": selected_tokens,
            "hard_ceiling_tokens": budget, "ceiling_utilization_pct": round(100 * utilization, 1),
            "remaining_margin_tokens": budget - selected_tokens, "scale_factor": selected_w / width,
            "linear_scale_factor": round(linear, 4), "was_scaled": was_scaled, "aspect_source": aspect_source,
            "reason": reason, "quality_warning": "; ".join(warnings)}


class SayaHiDreamSafeScale:
    """Resolution ceiling for the Phase 3 HiDream pass: reduces BEFORE HiDream past the ceiling (never upscales)."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "image": ("IMAGE",),
                "hard_safe_work_token_budget": ("INT", {
                    "default": DEFAULT_HARD_SAFE_WORK_TOKEN_BUDGET, "min": 256, "max": 20000, "step": 16,
                    "tooltip": "Hard ceiling for the regional HiDream pass, in image tokens (16x16 px). "
                               "Above it: mandatory reduction before HiDream. This is not the final resolution."}),
                "snap_tolerance": ("FLOAT", {
                    "default": DEFAULT_SNAP_TOLERANCE, "min": 0.0, "max": 0.2, "step": 0.005,
                    "tooltip": "Max ratio deviation from an official HiDream family to adopt its ratio."}),
            },
        }

    RETURN_TYPES = ("IMAGE", "INT", "INT", "INT", "INT", "BOOLEAN", "FLOAT", "STRING")
    RETURN_NAMES = ("image", "original_width", "original_height", "working_width", "working_height",
                    "was_scaled", "scale_factor", "report")
    FUNCTION = "scale"
    CATEGORY = "saya/hidream"
    DESCRIPTION = (
        "Deterministic ceiling for the Phase 3 HiDream pass: passthrough under the token ceiling, otherwise a "
        "mandatory reduction to the largest size (multiples of 16) under the ceiling, in the closest HiDream "
        "ratio family. Never upscales, never reads VRAM. Does not decide the final resolution: the downstream "
        "Hires Fix recovers original x output_scale."
    )

    def scale(self, image: Any, hard_safe_work_token_budget: int, snap_tolerance: float) -> tuple[Any, ...]:
        _batch, height, width, _channels = (int(size) for size in image.shape)
        info = select_size(width, height, hard_safe_work_token_budget, snap_tolerance)
        report = json.dumps(info, ensure_ascii=False, sort_keys=True)
        if not info["was_scaled"]:
            return image, width, height, width, height, False, 1.0, report
        resized = comfy.utils.common_upscale(image.movedim(-1, 1), info["selected_width"], info["selected_height"], "area", "disabled")
        return (resized.movedim(1, -1), width, height, info["selected_width"], info["selected_height"],
                True, info["scale_factor"], report)

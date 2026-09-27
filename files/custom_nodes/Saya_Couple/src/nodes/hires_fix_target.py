"""SayaHiresFixTarget -- the Phase 4 Hires Fix target: original size x output_scale, in ONE increase.

Phase 3 works at a working resolution (SayaHiDreamSafeScale); the Hires Fix
does not restore the original size in two steps, it aims directly for
``original x output_scale``.

* original size = the size of the Phase 2 checkpoint referenced by the
  Phase 3 manifest's ``source_file`` (read from the PNG header, without
  loading the image);
* target = the ORIGINAL's ratio, multiples of 16, close to
  ``original_pixels x output_scale^2``.

This node only COMPUTES; ``SayaHiresFixResize`` applies the target (one
coherent upscale).
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from PIL import Image

from .saya_resolution_scale import SayaUpscaleTargetCalculator

MANIFEST_KEY = "saya_phase_manifest"


class SayaHiresFixTargetError(ValueError):
    """Unusable path, manifest or source checkpoint (never a guessed size)."""


def read_original_size(checkpoint_path: str) -> tuple[int, int]:
    """(width, height) of the source checkpoint referenced by ``checkpoint_path``'s embedded manifest."""
    path = Path(checkpoint_path)
    if not checkpoint_path or not path.is_file():
        raise SayaHiresFixTargetError(f"checkpoint not found: {checkpoint_path!r}")
    with Image.open(path) as image:
        raw_manifest = image.info.get(MANIFEST_KEY)
    if raw_manifest is None:
        raise SayaHiresFixTargetError(f"{checkpoint_path!r}: PNG chunk {MANIFEST_KEY!r} missing")
    try:
        source = json.loads(raw_manifest).get("source_file")
    except (ValueError, AttributeError) as error:
        raise SayaHiresFixTargetError(f"{checkpoint_path!r}: unreadable manifest ({error})") from error
    if not source or not Path(source).is_file():
        raise SayaHiresFixTargetError(f"{checkpoint_path!r}: source_file missing or not found ({source!r})")
    with Image.open(source) as origin:
        return origin.size


def target_size(original_width: int, original_height: int, output_scale: float) -> tuple[int, int]:
    """Aspect-preserving target (multiples of 16) close to ``original x output_scale``."""
    pixels = round(original_width * original_height * output_scale ** 2)
    return SayaUpscaleTargetCalculator._best_target(original_width, original_height, pixels)


class SayaHiresFixTarget:
    """Computes the Hires Fix target: original size (via the manifest) x output_scale."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "checkpoint_path": ("STRING", {
                    "forceInput": True,
                    "tooltip": "Path of the Phase 3 checkpoint: its manifest gives the Phase 2 checkpoint (original size)."}),
                "output_scale": ("FLOAT", {
                    "default": 1.0, "min": 0.25, "max": 4.0, "step": 0.05,
                    "tooltip": "Final resolution = original resolution x output_scale (1.0 = back to original)."}),
            },
        }

    RETURN_TYPES = ("INT", "INT", "INT", "INT", "STRING")
    RETURN_NAMES = ("original_width", "original_height", "target_width", "target_height", "report")
    FUNCTION = "compute"
    CATEGORY = "saya/hidream"
    DESCRIPTION = (
        "Phase 4 Hires Fix target: original (read from the Phase 3 manifest) x output_scale, the "
        "original's ratio, multiples of 16. Computation only: SayaHiresFixResize applies the target in one upscale."
    )

    def compute(self, checkpoint_path: str, output_scale: float) -> tuple[int, int, int, int, str]:
        original_width, original_height = read_original_size(checkpoint_path)
        target_width, target_height = target_size(original_width, original_height, output_scale)
        report = json.dumps({
            "original": [original_width, original_height], "output_scale": output_scale,
            "target": [target_width, target_height],
        }, sort_keys=True)
        return original_width, original_height, target_width, target_height, report

"""Spatial region-mask helpers used by couple nodes."""

from __future__ import annotations

from typing import Any

try:
    import torch
except Exception:
    torch = None


def create_region_mask(
    width: int, height: int, orientation: str = "horizontal", center: float = 0.5, side: int = 0
) -> Any:
    """Create a left/right or top/bottom ownership mask."""
    if torch is None:
        return None
    w = max(int(width or 1024), 8)
    h = max(int(height or 1024), 8)
    c = float(center if center is not None else 0.5)
    c = min(max(c, 0.0), 1.0)
    mask = torch.zeros((1, h, w), dtype=torch.float32)
    if str(orientation) == "vertical":
        cut = int(h * c)
        if side == 0:
            mask[:, :cut, :] = 1.0
        else:
            mask[:, cut:, :] = 1.0
    else:
        cut = int(w * c)
        if side == 0:
            mask[:, :, :cut] = 1.0
        else:
            mask[:, :, cut:] = 1.0
    return mask

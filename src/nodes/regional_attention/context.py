"""Context normalization, mode selection, and listening-strength helpers."""

from __future__ import annotations

from typing import Any

import torch


def pad_context_token_lengths(tensors: Any) -> Any:
    """Pad context tensors so every regional prompt has the same token length."""
    tensors = list(tensors)
    if len(tensors) < 2:
        return tensors
    max_tokens = max(
        (t.shape[1] for t in tensors if hasattr(t, "ndim") and t.ndim >= 3), default=None
    )
    if max_tokens is None:
        return tensors
    padded = []
    for t in tensors:
        if hasattr(t, "ndim") and t.ndim >= 3 and (t.shape[1] < max_tokens):
            pad_len = max_tokens - t.shape[1]
            pad = t[:, -1:, ...].expand(t.shape[0], pad_len, *t.shape[2:]).clone()
            t = torch.cat((t, pad), dim=1)
        padded.append(t)
    return padded


def concatenate_context_tensors(tensors: Any, dim: int = 0) -> Any:
    """Concatenate context tensors after equalizing their token lengths."""
    return torch.cat(pad_context_token_lengths(tensors), dim=dim)


def attention_debug_enabled(local_debug: bool = False) -> bool:
    """Return whether regional-attention debug logging is enabled."""
    try:
        import os

        return bool(local_debug) or os.environ.get("SAYA_COUPLE_DEBUG", "0") == "1"
    except Exception:
        return bool(local_debug)


def resolve_attention_mode(mode: str, original_shape: Any = None, qkv: Any = None) -> Any:
    """Normalize the requested attention mode and resolve AUTO from the active pass shape."""
    mode = str(mode or "GEN").upper()
    if mode not in ("GEN", "REFINER", "DETAILER", "AUTO"):
        mode = "GEN"
    if mode != "AUTO":
        return mode
    try:
        if original_shape is not None:
            h = int(original_shape[-2])
            w = int(original_shape[-1])
            if max(h, w) <= 768:
                return "DETAILER"
    except Exception:
        pass
    return "GEN"


def default_region_strength(mode: str) -> Any:
    """Return the default regional listening strength for an attention mode."""
    mode = resolve_attention_mode(mode)
    return {"GEN": 1.0, "REFINER": 0.8, "DETAILER": 0.94, "AUTO": 1.0}.get(mode, 1.0)


def resolve_region_strength(mode: str, custom_strength: float = None) -> Any:
    """Clamp a custom listening strength or use the mode default."""
    try:
        if custom_strength is not None and float(custom_strength) >= 0:
            return max(0.0, min(1.5, float(custom_strength)))
    except Exception:
        pass
    return default_region_strength(mode)


def apply_region_strength(masks_v: Any, strength: float) -> Any:
    """Adjust normalized ownership masks without leaking prompt influence across confident regions."""
    try:
        if not isinstance(masks_v, torch.Tensor) or masks_v.ndim != 4:
            return masks_v
        length = max(1, int(masks_v.shape[0]))
        strength = max(0.0, min(1.5, float(strength)))
        mask_sum = masks_v.sum(dim=0, keepdim=True).clamp_min(1e-06)
        masks_v = masks_v / mask_sum
        if length <= 1 or abs(strength - 1.0) < 1e-06:
            return masks_v
        neutral = torch.ones_like(masks_v) / length
        if strength < 1.0:
            confidence = masks_v.max(dim=0, keepdim=True).values
            neutral_peak = 1.0 / length
            denom = max(1e-06, 1.0 - neutral_peak)
            ambiguity = ((1.0 - confidence) / denom).clamp(0.0, 1.0)
            edge_mix = (1.0 - strength) * ambiguity
            out = masks_v * (1.0 - edge_mix) + neutral * edge_mix
        else:
            power = 1.0 + (strength - 1.0) * 2.0
            out = masks_v.clamp_min(1e-06).pow(power)
        out = out / out.sum(dim=0, keepdim=True).clamp_min(1e-06)
        return out
    except Exception as e:
        print(f"[ComfyCouple SayaPatch v3.3] strength apply failed: {e}")
        return masks_v

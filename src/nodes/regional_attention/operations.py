"""Attention execution helpers and regional/standard blend selection."""

from __future__ import annotations

from typing import Any

from comfy.ldm.modules.attention import optimized_attention

from .context import attention_debug_enabled, resolve_attention_mode


def try_standard_attention(
    q_i: Any, orig_k: Any, orig_v: Any, chunks: Any, idx: Any, n_heads: int
) -> Any:
    """Compute the unmodified attention output used as a local fallback blend."""
    try:
        if orig_k is None or orig_v is None:
            return None
        chunks = max(1, int(chunks))
        idx = int(idx)
        k_parts = orig_k.chunk(chunks, dim=0)
        v_parts = orig_v.chunk(chunks, dim=0)
        if idx >= len(k_parts) or idx >= len(v_parts):
            return None
        kk = k_parts[idx]
        vv = v_parts[idx]
        target_b = q_i.shape[0]
        if kk.shape[0] != target_b:
            if kk.shape[0] == 1:
                kk = kk.expand(target_b, -1, -1)
                vv = vv.expand(target_b, -1, -1)
            else:
                reps = (target_b + kk.shape[0] - 1) // kk.shape[0]
                kk = kk.repeat(reps, 1, 1)[:target_b]
                vv = vv.repeat(reps, 1, 1)[:target_b]
        if kk.dtype != q_i.dtype:
            kk = kk.to(q_i.dtype)
        if vv.dtype != q_i.dtype:
            vv = vv.to(q_i.dtype)
        return optimized_attention(q_i, kk, vv, n_heads)
    except Exception as e:
        if attention_debug_enabled():
            print(f"[ComfyCouple SayaPatch v3.3] normal attention fallback unavailable: {e}")
        return None


def calculate_regional_blend(
    mode: str, masks_aligned: bool = True, custom_strength: float = None
) -> Any:
    """Choose how strongly regional attention should replace standard attention."""
    mode = resolve_attention_mode(mode)
    if not masks_aligned:
        return 0.0
    if mode == "REFINER":
        base = 0.92
    elif mode == "DETAILER":
        base = 0.99
    else:
        base = 1.0
    return max(0.0, min(1.0, float(base)))

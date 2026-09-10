"""Mask extraction, crop propagation, token-grid resizing, and ownership repair."""

from __future__ import annotations

from typing import Any

import torch
import torch.nn.functional as F

from .context import attention_debug_enabled


def first_valid_mask_shape(source_masks: Any) -> Any:
    """Return the spatial shape of the first valid source mask."""
    try:
        for m in source_masks:
            if isinstance(m, torch.Tensor) and m.ndim >= 2:
                return (int(m.shape[-2]), int(m.shape[-1]))
    except Exception:
        pass
    return None


def extract_detailer_crop_context(options: Any) -> Any:
    """Extract normalized detailer crop metadata from ComfyUI execution options."""
    try:
        if not isinstance(options, dict):
            return None
        ctx = options.get("saya_couple_crop")
        if not isinstance(ctx, dict):
            for key in ("transformer_options", "model_options", "extra_options"):
                nested = options.get(key)
                if isinstance(nested, dict):
                    candidate = nested.get("saya_couple_crop")
                    if isinstance(candidate, dict):
                        ctx = candidate
                        break
        if not isinstance(ctx, dict):
            return None
        region = ctx.get("crop_region", ctx.get("bbox", ctx.get("crop")))
        if isinstance(region, dict):
            region = [region.get("x1"), region.get("y1"), region.get("x2"), region.get("y2")]
        if not isinstance(region, (list, tuple)) or len(region) != 4:
            return None
        x1, y1, x2, y2 = [float(v) for v in region]
        if x2 <= x1 or y2 <= y1:
            return None
        full_w = ctx.get("full_width")
        full_h = ctx.get("full_height")
        full_size = ctx.get("full_size", ctx.get("image_size"))
        if (
            (full_w is None or full_h is None)
            and isinstance(full_size, (list, tuple))
            and (len(full_size) >= 2)
        ):
            full_w, full_h = (full_size[0], full_size[1])
        return {
            "crop_region": (x1, y1, x2, y2),
            "full_width": int(full_w) if full_w is not None else None,
            "full_height": int(full_h) if full_h is not None else None,
            "label": str(ctx.get("label", "detailer")),
        }
    except Exception as e:
        if attention_debug_enabled():
            print(f"[ComfyCouple SayaPatch v3.4] invalid crop context: {e}")
        return None


def crop_masks_to_detailer_region(source_masks: Any, crop_ctx: Any, label: str = "") -> Any:
    """Crop full-image ownership masks to the active detailer region."""
    if crop_ctx is None:
        return source_masks
    try:
        x1, y1, x2, y2 = crop_ctx["crop_region"]
        result = []
        debug_rows = []
        for mask in source_masks:
            if not isinstance(mask, torch.Tensor) or mask.ndim < 2:
                result.append(mask)
                continue
            mh = int(mask.shape[-2])
            mw = int(mask.shape[-1])
            full_w = max(1, int(crop_ctx.get("full_width") or mw))
            full_h = max(1, int(crop_ctx.get("full_height") or mh))
            sx = mw / full_w
            sy = mh / full_h
            ix1 = max(0, min(mw - 1, int(x1 * sx)))
            iy1 = max(0, min(mh - 1, int(y1 * sy)))
            ix2 = max(ix1 + 1, min(mw, int(x2 * sx + 0.999999)))
            iy2 = max(iy1 + 1, min(mh, int(y2 * sy + 0.999999)))
            cropped = mask[..., iy1:iy2, ix1:ix2]
            if cropped.shape[-2] <= 0 or cropped.shape[-1] <= 0:
                result.append(mask)
                debug_rows.append(f"{mh}x{mw}->fallback")
            else:
                result.append(cropped)
                debug_rows.append(f"{mh}x{mw}->{cropped.shape[-2]}x{cropped.shape[-1]}")
        if attention_debug_enabled():
            print(
                f"[ComfyCouple SayaPatch v3.4] exact detailer crop label={label or crop_ctx.get('label', 'detailer')} region={crop_ctx['crop_region']} full={crop_ctx.get('full_width')}x{crop_ctx.get('full_height')} masks={debug_rows}"
            )
        return result
    except Exception as e:
        print(f"[ComfyCouple SayaPatch v3.4] crop mask failed: {e}; using full masks")
        return source_masks


def clone_model_with_detailer_crop(
    model: Any, crop_region: Any, full_width: int, full_height: int, label: str = "detailer"
) -> Any:
    """Clone a model and attach exact detailer crop metadata to its options."""
    cloned = model.clone()
    transformer_options = cloned.model_options.setdefault("transformer_options", {})
    transformer_options["saya_couple_crop"] = {
        "crop_region": [float(v) for v in crop_region],
        "full_width": int(full_width),
        "full_height": int(full_height),
        "label": str(label),
    }
    return cloned


def masks_match_attention_pass(source_masks: Any, original_shape: Any = None) -> bool:
    """Check whether source masks describe the current attention pass dimensions."""
    try:
        shape = first_valid_mask_shape(source_masks)
        if shape is None or original_shape is None:
            return (True, "unknown")
        mh, mw = shape
        oh = int(original_shape[-2])
        ow = int(original_shape[-1])
        if mh <= 0 or mw <= 0 or oh <= 0 or (ow <= 0):
            return (True, "bad_shape_unknown")
        mask_aspect = mw / max(1, mh)
        pass_aspect = ow / max(1, oh)
        aspect_ratio = max(mask_aspect, pass_aspect) / max(1e-06, min(mask_aspect, pass_aspect))
        approx_ph = oh * 8
        approx_pw = ow * 8
        much_smaller_crop = approx_ph < mh * 0.7 and approx_pw < mw * 0.7
        aspect_mismatch = aspect_ratio > 1.35
        ok = not (much_smaller_crop or aspect_mismatch)
        reason = f"aspect_ratio={aspect_ratio:.2f} crop_smaller={much_smaller_crop} mask={mh}x{mw} pass={oh}x{ow}"
        return (ok, reason)
    except Exception as e:
        return (True, f"detect_failed:{e}")


def factor_token_grid(token_count: int, original_shape: Any = None) -> Any:
    """Infer a plausible two-dimensional grid for a flattened token count."""
    token_count = int(token_count)
    if token_count <= 0:
        return (1, 1)
    aspect = None
    if original_shape is not None:
        try:
            h0 = int(original_shape[-2])
            w0 = int(original_shape[-1])
        except Exception:
            h0 = 0
            w0 = 0
        if h0 > 0 and w0 > 0:
            for rate in (1, 2, 4, 8, 16, 32, 64):
                h = max(1, h0 // rate)
                w = max(1, w0 // rate)
                if h * w == token_count:
                    return (h, w)
            aspect = w0 / max(1, h0)
    side = int(round(token_count**0.5))
    if side * side == token_count:
        return (side, side)
    best = None
    best_score = None
    for h in range(1, int(token_count**0.5) + 1):
        if token_count % h == 0:
            w = token_count // h
            if aspect is None:
                score = abs(w - h)
            else:
                score = abs(w / max(1, h) - aspect)
            if best_score is None or score < best_score:
                best_score = score
                best = (h, w)
    return best if best is not None else (1, token_count)


def resize_masks_to_token_grid(masks: Any, target_tokens: int, original_shape: Any = None) -> Any:
    """Resize source masks to the token grid used by the active attention layer."""
    if masks.shape[1] == target_tokens:
        return masks
    old_tokens = int(masks.shape[1])
    new_tokens = int(target_tokens)
    old_h, old_w = factor_token_grid(old_tokens, None)
    new_h, new_w = factor_token_grid(new_tokens, original_shape)
    if old_h * old_w != old_tokens or new_h * new_w != new_tokens:
        print(
            f"[ComfyCouple SayaPatch v3.3] impossible resize: masks={tuple(masks.shape)} target_tokens={new_tokens}"
        )
        return masks
    b, t, c = masks.shape
    m = masks.permute(0, 2, 1).contiguous().reshape(b * c, 1, old_h, old_w)
    m = F.interpolate(m.float(), size=(new_h, new_w), mode="nearest")
    m = m.reshape(b, c, new_tokens).permute(0, 2, 1).contiguous()
    m = m.to(device=masks.device, dtype=masks.dtype)
    m = m.clamp(0, 1)
    if attention_debug_enabled():
        print(
            f"[ComfyCouple SayaPatch v3.3] resized mask {old_tokens} ({old_h}x{old_w}) -> {new_tokens} ({new_h}x{new_w})"
        )
    return m


def fill_unassigned_mask_regions(masks_v: Any, original_shape: Any = None) -> Any:
    """Assign uncovered token positions to their nearest regional owner."""
    try:
        if not isinstance(masks_v, torch.Tensor) or masks_v.ndim != 4:
            return masks_v
        length, batch, tokens, channels = masks_v.shape
        if length <= 1 or tokens <= 0:
            return masks_v
        h, w = factor_token_grid(tokens, original_shape)
        if h * w != tokens:
            return masks_v
        out = masks_v.clone()
        weights = out.mean(dim=-1).clamp(0, 1)
        device = out.device
        dtype = out.dtype
        yy = torch.arange(h, device=device, dtype=dtype).view(h, 1).expand(h, w).reshape(tokens)
        xx = torch.arange(w, device=device, dtype=dtype).view(1, w).expand(h, w).reshape(tokens)
        total_filled = 0
        fully_empty_batches = 0
        for b in range(batch):
            wb = weights[:, b, :]
            token_sum = wb.sum(dim=0)
            empty = token_sum <= 1e-06
            if not bool(empty.any()):
                continue
            empty_idx = empty.nonzero(as_tuple=False).flatten()
            total_filled += int(empty_idx.numel())
            centers_x = []
            centers_y = []
            region_totals = wb.sum(dim=1)
            any_region_valid = bool((region_totals > 1e-06).any())
            if not any_region_valid:
                fully_empty_batches += 1
            for region in range(length):
                region_map = wb[region]
                region_total = region_map.sum()
                if region_total > 1e-06:
                    cx = (region_map * xx).sum() / region_total
                    cy = (region_map * yy).sum() / region_total
                else:
                    cx = torch.tensor(
                        (region + 0.5) * w / max(1, length), device=device, dtype=dtype
                    )
                    cy = torch.tensor((h - 1) * 0.5, device=device, dtype=dtype)
                centers_x.append(cx)
                centers_y.append(cy)
            centers_x = torch.stack(centers_x)
            centers_y = torch.stack(centers_y)
            ex = xx[empty_idx]
            ey = yy[empty_idx]
            dist = (centers_x[:, None] - ex[None, :]) ** 2 + (centers_y[:, None] - ey[None, :]) ** 2
            assign = dist.argmin(dim=0)
            out[:, b, empty_idx, :] = 0
            for region in range(length):
                idx = empty_idx[assign == region]
                if idx.numel() > 0:
                    out[region, b, idx, :] = 1
        try:
            import os

            debug = os.environ.get("SAYA_COUPLE_DEBUG", "0") == "1"
        except Exception:
            debug = False
        if debug and total_filled > 0:
            print(
                f"[ComfyCouple SayaPatch v3.3] filled empty mask tokens={total_filled}, fully_empty_batches={fully_empty_batches}, grid={h}x{w}"
            )
        return out
    except Exception as e:
        print(f"[ComfyCouple SayaPatch v3.3] nearest fill failed: {e}")
        return masks_v


def prepare_masks_for_attention_output(
    masks: Any, qkv: Any, original_shape: Any = None, label: str = ""
) -> Any:
    """Normalize and reshape masks to match an attention output tensor."""
    try:
        if not isinstance(masks, torch.Tensor):
            return masks
        masks = masks.to(device=qkv.device, dtype=qkv.dtype)
        if masks.ndim == 2:
            masks = masks.unsqueeze(-1)
        if masks.ndim != 3:
            print(
                f"[ComfyCouple SayaPatch v3.3] bad mask ndim={getattr(masks, 'ndim', None)}, fallback neutral"
            )
            return torch.ones_like(qkv)
        if masks.shape[1] != qkv.shape[1]:
            masks = resize_masks_to_token_grid(masks, qkv.shape[1], original_shape)
        if masks.shape[1] != qkv.shape[1]:
            print(
                f"[ComfyCouple SayaPatch v3.3] token mismatch still present masks={tuple(masks.shape)} qkv={tuple(qkv.shape)}, fallback neutral"
            )
            return torch.ones_like(qkv)
        if masks.shape[2] == 1 and qkv.shape[2] != 1:
            masks = masks.expand(-1, -1, qkv.shape[2])
        elif masks.shape[2] != qkv.shape[2]:
            if masks.shape[2] > qkv.shape[2]:
                masks = masks[:, :, : qkv.shape[2]]
            else:
                masks = masks[:, :, :1].expand(-1, -1, qkv.shape[2])
        if masks.shape[0] != qkv.shape[0]:
            old_b = masks.shape[0]
            target_b = qkv.shape[0]
            if old_b == 1:
                masks = masks.expand(target_b, -1, -1)
            else:
                reps = (target_b + old_b - 1) // old_b
                masks = masks.repeat(reps, 1, 1)[:target_b]
            print(f"[ComfyCouple SayaPatch v2] aligned batch {old_b} -> {target_b}")
        return masks
    except Exception as e:
        print(f"[ComfyCouple SayaPatch v3.3] prepare failed: {e}, fallback neutral")
        return torch.ones_like(qkv)


def build_query_token_masks(masks: Any, q: Any, original_shape: Any) -> Any:
    """Project spatial ownership masks onto the query token layout."""
    if original_shape[2] * original_shape[3] == q.shape[1]:
        down_sample_rate = 1
    elif original_shape[2] // 2 * (original_shape[3] // 2) == q.shape[1]:
        down_sample_rate = 2
    elif original_shape[2] // 4 * (original_shape[3] // 4) == q.shape[1]:
        down_sample_rate = 4
    else:
        down_sample_rate = 8
    ret_masks = []
    for mask in masks:
        if isinstance(mask, torch.Tensor):
            size = (original_shape[2] // down_sample_rate, original_shape[3] // down_sample_rate)
            mask_downsample = F.interpolate(mask.unsqueeze(0), size=size, mode="nearest")
            mask_downsample = mask_downsample.view(1, -1, 1).repeat(q.shape[0], 1, q.shape[2])
            ret_masks.append(mask_downsample)
        else:
            ret_masks.append(torch.ones_like(q))
    ret_masks = torch.cat(ret_masks, dim=0)
    return ret_masks

"""Forge Couple attention engine adapted to ComfyUI ModelPatcher.

The core equations are intentionally kept equivalent to:
    Haoming02/sd-forge-couple/lib_couple/attention_couple.py

Original credits in that project also point to laksjdjf/cgem156-ComfyUI.
The upstream project is GPL-3.0 licensed; see LICENSE.

Saya-specific differences are limited to:
* ComfyUI device/dtype resolution instead of Forge ``modules.devices``;
* an explicit RuntimeError (not a warning) if Forge's k == v precondition fails;
* region masks are supplied as one shared spatial mask and repeated across the
  actual latent batch, matching Forge's own mask convention.
"""

from __future__ import annotations

from typing import Any

import torch

from .attention_masks import get_mask, lcm_for_list


def _detailer_debug(message: str) -> None:
    import os

    if os.environ.get("SAYA_COUPLE_DEBUG", "0") == "1":
        print(f"[Saya Forge Couple] {message}")


def _read_detailer_crop_context(extra_options: Any) -> dict[str, Any] | None:
    """Read the ``saya_couple_crop`` metadata attached by SayaDetailerForEach.

    Same contract as the previous Comfy-Couple engine: the detailer clones the
    patched model and stores the active crop region (pixel coords + full image
    size) under ``transformer_options['saya_couple_crop']``.
    """
    if not isinstance(extra_options, dict):
        return None
    ctx = extra_options.get("saya_couple_crop")
    if not isinstance(ctx, dict):
        for key in ("transformer_options", "model_options"):
            nested = extra_options.get(key)
            if isinstance(nested, dict):
                ctx = nested.get("saya_couple_crop")
                if isinstance(ctx, dict):
                    break
    if not isinstance(ctx, dict):
        return None
    region = ctx.get("crop_region", ctx.get("bbox", ctx.get("crop")))
    if isinstance(region, (list, tuple)) and len(region) == 4:
        try:
            x1, y1, x2, y2 = (float(v) for v in region)
        except (TypeError, ValueError):
            return None
        if x2 > x1 and y2 > y1:
            return {
                "crop_region": (x1, y1, x2, y2),
                "full_width": ctx.get("full_width"),
                "full_height": ctx.get("full_height"),
                "label": str(ctx.get("label", "detailer")),
            }
    return None


def _slice_masks_to_crop(masks: torch.Tensor, ctx: dict[str, Any]) -> torch.Tensor:
    """Crop a ``[regions,1,H,W]`` full-image routing mask to the detailer region.

    Geometry matches the previous engine's ``crop_masks_to_detailer_region`` so a
    detailer crop inherits the ownership values of its own full-image location
    instead of receiving the whole image gradient squeezed into the crop.
    """
    x1, y1, x2, y2 = ctx["crop_region"]
    mh, mw = int(masks.shape[-2]), int(masks.shape[-1])
    full_w = max(1, int(ctx.get("full_width") or mw))
    full_h = max(1, int(ctx.get("full_height") or mh))
    sx, sy = mw / full_w, mh / full_h
    ix1 = max(0, min(mw - 1, int(x1 * sx)))
    iy1 = max(0, min(mh - 1, int(y1 * sy)))
    ix2 = max(ix1 + 1, min(mw, int(x2 * sx + 0.999999)))
    iy2 = max(iy1 + 1, min(mh, int(y2 * sy + 0.999999)))
    cropped = masks[..., iy1:iy2, ix1:ix2]
    _detailer_debug(
        f"detailer crop label={ctx['label']} region={(x1, y1, x2, y2)} "
        f"mask={mh}x{mw} -> {cropped.shape[-2]}x{cropped.shape[-1]}"
    )
    return cropped


def _is_foreign_smaller_pass(masks: torch.Tensor, original_shape: Any) -> bool:
    """Detect a crop/tile pass with no crop metadata (e.g. small USDU tiles).

    Same thresholds as the previous engine's ``masks_match_attention_pass``:
    when the current pass samples a much smaller region than the routing masks
    describe, the masks cannot be mapped faithfully and regional routing must
    step aside (base branch only) instead of squeezing the full-image gradient
    into the small pass.
    """
    try:
        mh, mw = int(masks.shape[-2]), int(masks.shape[-1])
        oh, ow = int(original_shape[-2]), int(original_shape[-1])
    except Exception:
        return False
    if mh <= 0 or mw <= 0 or oh <= 0 or ow <= 0:
        return False
    approx_ph, approx_pw = oh * 8, ow * 8
    much_smaller = approx_ph < mh * 0.7 and approx_pw < mw * 0.7
    aspect = (mw / max(1, mh)) / max(1e-06, ow / max(1, oh))
    aspect_mismatch = max(aspect, 1.0 / aspect) > 1.35
    return much_smaller or aspect_mismatch


class AttentionCouple:
    """Patch only cross-attention (attn2). Self-attention remains untouched."""

    @staticmethod
    def _runtime_device_dtype(model: Any) -> tuple[torch.device, torch.dtype]:
        try:
            import comfy.model_management as model_management

            device = model_management.get_torch_device()
        except Exception:
            # Import-time/unit-test fallback. Real ComfyUI runs take the branch above.
            device = torch.device("cpu")

        dm = model.model.diffusion_model
        dtype = getattr(dm, "dtype", torch.float32)
        if not isinstance(dtype, torch.dtype):
            dtype = torch.float32
        return torch.device(device), dtype

    @classmethod
    @torch.inference_mode()
    def patch_unet(
        cls,
        model: Any,
        base_mask: torch.Tensor,
        kwargs: dict[str, Any],
        *,
        coarse_main_strength: float = 0.0,
        coarse_max_tokens: int = 0,
    ) -> Any:
        """Clone ``model`` and install Forge-equivalent attn2 input/output patches."""
        m = model.clone()
        num_conds = len(kwargs) // 2 + 1
        if num_conds < 2:
            raise RuntimeError("Saya Forge Couple: no regional conditioning supplied")

        device, dtype = cls._runtime_device_dtype(model)

        masks = [base_mask] + [kwargs[f"mask_{i}"] for i in range(1, num_conds)]
        masks = [cls._forge_mask_shape(x, index=i) for i, x in enumerate(masks)]
        mask_fine = torch.stack(masks, dim=0).to(device=device, dtype=dtype)

        if mask_fine.sum(dim=0).min().item() <= 0.0:
            raise RuntimeError("Saya Forge Couple: masks must completely fill the image")
        mask_fine = mask_fine / mask_fine.sum(dim=0, keepdim=True)

        # Contact 2.1 remains untouched at detail resolutions. At coarse cross-attention
        # resolutions only, transfer a bounded fraction of the remaining private budget
        # to the sampler's MAIN/base branch. This gives scene/background a direct path
        # without globally diluting P1/P2 where fine attributes are resolved.
        coarse_strength = min(0.50, max(0.0, float(coarse_main_strength)))
        coarse_limit = max(0, int(coarse_max_tokens))
        if coarse_strength > 0.0 and coarse_limit > 0:
            mask_coarse = mask_fine.clone()
            base = mask_fine[0:1]
            private = mask_fine[1:]
            mask_coarse[0:1] = base + (1.0 - base) * coarse_strength
            mask_coarse[1:] = private * (1.0 - coarse_strength)
            # Algebraically already sums to one; normalize once to avoid dtype drift.
            mask_coarse = mask_coarse / mask_coarse.sum(dim=0, keepdim=True)
        else:
            mask_coarse = mask_fine

        conds = [
            kwargs[f"cond_{i}"][0][0].to(device=device, dtype=dtype)
            for i in range(1, num_conds)
        ]
        num_tokens = [int(cond.shape[1]) for cond in conds]

        # Keep batch size in this patch closure. Forge stores it on the class; a
        # closure avoids cross-talk when several patched ComfyUI model branches coexist.
        state = {"batch_size": None}

        @torch.inference_mode()
        def attn2_patch(q: torch.Tensor, k: torch.Tensor, v: torch.Tensor, extra_options: dict):
            # This is an upstream Forge invariant. Never silently continue if a
            # different ComfyUI attention implementation violates it, because the
            # returned tuple deliberately uses ks for BOTH key and value.
            if not torch.allclose(k, v):
                raise RuntimeError(
                    "Saya Forge Couple: Forge attn2 requires k and v to be equal; "
                    "refusing to replace v silently"
                )

            cond_or_unconds = extra_options["cond_or_uncond"]
            num_chunks = len(cond_or_unconds)
            if num_chunks < 1 or q.shape[0] % num_chunks != 0:
                raise RuntimeError(
                    "Saya Forge Couple: unexpected cond/uncond batching "
                    f"q_batch={q.shape[0]} chunks={num_chunks}"
                )

            batch_size = q.shape[0] // num_chunks
            state["batch_size"] = batch_size
            q_chunks = q.chunk(num_chunks, dim=0)
            k_chunks = k.chunk(num_chunks, dim=0)
            lcm_tokens = lcm_for_list(num_tokens + [int(k.shape[1])])

            # Match the current attention compute dtype/device. Under normal SDXL
            # this is already identical to the install-time dtype; doing this here
            # also keeps the patch safe under ComfyUI casting/offload paths.
            conds_tensor = torch.cat(
                [
                    cond.to(device=k.device, dtype=k.dtype).repeat(
                        batch_size, lcm_tokens // num_tokens[i], 1
                    )
                    for i, cond in enumerate(conds)
                ],
                dim=0,
            )

            qs, ks = [], []
            for i, cond_or_uncond in enumerate(cond_or_unconds):
                k_target = k_chunks[i].repeat(1, lcm_tokens // k.shape[1], 1)
                if cond_or_uncond == 1:  # uncond
                    qs.append(q_chunks[i])
                    ks.append(k_target)
                else:  # cond
                    qs.append(q_chunks[i].repeat(num_conds, 1, 1))
                    ks.append(torch.cat([k_target, conds_tensor], dim=0))

            qs = torch.cat(qs, dim=0).to(q)
            ks = torch.cat(ks, dim=0).to(k)

            # Forge pads odd batches because some optimized attention backends expect
            # an even leading dimension. The output hook trims the unused tail by
            # consuming only the expected branch sizes.
            if qs.size(0) % 2 == 1:
                qs = torch.cat((qs, torch.zeros_like(qs[0]).unsqueeze(0)), dim=0)
                ks = torch.cat((ks, torch.zeros_like(ks[0]).unsqueeze(0)), dim=0)

            return qs, ks, ks

        @torch.inference_mode()
        def attn2_output_patch(out: torch.Tensor, extra_options: dict):
            cond_or_unconds = extra_options["cond_or_uncond"]
            batch_size = state.get("batch_size")
            if not isinstance(batch_size, int) or batch_size < 1:
                raise RuntimeError("Saya Forge Couple: attn2 output hook ran before input hook")

            selected_mask = (
                mask_coarse
                if coarse_strength > 0.0 and coarse_limit > 0 and out.shape[1] <= coarse_limit
                else mask_fine
            )
            # Detailer/tile passes sample a sub-region of the full image. Route by
            # that region's own full-image ownership values, or step aside when no
            # faithful mapping exists. Full-frame passes are untouched.
            crop_ctx = _read_detailer_crop_context(extra_options)
            if crop_ctx is not None:
                selected_mask = _slice_masks_to_crop(selected_mask, crop_ctx)
            elif _is_foreign_smaller_pass(selected_mask, extra_options["original_shape"]):
                neutral = torch.zeros_like(selected_mask)
                neutral[0:1] = 1.0
                _detailer_debug(
                    "mismatched pass without crop context: routing base branch only "
                    f"(mask={tuple(selected_mask.shape)} original_shape={extra_options['original_shape']})"
                )
                selected_mask = neutral
            mask_downsample = get_mask(
                selected_mask.to(device=out.device, dtype=out.dtype),
                batch_size,
                out.shape[1],
                extra_options["original_shape"],
            )
            outputs = []
            pos = 0
            for cond_or_uncond in cond_or_unconds:
                if cond_or_uncond == 1:  # uncond
                    outputs.append(out[pos : pos + batch_size])
                    pos += batch_size
                else:
                    region_count = num_conds * batch_size
                    masked_output = (
                        out[pos : pos + region_count] * mask_downsample
                    ).view(num_conds, batch_size, out.shape[1], out.shape[2])
                    outputs.append(masked_output.sum(dim=0))
                    pos += region_count

            if not outputs:
                raise RuntimeError("Saya Forge Couple: empty attn2 output")
            return torch.cat(outputs, dim=0)

        m.set_model_attn2_patch(attn2_patch)
        m.set_model_attn2_output_patch(attn2_output_patch)
        return m

    @staticmethod
    def _forge_mask_shape(mask: torch.Tensor, *, index: int) -> torch.Tensor:
        """Return Forge's expected ``[1,H,W]`` region mask.

        Saya exposes masks as ``[latent_batch,H,W]``. Their geometry is identical
        for each batch item, so attention routing needs one copy and Forge repeats it
        internally for the actual sample batch.
        """
        if not isinstance(mask, torch.Tensor):
            raise RuntimeError(f"Saya Forge Couple: mask_{index} is not a tensor")
        if mask.ndim == 2:
            return mask.unsqueeze(0)
        if mask.ndim == 3:
            if mask.shape[0] < 1:
                raise RuntimeError(f"Saya Forge Couple: mask_{index} has empty batch")
            return mask[:1]
        if mask.ndim == 4 and mask.shape[1] == 1:
            if mask.shape[0] < 1:
                raise RuntimeError(f"Saya Forge Couple: mask_{index} has empty batch")
            return mask[:1, 0]
        raise RuntimeError(
            f"Saya Forge Couple: mask_{index} must be [H,W], [B,H,W] or [B,1,H,W], "
            f"got {tuple(mask.shape)}"
        )

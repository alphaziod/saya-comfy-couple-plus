"""ANIMA-specific regional attention hooks."""

from __future__ import annotations

import types
from typing import Any, Self

import torch

from .context import (
    apply_region_strength,
    attention_debug_enabled,
    resolve_attention_mode,
    resolve_region_strength,
)
from .masks import (
    build_query_token_masks,
    fill_unassigned_mask_regions,
    masks_match_attention_pass,
    prepare_masks_for_attention_output,
)
from .operations import calculate_regional_blend


class AnimaAttentionMixin:
    """ANIMA-specific regional attention implementation shared by the node class."""

    def prepare_anima_context(
        self: Self,
        ctx: Any,
        batch_size: int,
        x: Any,
        reference_context: Any = None,
        label: str = "",
    ) -> Any:
        """Normalize ANIMA context tensors for one regional batch."""
        if not isinstance(ctx, torch.Tensor):
            return ctx
        if ctx.ndim != 3:
            return ctx.to(device=x.device)
        if isinstance(reference_context, torch.Tensor) and reference_context.ndim == 3:
            if ctx.shape[-1] != reference_context.shape[-1]:
                raise RuntimeError(
                    f"[ComfyCouple ANIMA] {label} conditioning dim mismatch: region_dim={ctx.shape[-1]} current_context_dim={reference_context.shape[-1]}. ANIMA must receive ANIMA-encoded conditionings, not SDXL conditionings."
                )
        if ctx.shape[0] == batch_size:
            out = ctx
        elif ctx.shape[0] == 1:
            out = ctx.repeat(batch_size, 1, 1)
        else:
            reps = (batch_size + ctx.shape[0] - 1) // ctx.shape[0]
            out = ctx.repeat(reps, 1, 1)[:batch_size]
        target_dtype = (
            reference_context.dtype if isinstance(reference_context, torch.Tensor) else x.dtype
        )
        return out.to(device=x.device, dtype=target_dtype)

    def install_anima_patch(self: Self, new_model: Any, dm: Any) -> None:
        """Install regional forwarding hooks on supported ANIMA attention blocks."""
        patched_count = 0
        for block_id, block in enumerate(getattr(dm, "blocks", [])):
            if not hasattr(block, "cross_attn"):
                continue
            attn = block.cross_attn
            if getattr(attn, "_saya_anima_real_hooked", False):
                continue
            orig_forward = attn.forward
            printed_flag = {"done": False}

            def saya_anima_forward(
                self_attn: Any,
                x: Any,
                context: Any = None,
                rope_emb: Any = None,
                transformer_options: Any = None,
                _orig: Any = orig_forward,
                _bid: Any = block_id,
                _printed: Any = printed_flag,
                **kwargs: Any,
            ) -> Any:
                """Execute the saya_anima_forward operation for this module."""
                return self.run_anima_regional_forward(
                    _orig,
                    _bid,
                    _printed,
                    x,
                    context=context,
                    rope_emb=rope_emb,
                    transformer_options=transformer_options,
                    **kwargs,
                )

            attn.forward = types.MethodType(saya_anima_forward, attn)
            attn._saya_anima_real_hooked = True
            attn._saya_anima_hooked = True
            patched_count += 1
        if patched_count <= 0:
            raise RuntimeError(
                "[ComfyCouple ANIMA] No dm.blocks[*].cross_attn.forward hooks were installed"
            )
        print(f"[ComfyCouple ANIMA] real cross_attn hooks installed: {patched_count}")

    def run_anima_regional_forward(
        self: Self,
        original_forward: Any,
        block_id: int,
        printed_flag: Any,
        x: Any,
        context: Any = None,
        rope_emb: Any = None,
        transformer_options: Any = None,
        **kwargs: Any,
    ) -> Any:
        """Run one ANIMA attention block with regional ownership and fallback blending."""
        if transformer_options is None:
            transformer_options = {}
        if context is None or not isinstance(transformer_options, dict):
            return original_forward(
                x,
                context=context,
                rope_emb=rope_emb,
                transformer_options=transformer_options,
                **kwargs,
            )
        cond_or_uncond = transformer_options.get("cond_or_uncond", None)
        original_shape = transformer_options.get("original_shape", None)
        if cond_or_uncond is None or original_shape is None or x.ndim != 3:
            return original_forward(
                x,
                context=context,
                rope_emb=rope_emb,
                transformer_options=transformer_options,
                **kwargs,
            )
        chunks = len(cond_or_uncond)
        if chunks <= 0 or x.shape[0] % chunks != 0:
            return original_forward(
                x,
                context=context,
                rope_emb=rope_emb,
                transformer_options=transformer_options,
                **kwargs,
            )
        if not printed_flag["done"]:
            try:
                print(
                    f"[ComfyCouple ANIMA] block={block_id} real couple active x={tuple(x.shape)} context={tuple(context.shape)} cond_or_uncond={cond_or_uncond} original_shape={original_shape}"
                )
            except Exception as e:
                print(f"[ComfyCouple ANIMA] debug print failed block={block_id}: {e}")
            printed_flag["done"] = True
        len_neg, len_pos = self.conditioning_length
        x_list = x.chunk(chunks, dim=0)
        context_list = None
        if (
            isinstance(context, torch.Tensor)
            and context.ndim == 3
            and (context.shape[0] % chunks == 0)
        ):
            context_list = context.chunk(chunks, dim=0)
        out_chunks = []
        for chunk_index, c in enumerate(cond_or_uncond):
            x_chunk = x_list[chunk_index]
            b = x_chunk.shape[0]
            if c == 0:
                region_conds = self.negative_positive_conds[1]
                source_masks = self.negative_positive_masks[1]
                length = len_pos
                label = "cond"
            else:
                region_conds = self.negative_positive_conds[0]
                source_masks = self.negative_positive_masks[0]
                length = len_neg
                label = "uncond"
            local_options = dict(transformer_options)
            local_options["cond_or_uncond"] = [c]
            region_outputs = []
            for region_index in range(length):
                region_context = self.prepare_anima_context(
                    region_conds[region_index],
                    b,
                    x_chunk,
                    reference_context=context,
                    label=f"{label}[{region_index}]",
                )
                region_out = original_forward(
                    x_chunk,
                    context=region_context,
                    rope_emb=rope_emb,
                    transformer_options=local_options,
                    **kwargs,
                )
                region_outputs.append(region_out)
            qkv = torch.cat(region_outputs, dim=0)
            try:
                masks = build_query_token_masks(source_masks, x_chunk, original_shape)
                masks = prepare_masks_for_attention_output(masks, qkv, original_shape, label)
                if (
                    isinstance(masks, torch.Tensor)
                    and masks.ndim == 3
                    and (masks.shape[0] == length * b)
                ):
                    mt, tt, cc = masks.shape
                    masks_v = masks.contiguous().view(length, b, tt, cc)
                    v3_mode = resolve_attention_mode(
                        getattr(self, "saya_mode", "GEN"), original_shape, qkv
                    )
                    masks_aligned, align_reason = masks_match_attention_pass(
                        source_masks, original_shape
                    )
                    local_safe = v3_mode == "DETAILER" or not masks_aligned
                    if not local_safe:
                        masks_v = fill_unassigned_mask_regions(masks_v, original_shape)
                    masks_v = masks_v / masks_v.sum(dim=0, keepdim=True).clamp_min(1e-06)
                    v3_strength = resolve_region_strength(
                        v3_mode, getattr(self, "saya_strength", -1.0)
                    )
                    if local_safe:
                        v3_strength = min(max(v3_strength, 0.9), 1.0)
                    masks_v = apply_region_strength(masks_v, v3_strength)
                    couple_blend = calculate_regional_blend(
                        v3_mode,
                        masks_aligned=masks_aligned,
                        custom_strength=getattr(self, "saya_strength", -1.0),
                    )
                    if attention_debug_enabled(getattr(self, "saya_debug", False)):
                        print(
                            f"[ComfyCouple ANIMA] block={block_id} mode={v3_mode} mask_strength={v3_strength:.2f} blend={couple_blend:.2f} aligned={masks_aligned} local_safe={local_safe} reason={align_reason} masks={tuple(masks_v.shape)} qkv={tuple(qkv.shape)}"
                        )
                    masks = masks_v.contiguous().view(length * b, tt, cc)
                else:
                    if attention_debug_enabled(getattr(self, "saya_debug", False)):
                        print(
                            f"[ComfyCouple ANIMA] block={block_id} bad mask group shape masks={getattr(masks, 'shape', None)} qkv={tuple(qkv.shape)}, neutral fallback"
                        )
                    tt = qkv.shape[1]
                    cc = qkv.shape[2]
                    masks = torch.ones(
                        (length * b, tt, cc), device=qkv.device, dtype=qkv.dtype
                    ) / max(1, length)
                    v3_mode = resolve_attention_mode(
                        getattr(self, "saya_mode", "GEN"), original_shape, qkv
                    )
                    couple_blend = 1.0
                    local_safe = True
            except Exception as e:
                print(
                    f"[ComfyCouple ANIMA] block={block_id} mask ownership failed: {e}, using neutral fallback"
                )
                masks = torch.ones_like(qkv) / max(1, length)
                v3_mode = resolve_attention_mode(
                    getattr(self, "saya_mode", "GEN"), original_shape, qkv
                )
                couple_blend = 1.0
                local_safe = True
            qkv_regional = qkv * masks
            qkv_regional = qkv_regional.view(length, b, qkv.shape[1], qkv.shape[2]).sum(dim=0)
            normal_out = None
            if (local_safe or couple_blend < 0.999) and context_list is not None:
                try:
                    normal_context = self.prepare_anima_context(
                        context_list[chunk_index],
                        b,
                        x_chunk,
                        reference_context=context,
                        label=f"{label}[normal]",
                    )
                    normal_out = original_forward(
                        x_chunk,
                        context=normal_context,
                        rope_emb=rope_emb,
                        transformer_options=local_options,
                        **kwargs,
                    )
                except Exception as e:
                    if attention_debug_enabled(getattr(self, "saya_debug", False)):
                        print(
                            f"[ComfyCouple ANIMA] block={block_id} normal fallback unavailable: {e}"
                        )
                    normal_out = None
            if normal_out is not None:
                out_chunk = normal_out * (1.0 - couple_blend) + qkv_regional * couple_blend
            else:
                out_chunk = qkv_regional
            out_chunks.append(out_chunk)
        return torch.cat(out_chunks, dim=0)

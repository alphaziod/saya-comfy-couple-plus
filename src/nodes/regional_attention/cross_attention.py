"""Cross-attention replacement for standard ComfyUI attention blocks."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Self

import torch
from comfy.ldm.modules.attention import optimized_attention

from .context import (
    apply_region_strength,
    attention_debug_enabled,
    concatenate_context_tensors,
    resolve_attention_mode,
    resolve_region_strength,
)
from .masks import (
    build_query_token_masks,
    crop_masks_to_detailer_region,
    extract_detailer_crop_context,
    fill_unassigned_mask_regions,
    masks_match_attention_pass,
    prepare_masks_for_attention_output,
)
from .operations import calculate_regional_blend, try_standard_attention


@dataclass(frozen=True, slots=True)
class BranchProjection:
    """Projected conditioning and masks for one conditional branch."""

    masks: Any
    source_masks: Any
    keys: Any
    values: Any
    region_count: int


class CrossAttentionPatch:
    """Callable regional cross-attention patch bound to one node and module."""

    def __init__(self, owner: Any, module: Any) -> None:
        """Store the node state and target attention module."""
        self.owner = owner
        self.module = module

    def __call__(self, q: Any, k: Any, v: Any, extra_options: dict[str, Any]) -> Any:
        """Apply regional ownership to every conditional branch in the batch."""
        self.owner._last_extra_options = extra_options
        condition_flags = extra_options["cond_or_uncond"]
        query_chunks = q.chunk(len(condition_flags), dim=0)
        batch_size = int(query_chunks[0].shape[0])
        conditional, unconditional = self._build_branch_projections(query_chunks[0], extra_options)

        outputs = []
        for branch_index, condition_flag in enumerate(condition_flags):
            projection = conditional if condition_flag == 0 else unconditional
            outputs.append(
                self._process_branch(
                    query=query_chunks[branch_index],
                    original_keys=k,
                    original_values=v,
                    branch_index=branch_index,
                    branch_count=len(condition_flags),
                    batch_size=batch_size,
                    projection=projection,
                    extra_options=extra_options,
                )
            )
        return torch.cat(outputs, dim=0)

    def _build_branch_projections(
        self,
        reference_query: Any,
        extra_options: dict[str, Any],
    ) -> tuple[BranchProjection, BranchProjection]:
        """Build mask and key/value projections for conditional and unconditional branches."""
        crop_context = extract_detailer_crop_context(extra_options)
        use_detailer_bank = bool(
            crop_context
            and getattr(self.owner, "detailer_negative_positive_masks", None)
            and getattr(self.owner, "detailer_negative_positive_conds", None)
        )
        mask_bank = (
            self.owner.detailer_negative_positive_masks
            if use_detailer_bank
            else self.owner.negative_positive_masks
        )
        cond_bank = (
            self.owner.detailer_negative_positive_conds
            if use_detailer_bank
            else self.owner.negative_positive_conds
        )
        count_bank = (
            self.owner.detailer_conditioning_length
            if use_detailer_bank and getattr(self.owner, "detailer_conditioning_length", None)
            else self.owner.conditioning_length
        )
        unconditional_masks = crop_masks_to_detailer_region(mask_bank[0], crop_context, "uncond")
        conditional_masks = crop_masks_to_detailer_region(mask_bank[1], crop_context, "cond")
        original_shape = extra_options["original_shape"]
        unconditional_query_masks = build_query_token_masks(
            unconditional_masks, reference_query, original_shape
        )
        conditional_query_masks = build_query_token_masks(
            conditional_masks, reference_query, original_shape
        )

        unconditional_context = concatenate_context_tensors(cond_bank[0], dim=0)
        conditional_context = concatenate_context_tensors(cond_bank[1], dim=0)
        negative_count, positive_count = count_bank
        return (
            BranchProjection(
                masks=conditional_query_masks,
                source_masks=conditional_masks,
                keys=self.module.to_k(conditional_context),
                values=self.module.to_v(conditional_context),
                region_count=positive_count,
            ),
            BranchProjection(
                masks=unconditional_query_masks,
                source_masks=unconditional_masks,
                keys=self.module.to_k(unconditional_context),
                values=self.module.to_v(unconditional_context),
                region_count=negative_count,
            ),
        )

    def _process_branch(
        self,
        *,
        query: Any,
        original_keys: Any,
        original_values: Any,
        branch_index: int,
        branch_count: int,
        batch_size: int,
        projection: BranchProjection,
        extra_options: dict[str, Any],
    ) -> Any:
        """Compute one branch and blend regional output with standard attention when needed."""
        repeated_query = query.repeat(projection.region_count, 1, 1)
        keys = self._repeat_projection(projection.keys, projection.region_count, batch_size)
        values = self._repeat_projection(projection.values, projection.region_count, batch_size)
        if keys.dtype != repeated_query.dtype or values.dtype != repeated_query.dtype:
            keys = keys.to(repeated_query.dtype)
            values = values.to(repeated_query.dtype)

        standard_output = try_standard_attention(
            query,
            original_keys,
            original_values,
            branch_count,
            branch_index,
            extra_options["n_heads"],
        )
        regional_output = optimized_attention(
            repeated_query,
            keys,
            values,
            extra_options["n_heads"],
        )
        masks = prepare_masks_for_attention_output(
            projection.masks,
            regional_output,
            extra_options.get("original_shape"),
            "cond" if branch_index == 0 else "uncond",
        )
        normalized_masks, blend, local_fallback = self._normalize_masks(
            masks=masks,
            source_masks=projection.source_masks,
            regional_output=regional_output,
            region_count=projection.region_count,
            batch_size=batch_size,
            original_shape=extra_options.get("original_shape"),
        )
        regional_output = regional_output * normalized_masks
        regional_output = regional_output.view(
            projection.region_count,
            batch_size,
            -1,
            self.module.heads * self.module.dim_head,
        ).sum(dim=0)

        if standard_output is not None and (local_fallback or blend < 0.999):
            return standard_output * (1.0 - blend) + regional_output * blend
        return regional_output

    def _normalize_masks(
        self,
        *,
        masks: Any,
        source_masks: Any,
        regional_output: Any,
        region_count: int,
        batch_size: int,
        original_shape: Any,
    ) -> tuple[Any, float, bool]:
        """Normalize mask ownership and return blend metadata for one branch."""
        mode = resolve_attention_mode(
            getattr(self.owner, "saya_mode", "GEN"),
            original_shape,
            regional_output,
        )
        if extract_detailer_crop_context(getattr(self.owner, "_last_extra_options", {}) or {}):
            mode = "DETAILER"
        try:
            expected_batch = region_count * batch_size
            if not isinstance(masks, torch.Tensor) or masks.ndim != 3:
                raise ValueError(f"unexpected mask rank: {getattr(masks, 'shape', None)}")
            if masks.shape[0] != expected_batch:
                raise ValueError(f"unexpected mask batch: {masks.shape[0]} != {expected_batch}")

            _, token_count, channels = masks.shape
            grouped_masks = masks.contiguous().view(
                region_count,
                batch_size,
                token_count,
                channels,
            )
            aligned, reason = masks_match_attention_pass(source_masks, original_shape)
            local_fallback = mode == "DETAILER" or not aligned
            if not local_fallback:
                grouped_masks = fill_unassigned_mask_regions(grouped_masks, original_shape)
            grouped_masks = grouped_masks / grouped_masks.sum(dim=0, keepdim=True).clamp_min(1e-06)

            strength = resolve_region_strength(
                mode,
                getattr(self.owner, "saya_strength", -1.0),
            )
            if local_fallback:
                strength = min(max(strength, 0.9), 1.0)
            grouped_masks = apply_region_strength(grouped_masks, strength)
            blend = calculate_regional_blend(
                mode,
                masks_aligned=aligned,
                custom_strength=getattr(self.owner, "saya_strength", -1.0),
            )
            self._log_mask_state(
                mode=mode,
                strength=strength,
                blend=blend,
                aligned=aligned,
                local_fallback=local_fallback,
                reason=reason,
                masks=grouped_masks,
                output=regional_output,
            )
            return (
                grouped_masks.contiguous().view(expected_batch, token_count, channels),
                blend,
                local_fallback,
            )
        except Exception as error:
            print(
                "[ComfyCouple SayaPatch v3.3] mask ownership failed: "
                f"{error}; using neutral fallback"
            )
            neutral_masks = torch.ones_like(regional_output) / max(1, region_count)
            return neutral_masks, 0.0, True

    def _log_mask_state(self, **state: Any) -> None:
        """Log mask ownership details when regional-attention diagnostics are enabled."""
        if not attention_debug_enabled(getattr(self.owner, "saya_debug", False)):
            return
        print(
            "[ComfyCouple SayaPatch v3.3] "
            f"mode={state['mode']} mask_strength={state['strength']:.2f} "
            f"blend={state['blend']:.2f} aligned={state['aligned']} "
            f"local_safe={state['local_fallback']} reason={state['reason']} "
            f"masks={tuple(state['masks'].shape)} qkv={tuple(state['output'].shape)}"
        )

    @staticmethod
    def _repeat_projection(projection: Any, region_count: int, batch_size: int) -> Any:
        """Repeat each regional key/value projection for every image in the batch."""
        return torch.cat(
            [
                projection[index].unsqueeze(0).repeat(batch_size, 1, 1)
                for index in range(region_count)
            ],
            dim=0,
        )


class CrossAttentionPatchMixin:
    """Factory mixin used by the regional-attention node."""

    def create_cross_attention_patch(self: Self, module: Any) -> CrossAttentionPatch:
        """Create the callable cross-attention replacement installed on a model block."""
        return CrossAttentionPatch(self, module)

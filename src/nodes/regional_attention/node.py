"""ComfyUI regional-attention node with standard and ANIMA patch support."""

from __future__ import annotations

import copy
from typing import Any, Self

import comfy
import torch

from .anima import AnimaAttentionMixin
from .cross_attention import CrossAttentionPatchMixin
from .replacement import install_attention_replacement


class RegionalAttentionNode(AnimaAttentionMixin, CrossAttentionPatchMixin):
    """ComfyUI node that installs character-aware regional attention on a model."""

    RETURN_TYPES = ("MODEL", "CONDITIONING", "CONDITIONING")
    RETURN_NAMES = ("model", "positive", "negative")
    FUNCTION = "attention_couple"
    CATEGORY = "Saya/Conditioning"

    @classmethod
    def INPUT_TYPES(s: type[Self]) -> dict[str, Any]:
        """Return the ComfyUI input schema exposed by this node."""
        return {
            "required": {
                "model": ("MODEL",),
                "positive": ("CONDITIONING",),
                "negative": ("CONDITIONING",),
                "mode": (["Attention", "Latent"],),
            },
            "optional": {
                "detailer_positive": ("CONDITIONING",),
                "public_positive": ("CONDITIONING",),
                "saya_mode": (["GEN", "REFINER", "DETAILER", "AUTO"], {"default": "GEN"}),
                "saya_strength": (
                    "FLOAT",
                    {"default": -1.0, "min": -1.0, "max": 1.5, "step": 0.05},
                ),
                "saya_debug": ("BOOLEAN", {"default": False}),
            },
        }

    def attention_couple(
        self: Self,
        model: Any,
        positive: Any,
        negative: Any,
        mode: str,
        detailer_positive: Any = None,
        public_positive: Any = None,
        saya_mode: str = "GEN",
        saya_strength: float = -1.0,
        saya_debug: bool = False,
    ) -> Any:
        """Clone a model and install regional attention patches for positive and negative conditioning."""
        if mode == "Latent":
            return (model, positive, negative)
        self.saya_mode = saya_mode
        self.saya_strength = saya_strength
        self.saya_debug = bool(saya_debug)
        new_positive = copy.deepcopy(positive)
        new_negative = copy.deepcopy(negative)
        detailer_positive = copy.deepcopy(detailer_positive) if detailer_positive else None
        returned_positive = copy.deepcopy(public_positive) if public_positive else None
        dtype = model.model.diffusion_model.dtype
        device = comfy.model_management.get_torch_device()

        def build_conditioning_bank(negative_conditions: Any, positive_conditions: Any) -> Any:
            banks_masks = []
            banks_conds = []
            neg_copy = copy.deepcopy(negative_conditions)
            pos_copy = copy.deepcopy(positive_conditions)
            for conditions in [neg_copy, pos_copy]:
                conditions_masks = []
                conditions_conds = []
                if len(conditions) != 1:
                    mask_dtype = torch.bfloat16 if "float8" in str(dtype) else dtype
                    mask_norm = torch.stack(
                        [
                            cond[1]["mask"].to(device, dtype=mask_dtype) * cond[1]["mask_strength"]
                            for cond in conditions
                        ]
                    )
                    mask_sum = mask_norm.sum(dim=0).clamp_min(1e-06)
                    mask_norm = mask_norm / mask_sum
                    conditions_masks.extend([mask_norm[i] for i in range(mask_norm.shape[0])])
                    conditions_conds.extend([cond[0].to(device, dtype=dtype) for cond in conditions])
                    if "mask" in conditions[0][1]:
                        del conditions[0][1]["mask"]
                    if "mask_strength" in conditions[0][1]:
                        del conditions[0][1]["mask_strength"]
                else:
                    conditions_masks = [False]
                    conditions_conds = [conditions[0][0].to(device, dtype=dtype)]
                banks_masks.append(conditions_masks)
                banks_conds.append(conditions_conds)
            return banks_masks, banks_conds, (len(neg_copy), len(pos_copy))

        (
            self.negative_positive_masks,
            self.negative_positive_conds,
            self.conditioning_length,
        ) = build_conditioning_bank(new_negative, new_positive)
        self.detailer_negative_positive_masks = None
        self.detailer_negative_positive_conds = None
        self.detailer_conditioning_length = None
        if detailer_positive:
            (
                self.detailer_negative_positive_masks,
                self.detailer_negative_positive_conds,
                self.detailer_conditioning_length,
            ) = build_conditioning_bank(new_negative, detailer_positive)
        new_model = model.clone()
        dm = new_model.model.diffusion_model
        self.sdxl = hasattr(dm, "label_emb")
        has_unet_blocks = (
            hasattr(dm, "input_blocks")
            and hasattr(dm, "middle_block")
            and hasattr(dm, "output_blocks")
        )
        has_anima_blocks = hasattr(dm, "blocks") and any(
            (hasattr(block, "cross_attn") for block in getattr(dm, "blocks", []))
        )
        if not has_unet_blocks:
            if has_anima_blocks:
                print(
                    "[ComfyCouple ANIMA] transformer model detected: installing real cross_attn regional hooks"
                )
                self.install_anima_patch(new_model, dm)
                return (new_model, [new_positive[0]], [new_negative[0]])
            print(
                f"[ComfyCouple ANIMA] Unsupported architecture type={type(dm).__name__}; no UNet blocks and no dm.blocks[*].cross_attn found."
            )
            return (new_model, [new_positive[0]], [new_negative[0]])
        if not self.sdxl:
            for id in [1, 2, 4, 5, 7, 8]:
                install_attention_replacement(
                    new_model,
                    self.create_cross_attention_patch(
                        dm.input_blocks[id][1].transformer_blocks[0].attn2
                    ),
                    ("input", id),
                )
            install_attention_replacement(
                new_model,
                self.create_cross_attention_patch(dm.middle_block[1].transformer_blocks[0].attn2),
                ("middle", 0),
            )
            for id in [3, 4, 5, 6, 7, 8, 9, 10, 11]:
                install_attention_replacement(
                    new_model,
                    self.create_cross_attention_patch(
                        dm.output_blocks[id][1].transformer_blocks[0].attn2
                    ),
                    ("output", id),
                )
        else:
            for id in [4, 5, 7, 8]:
                block_indices = range(2) if id in [4, 5] else range(10)
                for index in block_indices:
                    install_attention_replacement(
                        new_model,
                        self.create_cross_attention_patch(
                            dm.input_blocks[id][1].transformer_blocks[index].attn2
                        ),
                        ("input", id, index),
                    )
            for index in range(10):
                install_attention_replacement(
                    new_model,
                    self.create_cross_attention_patch(
                        dm.middle_block[1].transformer_blocks[index].attn2
                    ),
                    ("middle", 0, index),
                )
            for id in range(6):
                block_indices = range(2) if id in [3, 4, 5] else range(10)
                for index in block_indices:
                    install_attention_replacement(
                        new_model,
                        self.create_cross_attention_patch(
                            dm.output_blocks[id][1].transformer_blocks[index].attn2
                        ),
                        ("output", id, index),
                    )
        return (new_model, (returned_positive if returned_positive else [new_positive[0]]), [new_negative[0]])

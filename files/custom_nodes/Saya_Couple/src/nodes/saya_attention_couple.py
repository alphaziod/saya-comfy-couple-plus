# SPDX-License-Identifier: AGPL-3.0-or-later
# Saya Attention Couple PPM — local vendor copy of "Attention Couple (PPM)" (pamparamm/ComfyUI-ppm).
# Original implementation: laksjdjf, hako-mikan, Haoming02; ComfyUI-ppm by pamparamm (AGPL-3.0-or-later).
# Adapted by Saya Couple: this file stays under AGPL-3.0-or-later (see THIRD_PARTY_NOTICES.md).
# Copied 2026-09-18: algorithmic behavior unchanged (execute()'s logic carried over as-is),
# distinct ComfyUI identity (class/key "SayaAttentionCouplePPM", display name
# "Saya Attention Couple PPM") so it does not collide with the official node.
# Only interface difference: this is a V1 node (INPUT_TYPES) with explicit cond/mask
# pairs cond_1..cond_3 / mask_1..mask_3 — the official node generates them dynamically (up to 50).

import torch

import comfy.model_management
from comfy.model_base import Anima, BaseModel, CosmosPredict2, SDXL, SDXLRefiner
from comfy.model_patcher import ModelPatcher

from ..ppm_vendor.attention_couple.common import CondLike
from ..ppm_vendor.attention_couple.cosmos_couple import patch_cosmos_couple
from ..ppm_vendor.attention_couple.unet_couple import (
    unet_attn2_couple_wrapper,
    unet_attn2_output_couple_wrapper,
)
from ..ppm_vendor.negpip import has_negpip


class SayaAttentionCouplePPM:
    @classmethod
    def INPUT_TYPES(cls):
        return {
            "required": {
                "model": ("MODEL",),
                "base_cond": ("CONDITIONING", {
                    "tooltip": "Positive conditioning from KSampler/SamplerCustom node.\n"
                               "Can be optionally scaled up/down by using ConditioningSetAreaStrength node."}),
                "base_mask": ("MASK",),
                "cond_1": ("CONDITIONING",),
                "mask_1": ("MASK",),
                "cond_2": ("CONDITIONING",),
                "mask_2": ("MASK",),
            },
            "optional": {
                "cond_3": ("CONDITIONING",),
                "mask_3": ("MASK",),
            },
        }

    RETURN_TYPES = ("MODEL",)
    RETURN_NAMES = ("MODEL",)
    FUNCTION = "couple"
    CATEGORY = "saya/couple"

    def couple(self, model: ModelPatcher, base_cond: CondLike, base_mask,
               cond_1, mask_1, cond_2, mask_2, cond_3=None, mask_3=None):
        pairs = (
            ("cond_1", cond_1, "mask_1", mask_1),
            ("cond_2", cond_2, "mask_2", mask_2),
            ("cond_3", cond_3, "mask_3", mask_3),
        )
        for cond_name, cond, mask_name, mask in pairs:
            if (cond is None) != (mask is None):
                raise ValueError(
                    f"Attention Couple: {cond_name} and {mask_name} must be connected together."
                )
        cond_inputs: list[CondLike] = [c for c in (cond_1, cond_2, cond_3) if c is not None]
        mask_inputs = [k for k in (mask_1, mask_2, mask_3) if k is not None]

        m: ModelPatcher = model.clone()
        dtype = m.model.diffusion_model.dtype
        device = comfy.model_management.get_torch_device()
        _has_negpip = has_negpip(m.model_options)

        mask = [base_mask] + mask_inputs
        mask = torch.stack(mask, dim=0).to(device, dtype=dtype)
        if mask.sum(dim=0).min() <= 0:
            raise ValueError("Masks contain non-filled areas")
        mask = mask / mask.sum(dim=0, keepdim=True)

        model_type = type(m.model)

        # SD1.* and SDXL
        if issubclass(model_type, SDXL) or issubclass(model_type, SDXLRefiner) or model_type == BaseModel:
            m.set_model_attn2_patch(
                unet_attn2_couple_wrapper(
                    base_cond,
                    cond_inputs,
                    _has_negpip,
                    device,
                    dtype,
                )
            )
            m.set_model_attn2_output_patch(unet_attn2_output_couple_wrapper(mask))

        # Anima and Cosmos Predict2
        elif issubclass(model_type, Anima) or issubclass(model_type, CosmosPredict2):
            patch_cosmos_couple(m, cond_inputs, mask)

        else:
            raise ValueError(f"unsupported model type: {model_type}")

        return (m,)

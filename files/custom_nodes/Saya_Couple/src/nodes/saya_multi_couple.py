"""Phase 1 couple node and the shared MultiMaskCouple regional core.

MultiMaskCouple is an external pack. A fresh AttentionCouple instance per
patched model guarantees that the conditionings it captures (raw_positive /
raw_negative, read when the patch runs) are never shared between models.
"""

from __future__ import annotations

from nodes import ConditioningCombine, ConditioningSetMask

from custom_nodes.MultiMaskCouple.attention_couple import AttentionCouple

from .couple_imprint_v2 import DEFAULT_ATTENTION_PARAMS
from .saya_dual_attention import enable_dual_attention


def _masked_cond(cond, mask, strength):
    return ConditioningSetMask().append(cond, mask, "default", float(strength))[0]


def _attention_couple_patch(model, clip, positive, negative):
    # Fresh instance on every call: the prompts captured by the patch
    # stay private to this model, regardless of execution order.
    return AttentionCouple().attention_couple(
        model=model,
        clip=clip,
        positive=positive,
        negative=negative,
        mode="Attention",
    )


def apply_multimask_couple(
    model,
    clip,
    mask_1,
    mask_2,
    pos_1,
    neg_1,
    pos_2,
    neg_2,
    strength_1,
    strength_2,
    base_weight,
    person_weight,
    main=None,
    couple_fn=_attention_couple_patch,
):
    """MultiMaskCouple regional attention on ``model``; returns the patched MODEL.

    Per region: MAIN masked at ``base_weight`` + the person masked at
    ``person_weight * strength``, coupled with MultiMaskCouple's
    ``AttentionCouple`` (``couple_fn`` is the test seam, same
    (model, clip, positive, negative) -> (model, positive, negative) contract).
    Used for Phase 1's MODEL_2 and the full-frame reconstruct passes
    (``couple_reconstruct.py``). The attn2 patch replaces cross-attention
    entirely, so callers keep MAIN as the sampler positive.
    """
    pos_regions = []
    for pos, mask, strength in ((pos_1, mask_1, strength_1), (pos_2, mask_2, strength_2)):
        if main is not None:
            pos_regions += _masked_cond(main, mask, base_weight)
            strength = person_weight * strength
        pos_regions += _masked_cond(pos, mask, strength)
    neg_regions = ConditioningCombine().combine(
        _masked_cond(neg_1, mask_1, strength_1),
        _masked_cond(neg_2, mask_2, strength_2),
    )[0]
    return couple_fn(model, clip, pos_regions, neg_regions)[0]


class SayaMultiCouple:
    """Phase 1 couple: MODEL_1 runs the core Saya dual attention, MODEL_2 the MultiMaskCouple patch.

    Solo: MAIN + pos_1 only, both models returned unpatched.
    """

    @classmethod
    def INPUT_TYPES(cls):
        return {
            "required": {
                "model_1": ("MODEL",),
                "clip": ("CLIP",),
                "mask_1": ("MASK",),
                "mask_2": ("MASK",),
                "pos_1": ("CONDITIONING",),
                "neg_1": ("CONDITIONING",),
                "pos_2": ("CONDITIONING",),
                "neg_2": ("CONDITIONING",),
                "strength_1": ("FLOAT", {"default": 1.0, "min": 0.0, "max": 2.0, "step": 0.05}),
                "strength_2": ("FLOAT", {"default": 1.0, "min": 0.0, "max": 2.0, "step": 0.05}),
            },
            "optional": {
                "model_2": ("MODEL",),
                "main": ("CONDITIONING", {"tooltip": "Required in Couple mode (base of the dual attention)."}),
                "solo": ("BOOLEAN", {
                    "default": False,
                    "tooltip": "OFF = Couple. ON = Solo: MAIN + pos_1 only, pos_2/neg_2/mask_2 "
                               "ignored, models returned unpatched.",
                }),
            },
        }

    RETURN_TYPES = ("MODEL", "MODEL", "CONDITIONING", "CONDITIONING")
    RETURN_NAMES = ("MODEL_1_PATCHED", "MODEL_2_PATCHED", "CONDITIONING", "NEGATIVE")
    FUNCTION = "apply"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Couple: MODEL_1 gets the core Saya dual attention (positive = MAIN, P1/P2 and "
        "masks travel as its payload); MODEL_2 gets the MultiMaskCouple regional patch "
        "(None if model_2 is not connected). Solo: MAIN + P1, models unpatched."
    )

    def apply(
        self,
        model_1,
        clip,
        mask_1,
        mask_2,
        pos_1,
        neg_1,
        pos_2,
        neg_2,
        strength_1=1.0,
        strength_2=1.0,
        model_2=None,
        main=None,
        solo=False,
    ):
        if solo:
            positive = ConditioningCombine().combine(main, pos_1)[0] if main is not None else pos_1
            return (model_1, model_2, positive, neg_1)

        model_1_patched = enable_dual_attention(model_1, main, pos_1, pos_2, mask_1, mask_2)
        model_2_patched = None
        if model_2 is not None:
            model_2_patched = apply_multimask_couple(
                model_2, clip, mask_1, mask_2, pos_1, neg_1, pos_2, neg_2,
                strength_1, strength_2,
                DEFAULT_ATTENTION_PARAMS["base_weight"], DEFAULT_ATTENTION_PARAMS["person_weight"],
                main=main, couple_fn=_attention_couple_patch,
            )
        return (model_1_patched, model_2_patched, main, neg_1)

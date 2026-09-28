"""Regional multi-model couple, via the MultiMaskCouple pack (external library).

Each connected model gets its own coupling patch. A dedicated AttentionCouple
instance per model guarantees that the conditionings it captures
(raw_positive / raw_negative, instance attributes read when the patch runs)
are never shared between the two patches.
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
    model_1,
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
    model_2=None,
    main=None,
    couple_fn=_attention_couple_patch,
):
    """The MultiMaskCouple regional-attention core -- Sampler 1's algorithm,
    reusable by any reconstruction path (``couple_reconstruct.py``).

    Same construction as ``SayaMultiCouple.apply()``'s normal (non-solo,
    non-dual) branch: MAIN masked at ``base_weight`` + each person masked at
    ``person_weight * strength`` per region, coupled with
    ``custom_nodes.MultiMaskCouple.attention_couple.AttentionCouple`` (or the
    injected ``couple_fn`` -- test seam, same (model, clip, positive,
    negative) -> (model, positive, negative) contract). Returns
    (model_1_patched, model_2_patched|None, positive, negative).
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

    # AttentionCouple() reads the per-region prompts into the model's own
    # attn2 patch and hands back an empty-prompt placeholder. Cross-attention
    # is fully replaced by the patch, so the sampler's positive only supplies
    # the SDXL pooled vector: give it MAIN's, not the empty prompt's.
    model_1_patched, coupled_positive, _ = couple_fn(model_1, clip, pos_regions, neg_regions)
    if main is not None:
        coupled_positive = main

    model_2_patched = None
    if model_2 is not None:
        model_2_patched, _, _ = couple_fn(model_2, clip, pos_regions, neg_regions)

    # NEGATIVE = the caller's global negative, unchanged (matches the
    # current graph wiring, where pass 1's negative comes straight from
    # the NEG encode node).
    return model_1_patched, model_2_patched, coupled_positive, neg_1


class SayaMultiCouple:
    """Couple two models on the same regions, each with an independent patch."""

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
                "main": ("CONDITIONING",),
                "solo": ("BOOLEAN", {
                    "default": False,
                    "tooltip": "OFF (default) = Couple, unchanged behavior. ON = SOLO: "
                               "MAIN + pos_1 only, pos_2/neg_2/mask_2 ignored entirely, "
                               "no attention-couple patch, models returned unpatched.",
                }),
                "dual_attention_enabled": ("BOOLEAN", {
                    "default": False,
                    "tooltip": "EXPERIMENTAL, MODEL_1 only. OFF (default) = historic behavior, unchanged. "
                               "ON = MODEL_1 stays the raw model and runs the core Saya forced attn2 path "
                               "(saya_dual_mode); the positive is MAIN; P1/P2/masks only travel as its payload. "
                               "Needs main. MODEL_2 keeps the historic couple.",
                }),
            },
        }

    RETURN_TYPES = ("MODEL", "MODEL", "CONDITIONING", "CONDITIONING")
    RETURN_NAMES = ("MODEL_1_PATCHED", "MODEL_2_PATCHED", "CONDITIONING", "NEGATIVE")
    FUNCTION = "apply"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Regional two-model couple: one independent coupling patch per model, "
        "shared regional masks and strengths. MODEL_2_PATCHED is None if "
        "model_2 is not connected (pass 2 then requires a model)."
    )

    @staticmethod
    def _masked(cond, mask, strength):
        return ConditioningSetMask().append(cond, mask, "default", float(strength))[0]

    @staticmethod
    def _couple(model, clip, positive, negative):
        # Fresh instance on every call: the prompts captured by the patch
        # stay private to this model, regardless of execution order.
        return AttentionCouple().attention_couple(
            model=model,
            clip=clip,
            positive=positive,
            negative=negative,
            mode="Attention",
        )

    def _apply_dual(self, model_1, clip, mask_1, mask_2, pos_1, neg_1, pos_2, neg_2, strength_1, strength_2, model_2, main, solo):
        """Dual ON: no historic region is built for MODEL_1, nothing falls back to the historic couple."""
        if solo:
            raise RuntimeError("dual_attention_enabled cannot be combined with solo")
        model_1_patched = enable_dual_attention(model_1, main, pos_1, pos_2, mask_1, mask_2)

        # MODEL_2 (Sampler 2) keeps the historic couple, with the historic regions.
        model_2_patched = None
        if model_2 is not None:
            base_weight = DEFAULT_ATTENTION_PARAMS["base_weight"]
            person_weight = DEFAULT_ATTENTION_PARAMS["person_weight"]
            pos_regions = []
            for pos, mask, strength in ((pos_1, mask_1, strength_1), (pos_2, mask_2, strength_2)):
                pos_regions += self._masked(main, mask, base_weight)
                pos_regions += self._masked(pos, mask, person_weight * strength)
            neg_regions = ConditioningCombine().combine(
                self._masked(neg_1, mask_1, strength_1),
                self._masked(neg_2, mask_2, strength_2),
            )[0]
            model_2_patched, _, _ = self._couple(model_2, clip, pos_regions, neg_regions)
        return (model_1_patched, model_2_patched, main, neg_1)

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
        dual_attention_enabled=False,
    ):
        if dual_attention_enabled:
            return self._apply_dual(
                model_1, clip, mask_1, mask_2, pos_1, neg_1, pos_2, neg_2, strength_1, strength_2, model_2, main, solo,
            )
        if solo:
            # SOLO: MAIN + pos_1 only. pos_2/neg_2/mask_2 are read nowhere
            # below. No region split, no attention-couple patch — the
            # model(s) are returned exactly as received.
            positive = ConditioningCombine().combine(main, pos_1)[0] if main is not None else pos_1
            return (model_1, model_2, positive, neg_1)

        # MAIN is already encoded by the normal CLIPTextEncode node.  Merge it
        # with each person lane as CONDITIONING; no prompt text crosses this
        # node or the sampler boundary.
        # The two regions cover the whole frame, so MAIN (scene, background) only
        # reaches the image through them. Weight it like the Phase 2+ reconstruct
        # (base vs person) instead of an even split with each person prompt.
        return apply_multimask_couple(
            model_1, clip, mask_1, mask_2, pos_1, neg_1, pos_2, neg_2,
            strength_1, strength_2,
            DEFAULT_ATTENTION_PARAMS["base_weight"], DEFAULT_ATTENTION_PARAMS["person_weight"],
            model_2=model_2, main=main, couple_fn=self._couple,
        )

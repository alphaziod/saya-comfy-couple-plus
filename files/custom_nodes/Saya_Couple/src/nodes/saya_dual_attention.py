"""Wiring of the core Saya forced attn2 path (saya_dual_mode) onto MODEL_1.

This module only transports data: the contexts P1/P2 and the masks travel in
transformer_options["saya_dual"]; the core owns the operator and the fusion.
Every violation raises RuntimeError, there is no path back to the historic couple.
"""

from __future__ import annotations

import torch

import comfy.ldm.modules.attention as attention

FUSION_MODE = "main_locked_delta"

# model_options entries that run around the model call and can rewrite context,
# conds or transformer_options: refused. The other sampler_* entries only act on
# predictions (sampler_pre_cfg_function, sampler_cfg_function,
# sampler_post_cfg_function: APG, CFGZeroStar, Epsilon Scaling, PAG) and are orthogonal.
_CONFLICTING_MODEL_OPTIONS = ("model_function_wrapper", "sampler_calc_cond_batch_function")


def _context(cond, name):
    if cond is None:
        raise RuntimeError(f"dual attention: {name} conditioning is missing")
    if len(cond) != 1 or not torch.is_tensor(cond[0][0]) or cond[0][0].ndim != 3 or cond[0][0].shape[0] != 1:
        raise RuntimeError(f"dual attention: {name} must hold exactly one [1, tokens, channels] conditioning")
    return cond[0][0]


def _check_model(model):
    if model is None:
        raise RuntimeError("dual attention: MODEL_1 is missing")
    classes = [key for key in model.object_patches if key == "__class__" or key.endswith(".__class__")]
    if classes:
        raise RuntimeError(f"dual attention: object patch replaces a class ({classes[0]}, {len(classes)} in total), the core flag would be ignored")
    blocks = [m for m in model.model.diffusion_model.modules() if hasattr(m, "attn2") and hasattr(m, "norm2")]
    if not blocks:
        raise RuntimeError("dual attention: MODEL_1 has no cross-attention transformer block")
    swapped = {type(m).__name__ for m in blocks if type(m) is not attention.BasicTransformerBlock}
    if swapped:
        raise RuntimeError(f"dual attention: transformer block class replaced by {sorted(swapped)}, the core flag would be ignored")

    options = model.model_options
    transformer_options = options.get("transformer_options", {})
    for name in _CONFLICTING_MODEL_OPTIONS:
        if options.get(name) is not None:
            raise RuntimeError(f"dual attention: model_options['{name}'] can rewrite context or transformer_options")
    live = {}
    for source in (model.wrappers, transformer_options.get("wrappers", {})):
        for kind, keys in source.items():
            live.setdefault(kind, []).extend(str(key) for key, wrappers in keys.items() if wrappers)
    live = {kind: keys for kind, keys in live.items() if keys}
    if live:
        raise RuntimeError(f"dual attention: model wrappers present {live}, they can rewrite context or transformer_options")
    if "saya_dual_mode" in transformer_options or "saya_dual" in transformer_options:
        raise RuntimeError("dual attention: MODEL_1 already carries saya_dual_mode")
    if "optimized_attention_override" in transformer_options:
        raise RuntimeError("dual attention: optimized_attention_override is forbidden")
    patches = transformer_options.get("patches", {})
    replaced = transformer_options.get("patches_replace", {}).get("attn2")
    present = [name for name in ("attn2_patch", "attn2_output_patch") if patches.get(name)] + (["attn2 patches_replace"] if replaced else [])
    if present:
        raise RuntimeError(f"dual attention: MODEL_1 already carries an attn2 patch ({', '.join(present)}), the historic couple must not be applied")


def enable_dual_attention(model, main, pos_1, pos_2, mask_1, mask_2):
    """Clone MODEL_1 with saya_dual_mode and the saya_dual payload."""
    contexts = {"main": _context(main, "main"), "p1": _context(pos_1, "pos_1"), "p2": _context(pos_2, "pos_2")}
    if len({id(t) for t in contexts.values()}) != 3 or len({t.data_ptr() for t in contexts.values()}) != 3:
        raise RuntimeError("dual attention: main, pos_1 and pos_2 must be three separate conditionings")
    for name, mask in (("mask_1", mask_1), ("mask_2", mask_2)):
        if not torch.is_tensor(mask) or mask.ndim not in (2, 3):
            raise RuntimeError(f"dual attention: {name} is missing or not a [H, W] / [B, H, W] mask")
    _check_model(model)

    patched = model.clone()
    options = patched.model_options.setdefault("transformer_options", {})
    options["saya_dual_mode"] = True
    options["saya_dual"] = {
        "p1": contexts["p1"],
        "p2": contexts["p2"],
        "mask_1": mask_1,
        "mask_2": mask_2,
        "fusion_mode": FUSION_MODE,
        "params": {},
    }
    return patched

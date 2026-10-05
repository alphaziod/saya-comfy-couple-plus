"""Wiring of the Saya Couple engine (src/engine/dual_attention.py) onto MODEL_1.

M1 (2026-10-05, audit Fable): no ComfyUI core patch any more. The engine is injected with the official
ModelPatcher object patches: for every cross-attention transformer block of MODEL_1, the clone carries a
shallow copy of the block (same stock class behaviour, same sub-modules, same weights, same state_dict keys)
whose ``attn2`` is ``SayaAttn2``: when the payload ``transformer_options["saya_dual"]`` is present it runs
``engine.saya_dual_attn2(block, n, context, transformer_options)``, otherwise the stock cross-attention.
ComfyUI installs the copies in ``patch_model()`` and restores the originals in ``unpatch_model()``; when it
drops a clone without unpatching (another clone of the same model takes over, ``unpatch_all=False``), the
ON_DETACH callback puts the originals back itself, so every clone restores exactly what it found.

This module only transports data: the contexts P1/P2 and the masks travel in the payload; the engine owns
the operator and the fusion. Every violation raises RuntimeError, there is no path back to the historic couple.
"""

from __future__ import annotations

import torch
from torch import nn

import comfy.ldm.modules.attention as attention
import comfy.utils
from comfy.patcher_extension import CallbacksMP

from ..engine import dual_attention as engine

FUSION_MODE = "main_locked_delta"
# Selectable ownership of each pixel (SayaMultiCouple "ownership" input).
OWNERSHIP_FUSION_MODES = {"static_split": "main_locked_delta", "dynamic": "main_locked_delta_dynamic"}

# model_options entries that run around the model call and can rewrite context,
# conds or transformer_options: refused. The other sampler_* entries only act on
# predictions (sampler_pre_cfg_function, sampler_cfg_function,
# sampler_post_cfg_function: APG, CFGZeroStar, Epsilon Scaling, PAG) and are orthogonal.
_CONFLICTING_MODEL_OPTIONS = ("model_function_wrapper", "sampler_calc_cond_batch_function")

def engine_call(transformer_options):
    """The engine runs when the payload is present (None = no payload)."""
    return transformer_options.get("saya_dual") is not None


class SayaAttn2(nn.Module):
    """Stands in for ``block.attn2``: same projections (so ``attn2.to_q.weight``... keep their state_dict keys and
    their LoRA patches), the engine when the payload is there, the stock cross-attention otherwise."""

    def __init__(self, block):
        super().__init__()
        attn = block.attn2
        self.to_q, self.to_k, self.to_v, self.to_out = attn.to_q, attn.to_k, attn.to_v, attn.to_out
        self.heads, self.dim_head, self.attn_precision = attn.heads, attn.dim_head, attn.attn_precision
        self.saya_block, self.saya_attn = [block], [attn]  # plain lists: not sub-modules, no duplicate keys

    def forward(self, x, context=None, value=None, mask=None, transformer_options={}):
        if engine_call(transformer_options):
            return engine.saya_dual_attn2(self.saya_block[0], x, context, transformer_options)
        return self.saya_attn[0](x, context=context, value=value, mask=mask, transformer_options=transformer_options)


_BLOCK_CLASSES = {}


def _block_class(cls):
    """Subclass of the stock block class: the stock forward, preceded by the engine's hook check (an attn2 hook
    added after the clone would otherwise bypass the proxy silently)."""
    sub = _BLOCK_CLASSES.get(cls)
    if sub is None:
        def forward(self, x, context=None, transformer_options={}):
            if engine_call(transformer_options):
                engine.saya_dual_check_hooks(self, transformer_options)
            return cls.forward(self, x, context, transformer_options)
        sub = type("Saya" + cls.__name__, (cls,), {"forward": forward, "__module__": __name__, "saya_original_class": cls})
        _BLOCK_CLASSES[cls] = sub
    return sub


def saya_block(block):
    """Shallow copy of a stock block with ``attn2`` = SayaAttn2. Sub-modules, parameters, buffers and hooks are the
    same objects (fresh registries, so the original block is never touched); a copy of a copy is a copy of the
    original (``saya_source``), object patches never nest."""
    block = getattr(block, "saya_source", [block])[0]
    wrapper = object.__new__(_block_class(type(block)))
    for name, value in block.__dict__.items():
        wrapper.__dict__[name] = value.copy() if isinstance(value, (dict, set)) else value
    wrapper._modules["attn2"] = SayaAttn2(block)
    wrapper.__dict__["saya_source"] = [block]  # a list: not registered as a sub-module
    return wrapper


def _is_stock_block(module):
    return type(module) is attention.BasicTransformerBlock or getattr(type(module), "saya_original_class", None) is attention.BasicTransformerBlock


def saya_block_names(model):
    """``diffusion_model.<path>`` and module of every cross-attention transformer block of MODEL_1."""
    return [(f"diffusion_model.{name}", module) for name, module in model.model.diffusion_model.named_modules()
            if hasattr(module, "attn2") and hasattr(module, "norm2")]


def restore_saya_blocks(patcher, unpatch_all):
    """ON_DETACH: with ``unpatch_all`` ComfyUI restores the object patches itself; without it (a clone dropped because
    another clone of the same model takes over) they would stay on the shared model and be captured as the next
    clone's originals. Put ours back, once."""
    if unpatch_all:
        return
    for name, original in list(patcher.object_patches_backup.items()):
        current = comfy.utils.get_attr(patcher.model, name)
        if getattr(current, "saya_source", None) is not None and current is patcher.object_patches.get(name):
            comfy.utils.set_attr(patcher.model, name, original)
            del patcher.object_patches_backup[name]


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
        raise RuntimeError(f"dual attention: object patch replaces a class ({classes[0]}, {len(classes)} in total), the engine would be bypassed")
    blocks = saya_block_names(model)
    if not blocks:
        raise RuntimeError("dual attention: MODEL_1 has no cross-attention transformer block")
    swapped = {type(m).__name__ for _, m in blocks if not _is_stock_block(m)}
    if swapped:
        raise RuntimeError(f"dual attention: transformer block class replaced by {sorted(swapped)}, the engine would be bypassed")
    foreign = [name for name, _ in blocks if name in model.object_patches and getattr(model.object_patches[name], "saya_source", None) is None]
    if foreign:
        raise RuntimeError(f"dual attention: another object patch replaces a transformer block ({foreign[0]}, {len(foreign)} in total)")

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
    if "saya_dual" in transformer_options or any(name in model.object_patches for name, _ in blocks):
        raise RuntimeError("dual attention: MODEL_1 already carries the Saya engine (saya_dual)")
    if "optimized_attention_override" in transformer_options:
        raise RuntimeError("dual attention: optimized_attention_override is forbidden")
    patches = transformer_options.get("patches", {})
    replaced = transformer_options.get("patches_replace", {}).get("attn2")
    present = [name for name in ("attn2_patch", "attn2_output_patch") if patches.get(name)] + (["attn2 patches_replace"] if replaced else [])
    if present:
        raise RuntimeError(f"dual attention: MODEL_1 already carries an attn2 patch ({', '.join(present)}), the historic couple must not be applied")
    return blocks


def enable_dual_attention(model, main, pos_1, pos_2, mask_1, mask_2, ownership="static_split", params=None):
    """Clone MODEL_1 with the Saya engine on every cross-attention block and the saya_dual payload."""
    if ownership not in OWNERSHIP_FUSION_MODES:
        raise RuntimeError(f"dual attention: unknown ownership {ownership!r}, known: {sorted(OWNERSHIP_FUSION_MODES)}")
    contexts = {"main": _context(main, "main"), "p1": _context(pos_1, "pos_1"), "p2": _context(pos_2, "pos_2")}
    if len({id(t) for t in contexts.values()}) != 3 or len({t.data_ptr() for t in contexts.values()}) != 3:
        raise RuntimeError("dual attention: main, pos_1 and pos_2 must be three separate conditionings")
    for name, mask in (("mask_1", mask_1), ("mask_2", mask_2)):
        if not torch.is_tensor(mask) or mask.ndim not in (2, 3):
            raise RuntimeError(f"dual attention: {name} is missing or not a [H, W] / [B, H, W] mask")
    blocks = _check_model(model)

    patched = model.clone()
    options = patched.model_options.setdefault("transformer_options", {})
    options["saya_dual"] = {
        "p1": contexts["p1"],
        "p2": contexts["p2"],
        "mask_1": mask_1,
        "mask_2": mask_2,
        "fusion_mode": OWNERSHIP_FUSION_MODES[ownership],
        "params": dict(params or {}),
    }
    for name, block in blocks:
        patched.add_object_patch(name, saya_block(block))
    patched.add_callback(CallbacksMP.ON_DETACH, restore_saya_blocks)
    return patched

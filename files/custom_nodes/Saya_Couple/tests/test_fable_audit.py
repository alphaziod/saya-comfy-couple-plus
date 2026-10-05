"""Audit Fable 2026-10-05: silent fallbacks of the dynamic ownership, and the Sampler 1 map capture key.

1. SayaMultiCouple, ownership = dynamic but no anchor can be derived: the engine keeps the static split
   (by design) -- the node must say so (warning), not stay silent.
2. SayaMultiCouple, ownership = dynamic on a frame whose /32 grid exceeds the engine's max_tokens: the
   dynamic ownership never starts -- warning too.
3. SayaOwnershipMapCapture with a COUPLE_RECIPE reads the engine state of THAT recipe's MODEL_1 (the
   engine keys its state by the P1 conditioning tensor), never another model's state.
"""

from __future__ import annotations

import logging

import torch

from harness import Check, load_pack
from test_hidream_reconstruct import _imprint


class _Clip:
    def __init__(self):
        self.encoded: list[str] = []

    def tokenize(self, text, return_word_ids=False):
        tokens = [(index + 1, 1.0, index + 1) for index, _ in enumerate(text.split())]
        return {"l": [tokens]} if return_word_ids else [text]

    def encode_from_tokens_scheduled(self, tokens):
        self.encoded.append(tokens[0])
        return [[torch.zeros(1, 3, 8), {}]]


def _cond(fill):
    return [[torch.full((1, 5, 8), float(fill)), {}]]


class _Warnings(logging.Handler):
    def __init__(self):
        super().__init__(level=logging.WARNING)
        self.messages: list[str] = []

    def emit(self, record):
        self.messages.append(record.getMessage())


def _apply_dynamic(module, mask_hw, **kwargs):
    """SayaMultiCouple.apply in Couple / dynamic with both patch functions stubbed; returns (params, warnings)."""
    load_pack()
    params: list = []
    saved = module.enable_dual_attention, module.apply_multimask_couple
    module.enable_dual_attention = lambda model, main, pos_1, pos_2, m1, m2, ownership, p: params.append(p) or "m1"
    module.apply_multimask_couple = lambda *args, **kw: "m2"
    handler = _Warnings()
    logging.getLogger().addHandler(handler)
    clip = _Clip()
    try:
        mask = torch.ones(*mask_hw)
        module.SayaMultiCouple().apply("model", clip, mask, 1 - mask, _cond(1), _cond(2), _cond(3), _cond(4),
                                       model_2="raw", main=_cond(0), ownership="dynamic", **kwargs)
    finally:
        module.enable_dual_attention, module.apply_multimask_couple = saved
        logging.getLogger().removeHandler(handler)
    return params[0], handler.messages, clip.encoded


def test_dynamic_silent_fallbacks_are_logged():
    load_pack()
    import saya_couple.src.nodes.saya_multi_couple as module

    c = Check("dynamic_silent_fallbacks_are_logged")
    c.eq(module._attention_grid_32(torch.zeros(1216, 832)), (38, 26), "832x1216 -> /32 grid 38x26 (988 tokens)")
    c.eq(module._attention_grid_32(torch.zeros(1, 864, 1536)), (27, 48), "1536x864 (batched mask) -> 27x48 (1296 tokens)")

    params, warnings, _ = _apply_dynamic(module, (1216, 832))
    c.ok("p1_anchor" not in params, "no text / no oracle anchors: the payload carries no anchor (static split in the engine)")
    c.ok(any("no P1/P2 anchor" in m for m in warnings), f"...and the node warns about it: {warnings}")

    params, warnings, _ = _apply_dynamic(module, (1216, 832), p1_anchors="red hair", p2_anchors="blue hair")
    c.ok(params.get("p1_anchor") is not None and params.get("person_anchor") is not None, "oracle anchors: payload has the anchors")
    c.eq(warnings, [], "832x1216 with anchors: nothing to warn about")

    params, warnings, _ = _apply_dynamic(module, (864, 1536), p1_anchors="red hair", p2_anchors="blue hair")
    c.ok(any("max_tokens" in m and "27x48" in m for m in warnings), f"1536x864: the /32 grid exceeds max_tokens, the node warns: {warnings}")
    return c.report()


def test_capture_reads_the_recipe_state_only():
    load_pack()
    from saya_couple.src.nodes import couple_imprint_v2 as v2
    from saya_couple.src.nodes import ownership_map as om

    c = Check("capture_reads_the_recipe_state_only")
    states: dict = {}
    om._engine_state = lambda: states
    om.apply_multimask_couple = lambda *args, **kwargs: "rebuilt"
    node = om.SayaOwnershipMapCapture()
    payload = v2.canonical_imprint_json(_imprint(v2))
    pos_1 = [[torch.zeros(1, 2, 4), {}]]
    recipe = {"model_2": "raw", "model_2_patched": "static", "clip": None, "pos_1": pos_1, "neg_1": 2, "pos_2": 3, "neg_2": 4,
              "strength_1": 1.0, "strength_2": 1.0, "main": 5, "dynamic": True}

    def state(rows):
        active = torch.tensor(rows).unsqueeze(0)
        return {"active": active, "map": torch.zeros(1, 3, *active.shape[1:]), "block": None}

    other = torch.zeros(1)
    states[id(other)] = state([[0, 1], [1, 0]])
    out, model, report = node.capture({"samples": torch.zeros(1, 4, 16, 16)}, payload, recipe)
    c.ok("ownership_map" not in v2.parse_imprint_json(out)["couple_imprint"], "a single state of ANOTHER model: no map (never another image's map)")
    c.ok("another MODEL_1" in report and model == "static", f"...said in the report, static MODEL_2: {report}")
    c.eq(states, {}, "the foreign state is consumed too (the engine never keeps stale state)")

    states[id(other)] = state([[0, 1], [1, 0]])
    states[id(pos_1[0][0])] = state([[0, 2], [-1, 1]])
    out, model, report = node.capture({"samples": torch.zeros(1, 4, 16, 16)}, payload, recipe)
    c.eq(v2.parse_imprint_json(out)["couple_imprint"].get("ownership_map", {}).get("rows"), ["1b", "s2"],
         "two states: the recipe's own (keyed by its P1 conditioning) is captured")
    c.eq(model, "rebuilt", "MODEL_2 rebuilt on the recipe's map")

    states[id(other)] = state([[0, 1], [1, 0]])
    out, model, report = node.capture({"samples": torch.zeros(1, 4, 16, 16)}, payload, None)
    c.eq(v2.parse_imprint_json(out)["couple_imprint"].get("ownership_map", {}).get("rows"), ["12", "21"],
         "no recipe: the only state there is (historic rule)")
    return c.report()


def test_person_anchor_includes_men():
    load_pack()
    import saya_couple.src.nodes.saya_multi_couple as module

    c = Check("person_anchor_includes_men")
    girl, boy = "1girl, adult, long blonde hair, nude", "1boy, adult man, short black hair, nude"
    c.eq(module.person_anchor_text("auto"), "woman", "auto, no text: historic anchor")
    c.eq(module.person_anchor_text("auto", girl, "1girl, futanari, red hair"), "woman", "auto, all-female: 'woman' alone (bit-identical payload)")
    c.eq(module.person_anchor_text("auto", girl, boy), "woman, man", "auto, a male P1/P2 text: 'woman, man'")
    c.eq(module.person_anchor_text("auto", "2boys, adult, muscular"), "woman, man", "'2boys': digits are not letters, 'boys' is seen")
    c.eq(module.person_anchor_text("auto", "a womanly figure, boyish charm"), "woman", "'womanly' / 'boyish' are not person words")
    c.eq(module.person_anchor_text("woman", girl, boy), "woman", "explicit value: used as is, no auto")
    c.eq(module.person_anchor_text(" person ", girl, boy), "person", "explicit value: stripped")

    _, _, encoded = _apply_dynamic(module, (1216, 832), p1_anchors="black hair", p2_anchors="blonde hair", p1_text=boy, p2_text=girl, person_anchor="auto")
    c.ok("woman, man" in encoded, f"node, auto with a male text: the person anchor encoded is 'woman, man' ({encoded})")
    _, _, encoded = _apply_dynamic(module, (1216, 832), p1_anchors="red hair", p2_anchors="blue hair", p1_text="1girl, red hair", p2_text="1girl, blue hair", person_anchor="auto")
    c.ok("woman" in encoded and "woman, man" not in encoded, f"node, auto, all-female: 'woman' alone ({encoded})")
    _, _, encoded = _apply_dynamic(module, (1216, 832), p1_anchors="black hair", p2_anchors="blonde hair", p1_text=boy, p2_text=girl)
    c.ok("woman" in encoded and "woman, man" not in encoded, "node, DEFAULT (no widget value, old workflows): historic 'woman' even with a male text")
    return c.report()


def test_multimask_couple_missing_is_a_clear_error():
    """Without MultiMaskCouple the pack still loads (Solo works); Couple fails with a clear RuntimeError."""
    import importlib
    import sys

    load_pack()
    import saya_couple.src.nodes.saya_multi_couple as module

    c = Check("multimask_couple_missing_is_a_clear_error")
    name = "custom_nodes.MultiMaskCouple.attention_couple"
    saved = {k: sys.modules.get(k) for k in (name, "custom_nodes.MultiMaskCouple", "custom_nodes")}
    try:
        sys.modules[name] = None  # import of this module now raises ImportError
        importlib.reload(module)
        c.ok(hasattr(module, "SayaMultiCouple"), "module (and so the registry) still imports without MultiMaskCouple")
        try:
            module._attention_couple_patch("model", None, [], [])
            c.ok(False, "Couple without MultiMaskCouple must raise")
        except RuntimeError as error:
            c.ok("MultiMaskCouple is missing" in str(error) and module.MULTIMASK_HINT in str(error), f"clear error: {error}")
    finally:
        for k, v in saved.items():
            if v is None:
                sys.modules.pop(k, None)
            else:
                sys.modules[k] = v
        importlib.reload(module)
    c.ok(module._multimask_couple().__name__ == "AttentionCouple", "MultiMaskCouple back: the real class is used")
    return c.report()


def test_m1_lifecycle():
    """M1 (object-patch injection) on a real ModelPatcher: pose / forward / exception / restore, Solo through a patched
    clone, process chaud (a clone dropped without unpatch), prompt change."""
    import os

    import comfy.ldm.modules.attention as attention
    import comfy.model_patcher
    import comfy.ops
    import comfy.utils
    from torch import nn

    load_pack()
    from saya_couple.src.nodes import saya_dual_attention as dual

    c = Check("m1_lifecycle")
    torch.manual_seed(0)
    block = attention.BasicTransformerBlock(16, 2, 8, context_dim=8, operations=comfy.ops.disable_weight_init)
    for p in block.parameters():
        nn.init.normal_(p, std=0.3)
    model = nn.Module()
    model.diffusion_model = nn.Module()
    model.diffusion_model.transformer_blocks = nn.ModuleList([block])
    mp = comfy.model_patcher.ModelPatcher(model, torch.device("cpu"), torch.device("cpu"))
    mask = torch.ones(8, 8)
    conds = lambda: [[[torch.rand(1, 4, 8), {}]] for _ in range(3)]
    keys = list(model.state_dict().keys())
    x, ctx = torch.randn(2, 16, 16), torch.randn(2, 5, 8)
    base = {"cond_or_uncond": [1, 0], "activations_shape": [2, 16, 4, 4]}
    stock = block(x, ctx, dict(base))

    # 1. Couple clone: pose, forward, a hook added after the clone, exception, restore
    m1 = dual.enable_dual_attention(mp, *conds(), mask, mask)
    c.ok(not mp.object_patches and "saya_dual" not in mp.model_options["transformer_options"], "the raw ModelPatcher is untouched")
    m1.patch_model(load_weights=False)
    live = model.diffusion_model.transformer_blocks[0]
    c.ok(live is not block and live.saya_source[0] is block, "patch_model: the Saya copy stands in for the block")
    c.eq(list(model.state_dict().keys()), keys, "state_dict keys unchanged (LoRA / lowvram by key)")
    opts = {**base, **m1.model_options["transformer_options"]}
    out = live(x, ctx, opts)
    c.ok(bool(torch.isfinite(out).all()) and not torch.equal(out, stock), "Couple forward through the object patch (differs from stock)")
    bad = dict(opts, patches_replace={"attn2": {("input", 1, 0): lambda *a: None}})
    c.raises(RuntimeError, lambda: live(x, ctx, bad), "attn2 hook added after the clone: refused at forward, never bypassed")
    c.ok(torch.equal(live(x, ctx, dict(base)), stock), "no payload through the patched clone (Solo-like call): stock output, bit-identical")
    m1.unpatch_model(unpatch_weights=False)
    c.ok(model.diffusion_model.transformer_blocks[0] is block and not m1.object_patches_backup, "unpatch_model: original block back, backup empty (also after the exception)")
    c.ok(torch.equal(block(x, ctx, dict(opts)), stock), "after restore the raw (stock) block ignores the payload")

    # 2. process chaud: ComfyUI drops a loaded clone WITHOUT unpatching when another clone of the same model takes over
    m1.patch_model(load_weights=False)
    m1.detach(unpatch_all=False)
    c.ok(model.diffusion_model.transformer_blocks[0] is block and not m1.object_patches_backup,
         "detach(unpatch_all=False): the ON_DETACH callback restored the original, nothing stale for the next clone")
    m1.patch_model(load_weights=False)
    c.ok(model.diffusion_model.transformer_blocks[0].saya_source[0] is block and m1.object_patches_backup["diffusion_model.transformer_blocks.0"] is block,
         "re-patch after detach: the backup is the real original again")
    m1.unpatch_model(unpatch_weights=False)

    # 3. a stale Saya copy left on the shared model by a foreign path: accepted, unwrapped, never nested
    comfy.utils.set_attr(model, "diffusion_model.transformer_blocks.0", dual.saya_block(block))
    m2 = dual.enable_dual_attention(mp, *conds(), mask, mask)
    copy = m2.object_patches["diffusion_model.transformer_blocks.0"]
    c.ok(copy.saya_source[0] is block and copy.attn2.saya_block[0] is block, "stale copy on the shared model: the new copy wraps the real block (no nesting)")
    comfy.utils.set_attr(model, "diffusion_model.transformer_blocks.0", block)
    c.ok(m1.model_options["transformer_options"]["saya_dual"]["p1"] is not m2.model_options["transformer_options"]["saya_dual"]["p1"],
         "new prompt = new payload tensors (engine state keyed by the P1 tensor)")
    m3 = m2.clone()
    c.ok(m3.object_patches == m2.object_patches and m3.callbacks.get("on_detach_after"), "clone() carries the object patches and the ON_DETACH callback (Epsilon / CFGZeroStar clones)")

    return c.report()


def test_vae_fallback_is_logged():
    """FO-04: a registered but unreadable VAE used to come back as the fallback silently (a corrupt file looked like
    'none'); the substitution is now logged with the real error. A missing VAE is logged too."""
    import tempfile
    from pathlib import Path

    load_pack()
    from saya_couple.src.services import models

    c = Check("vae_fallback_is_logged")
    handler = _Warnings()
    logging.getLogger().addHandler(handler)
    saved = models.resolve_registered_model_path
    try:
        with tempfile.TemporaryDirectory() as tmp:
            garbage = Path(tmp) / "broken.safetensors"
            garbage.write_bytes(b"not a tensor file")
            models.resolve_registered_model_path = lambda kind, name: str(garbage)
            c.ok(models.load_vae_or_fallback("broken.safetensors", fallback="FALLBACK") == "FALLBACK", "unreadable VAE: fallback returned")
            c.ok(any("unreadable" in m and "broken.safetensors" in m for m in handler.messages), f"...and logged with the real error: {handler.messages}")
        handler.messages.clear()
        models.resolve_registered_model_path = lambda kind, name: None
        c.ok(models.load_vae_or_fallback("ghost.safetensors", fallback="FALLBACK") == "FALLBACK", "missing VAE: fallback returned")
        c.ok(any("not found" in m for m in handler.messages), f"...and logged: {handler.messages}")
        handler.messages.clear()
        c.ok(models.load_vae_or_fallback("none", fallback="FALLBACK") == "FALLBACK" and not handler.messages, "'none' = fallback by choice, nothing logged")
    finally:
        models.resolve_registered_model_path = saved
        logging.getLogger().removeHandler(handler)
    return c.report()


def test_blur_is_shared():
    """P-E D6: SayaSplitMask's blur is region_masks.gaussian_pixel_sigma_blur, bit-identical to the historic copy."""
    import math
    from torch.nn import functional

    load_pack()
    from saya_couple.src.nodes import region_masks, saya_split_mask

    def historic(mask, sigma):  # the removed copy, verbatim
        sigma = float(sigma)
        if sigma <= 0.0:
            return mask
        radius = max(1, int(math.ceil(3.0 * sigma)))
        coordinates = torch.arange(-radius, radius + 1, dtype=torch.float32, device=mask.device)
        kernel_1d = torch.exp(-(coordinates * coordinates) / (2.0 * sigma * sigma))
        kernel_1d /= kernel_1d.sum()
        kernel_2d = torch.outer(kernel_1d, kernel_1d)[None, None, :, :]
        padded = functional.pad(mask.unsqueeze(1), (radius, radius, radius, radius), mode="replicate")
        return functional.conv2d(padded, kernel_2d).squeeze(1).clamp_(0.0, 1.0)

    c = Check("blur_is_shared")
    g = torch.Generator().manual_seed(3)
    for sigma in (0.0, 0.2, 1.0, 2.5, 7.0):
        mask = (torch.rand(2, 40, 56, generator=g) > 0.5).float()
        c.ok(torch.equal(saya_split_mask.SayaSplitMask._gaussian_blur(mask, sigma), historic(mask, sigma)), f"sigma {sigma}: bit-identical to the historic copy")
        c.ok(torch.equal(region_masks.gaussian_pixel_sigma_blur(mask[0], sigma), historic(mask, sigma)[0]), f"sigma {sigma}: (H,W) form identical")
    return c.report()


def test_invariant_i6_map_consistent_across_grids():
    """04-I6: the Sampler 1 map (grid /32) rasterized at /8, /16 and /32 is binary at every scale and nested: every /8 cell
    carries the code of its /32 cell."""
    load_pack()
    from saya_couple.src.nodes import region_masks as rm

    c = Check("invariant_i6_map_consistent_across_grids")
    g = torch.Generator().manual_seed(5)
    rows = ["".join("12bs"[int(v)] for v in torch.randint(0, 4, (26,), generator=g)) for _ in range(38)]
    ownership = {"grid": [38, 26], "rows": rows, "source": "s1_dynamic"}
    codes = {}
    for scale in (8, 16, 32):
        height, width = 1216 // scale, 832 // scale
        codes[scale] = rm._ownership_codes(ownership, height, width)
        c.ok(set(codes[scale].unique().tolist()) <= {ord("1"), ord("2"), ord("b"), ord("s")}, f"/{scale}: only the four codes (binary ownership)")
    c.ok(torch.equal(codes[32], torch.tensor([[ord(ch) for ch in row] for row in rows], dtype=torch.float32)), "/32: the map itself")
    f = 32 // 8
    nested = codes[8][::f, ::f]
    c.ok(torch.equal(nested, codes[32]) and torch.equal(codes[8].reshape(38, f, 26, f).amax(dim=(1, 3)), codes[8].reshape(38, f, 26, f).amin(dim=(1, 3))),
         "/8: every 4x4 block is one /32 cell (no boundary blend)")
    f16 = 32 // 16
    c.ok(torch.equal(codes[16].reshape(38, f16, 26, f16).amax(dim=(1, 3)), codes[32]), "/16: nested in /32")
    return c.report()


def test_invariant_i8_engine_state_lifecycle():
    """04-I8 (documented behaviour of the engine state): a later call at a LOWER sigma on another grid adds no evidence and
    derives its masks from the existing map (nearest resize); a call at a HIGHER sigma starts a new state (static split
    until the map exists); the capture empties the state."""
    load_pack()
    from saya_couple.src.engine import dual_attention as new
    from saya_couple.src.nodes import ownership_map as om
    import test_core_saya_dual as T

    c = Check("invariant_i8_engine_state_lifecycle")
    _, b_new, _ = T._blocks()
    flags = [1, 0]
    x, ctx = T._inputs(flags)
    g = torch.Generator().manual_seed(11)
    anchors = {"p1_anchor": (torch.randn(1, 6, T.CTX, generator=g), [1, 2]), "p2_anchor": (torch.randn(1, 6, T.CTX, generator=g), [1, 3]),
               "person_anchor": (torch.randn(1, 6, T.CTX, generator=g), [1])}
    dynamic = dict(T._payload(mode="main_locked_delta"), fusion_mode="main_locked_delta_dynamic", params={"start_sigma": 5.0, "confirm_steps": 1, **anchors})
    new._SAYA_OWNERSHIP_STATE.clear()
    for sigma in (4.0, 3.0, 2.0):
        b_new(x, ctx, T._opts(flags, dynamic, sigmas=torch.tensor([sigma])))
    state = next(iter(new._SAYA_OWNERSHIP_STATE.values()))
    c.ok(state["grid"] == [4, 4] and state["map"] is not None, "state on the 4x4 grid with a map")
    count_before = state["count"]
    # lower sigma, another grid (2x8 = same 16 tokens, different shape): no evidence added, masks from the resized map
    n = b_new.norm2(x)
    q = b_new.attn2.to_q(n)
    m1 = torch.cat([torch.ones(2, 8, 1), torch.zeros(2, 8, 1)], dim=1)
    masks = new.saya_dynamic_masks(b_new, n, q, dynamic, [1], 1, 2, 8, [m1, 1 - m1], {"sigmas": torch.tensor([1.5])})
    c.ok(state["grid"] == [4, 4] and state["count"] == 0 and state["sigma"] == 1.5, "lower sigma on another grid: step opened, grid kept, no evidence accumulated")
    c.ok(bool(((masks[0] == 0) | (masks[0] == 1)).all()) and torch.allclose(masks[0] + masks[1], torch.ones_like(m1)), "masks still binary and complementary (map resized nearest)")
    # higher sigma = a new sampling: fresh state, static split until a map exists
    ref = b_new(x, ctx, T._opts(flags, T._payload(mode="main_locked_delta")))
    out = b_new(x, ctx, T._opts(flags, dynamic, sigmas=torch.tensor([14.0])))
    state2 = next(iter(new._SAYA_OWNERSHIP_STATE.values()))
    c.ok(state2 is not state and state2["map"] is None and torch.equal(out, ref), "higher sigma: new state, static split (bit-identical to main_locked_delta)")
    # the capture consumes the state
    states = new._SAYA_OWNERSHIP_STATE
    om._engine_state = lambda: states
    om._LAST.clear()
    from saya_couple.src.nodes import couple_imprint_v2 as v2
    from test_hidream_reconstruct import _imprint
    om.SayaOwnershipMapCapture().capture({"samples": torch.zeros(1, 4, 16, 16)}, v2.canonical_imprint_json(_imprint(v2)), None)
    c.eq(states, {}, "SayaOwnershipMapCapture empties the engine state (nothing survives for the next run)")
    new._SAYA_OWNERSHIP_STATE.clear()
    return c.report()


TESTS = (test_dynamic_silent_fallbacks_are_logged, test_capture_reads_the_recipe_state_only, test_person_anchor_includes_men,
         test_multimask_couple_missing_is_a_clear_error, test_m1_lifecycle, test_vae_fallback_is_logged, test_blur_is_shared,
         test_invariant_i6_map_consistent_across_grids, test_invariant_i8_engine_state_lifecycle)

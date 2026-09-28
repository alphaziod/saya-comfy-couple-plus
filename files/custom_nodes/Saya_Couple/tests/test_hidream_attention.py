"""HiDream Phase 3 regional attention, checked against the REAL RES4LYF mask builder.

The Phase 3 graph feeds SayaCoupleHiDreamReconstruct into a RES4LYF regional
node; RES4LYF's ``FullAttentionMaskHiDream`` then builds the joint
(image + text) attention mask used by every double/single stream block of the
patched HiDream model. These tests build that mask from the pack's own
outputs, with realistic HiDream token lengths (T5 125/128, Llama 3 varying per
prompt), and check who may attend to whom.

Token layout (``HDModel.forward``): image tokens first, then T5 of every
region, then Llama (last layer) of every region, then Llama (current block)
of every region.
"""

from __future__ import annotations

import importlib
import sys
from pathlib import Path

from harness import COMFY_ROOT, Check, load_pack

LENGTHS = {"a": (128, 381), "b": (125, 330)}


def _res4lyf():
    load_pack()
    custom_nodes = str(COMFY_ROOT / "custom_nodes")
    if not (Path(custom_nodes) / "RES4LYF").is_dir():
        return None
    if custom_nodes not in sys.path:
        sys.path.insert(0, custom_nodes)
    return importlib.import_module("RES4LYF.conditioning")


def _hidream_model():
    import comfy.supported_models

    class _Ns:
        pass

    model = _Ns()
    model.model = _Ns()
    model.model.model_config = object.__new__(comfy.supported_models.HiDream)
    return model


def _reconstruct(latent_hw=(16, 24), p2="p2"):
    """Real node outputs, with a fake Quad CLIP producing HiDream-shaped conditionings."""
    import torch
    from test_hidream_reconstruct import _imprint, _mods, _run

    _, hr, v2 = _mods()
    order = iter(("a", "b", "negative"))

    def encoder(clip, text):
        role = next(order)
        t5, llama = LENGTHS.get(role, (128, 128))
        return [[torch.zeros(1, t5, 8), {"conditioning_llama3": torch.zeros(1, 32, llama, 8),
                                         "pooled_output": torch.zeros(1, 4)}]]

    imprint = _imprint(v2, p2=p2, split=0.4)
    out, _ = _run(torch, hr, imprint, latent={"samples": torch.zeros(1, 16, *latent_hw)}, encoder=encoder)
    return out


def _mask(cm, out, node, latent_hw=(16, 24)):
    import torch

    cond_a, cond_b, _solo, _neg, mask_a, mask_b, _regional = out
    common = dict(weight=1.0, region_bleed=0.0, region_bleed_start_step=0, weight_scheduler="constant",
                  start_step=0, mask_type="boolean", edge_width=0, invert_mask=False)
    if node == "ClownRegionalConditioning_AB":
        cond = cm.ClownRegionalConditioning_AB().main(conditioning_A=cond_a, conditioning_B=cond_b,
                                                      mask_A=mask_a, mask_B=mask_b, end_step=-1, **common)[0]
    else:
        # RES4LYF's three-region node, fed a third (MAIN-only, empty-mask) region.
        import torch as _torch
        cond_c = [[_torch.zeros(1, 128, 8), {"conditioning_llama3": _torch.zeros(1, 32, 200, 8),
                                              "pooled_output": _torch.zeros(1, 4)}]]
        cond = cm.ClownRegionalConditioning3().main(conditioning_A=cond_a, conditioning_B=cond_b,
                                                    conditioning_unmasked=cond_c, mask_A=mask_a,
                                                    mask_B=mask_b, end_step=100, **common)[0]
    cond = cond[0][1]["callback_regional"](_hidream_model())
    attn = cond[0][1]["AttnMask"]
    attn.set_latent(torch.zeros(1, 16, *latent_hw))
    attn.generate()
    return attn


def _text_regions(attn):
    """Region index of every text token, in HDModel order (T5 | Llama last | Llama current)."""
    regions = []
    for column in range(3):
        for region, lengths in enumerate(attn.context_lens_list):
            regions += [region] * lengths[column]
    return regions


def _cross_region_text_pairs(attn):
    import torch

    labels = torch.tensor(_text_regions(attn))
    text = attn.attn_mask.mask[attn.img_len:, attn.img_len:].bool()
    same = labels[:, None] == labels[None, :]
    return int((text & ~same).sum()), int((~text & same).sum())


def test_three_region_text_mask_leaks():
    """Why Phase 3 uses two regions: with 3, RES4LYF's parity checkerboard mixes regions.

    If this starts failing, RES4LYF fixed its text mask and the constraint can be revisited.
    """
    cm = _res4lyf()
    c = Check("hidream_three_region_text_leak")
    if cm is None:
        c.skip("RES4LYF not installed")
        return c.report()
    attn = _mask(cm, _reconstruct(), "ClownRegionalConditioning3")
    leaked, _ = _cross_region_text_pairs(attn)
    c.ok(leaked > 0, "3 regions: text tokens of one person attend the other person's text")
    return c.report()


def test_couple_two_regions_isolated():
    cm = _res4lyf()
    c = Check("hidream_couple_two_regions_isolated")
    if cm is None:
        c.skip("RES4LYF not installed")
        return c.report()
    import torch

    hw = (16, 24)
    out = _reconstruct(hw)
    c.eq(out[6], True, "regional_enabled")
    attn = _mask(cm, out, "ClownRegionalConditioning_AB", hw)
    c.eq(attn.num_regions, 2, "exactly P1 and P2 regions")
    c.eq(attn.context_lens_list, [[128, 381, 381], [125, 330, 330]], "each region keeps its own exact token lengths")
    leaked, missing = _cross_region_text_pairs(attn)
    c.eq((leaked, missing), (0, 0), "text <-> text: same region only, complete within a region")

    mask = attn.attn_mask.mask.bool()
    img_len = attn.img_len
    c.eq(img_len, (hw[0] // 2) * (hw[1] // 2), "image tokens = 2x2 patch grid of the sampled latent")
    labels = torch.tensor(_text_regions(attn))
    # one pixel per 16 px patch (latent x 8, patch 2): region 0 = P1 (mask_a), 1 = P2
    image_region = out[4][0, ::16, ::16].flatten().round().long() ^ 1
    img_to_text = mask[:img_len, img_len:]
    expected = image_region[:, None] == labels[None, :]
    c.ok(bool((img_to_text == expected).all()), "image token -> text of its own region only (P1 or P2)")
    c.ok(bool((img_to_text.sum(dim=1) > 0).all()), "every image token sees some text (no orphan token)")
    return c.report()


def test_p2_absent_complement_region():
    """Couple with an empty P2 prompt: region B is MAIN on P1's complement, never an empty mask."""
    cm = _res4lyf()
    c = Check("hidream_p2_absent_complement")
    if cm is None:
        c.skip("RES4LYF not installed")
        return c.report()
    out = _reconstruct(p2=None)
    mask_a, mask_b = out[4], out[5]
    c.ok(bool((mask_a + mask_b == 1).all()), "A + B cover the whole frame")
    attn = _mask(cm, out, "ClownRegionalConditioning_AB")
    img_to_text = attn.attn_mask.mask.bool()[:attn.img_len, attn.img_len:]
    c.ok(bool((img_to_text.sum(dim=1) > 0).all()), "every image token sees some text")
    return c.report()


TESTS = [
    test_three_region_text_mask_leaks,
    test_couple_two_regions_isolated,
    test_p2_absent_complement_region,
]

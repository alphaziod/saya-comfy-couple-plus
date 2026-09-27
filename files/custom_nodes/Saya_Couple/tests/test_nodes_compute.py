"""Compute nodes: color math, masks, resolution calculators — CPU tensors only."""

import torch

from harness import Check, load_pack

import comfy.model_base


class _FakeDiffusionModel:
    dtype = torch.float32


class _FakeSDXL(comfy.model_base.SDXL):
    pass


class _FakeUnsupportedModel:
    pass


class _FakeModelPatcher:
    """Minimal ModelPatcher double: only what SayaAttentionCouplePPM reads."""

    def __init__(self, model_cls):
        self.model = model_cls.__new__(model_cls)
        self.model.diffusion_model = _FakeDiffusionModel()
        self.model_options = {}
        self.attn2_patch = None
        self.attn2_output_patch = None

    def clone(self):
        return self

    def set_model_attn2_patch(self, patch):
        self.attn2_patch = patch

    def set_model_attn2_output_patch(self, patch):
        self.attn2_output_patch = patch


def _fake_cond():
    return [[torch.rand(1, 4, 8), {}]]


def _full_mask():
    return torch.ones(8, 8)


def test_oklab_roundtrip():
    load_pack()
    from saya_couple.src.nodes.chroma_anchor import oklab_to_rgb, rgb_to_oklab

    c = Check("oklab_roundtrip")
    rgb = torch.rand(4, 16, 16, 3)
    back = oklab_to_rgb(rgb_to_oklab(rgb))
    c.ok(torch.allclose(rgb, back, atol=1e-4), f"roundtrip max err {float((rgb - back).abs().max()):.2e}")
    return c.report()


def test_chroma_anchor_behavior():
    load_pack()
    from saya_couple.src.nodes.chroma_anchor import SayaChromaAnchor

    c = Check("chroma_anchor_behavior")
    node = SayaChromaAnchor()
    refined = torch.rand(1, 24, 24, 3)
    reference = torch.rand(1, 24, 24, 3)

    bypassed = node.run(refined, reference, 1.15, 0.01, 0.0)[0]
    c.ok(bypassed is refined, "strength 0 returns input untouched")

    out = node.run(refined, reference, 1.0, 0.0, 1.0)[0]
    c.eq(tuple(out.shape), tuple(refined.shape), "shape preserved")
    c.ok(float(out.min()) >= 0.0 and float(out.max()) <= 1.0, "output clamped to [0,1]")

    # A neutral (gray) reference must cap a saturated refine hard.
    gray_ref = torch.full((1, 24, 24, 3), 0.5)
    capped = node.run(refined, gray_ref, 1.0, 0.0, 1.0)[0]
    c.ok(float(capped.std()) < float(refined.std()), "gray reference desaturates the refine")

    # reference batch 2 vs image batch 1 must not crash (repeat_to_batch_size path).
    ref2 = torch.rand(2, 24, 24, 3)
    out2 = node.run(refined, ref2, 1.15, 0.01, 1.0)[0]
    c.eq(tuple(out2.shape), (1, 24, 24, 3), "reference batch 2 handled")
    return c.report()


def test_naturalize_determinism():
    load_pack()
    from saya_couple.src.nodes.naturalize_postprocess import SayaNaturalizePostProcess

    c = Check("naturalize_determinism")
    node = SayaNaturalizePostProcess()
    image = torch.rand(2, 16, 16, 3)

    same1 = node.run(image, 0.01, 0.5, 123)[0]
    same2 = node.run(image, 0.01, 0.5, 123)[0]
    c.ok(torch.equal(same1, same2), "same seed -> identical output")

    other = node.run(image, 0.01, 0.5, 124)[0]
    c.ok(not torch.equal(same1, other), "different seed -> different output")

    untouched = node.run(image, 0.0, 0.0, 123)[0]
    c.ok(untouched is image, "zero strengths return input untouched")
    return c.report()


def test_ppm_masks_geometry():
    load_pack()
    from saya_couple.src.nodes.ppm_masks import SayaPPMMasks

    c = Check("ppm_masks_geometry")
    node = SayaPPMMasks()
    latent = {"samples": torch.zeros(1, 4, 8, 12)}  # 96px wide, 64px tall at 8x

    base, p1, p2 = node.make_masks(latent, Orientation="horizontal", Split=0.5, Overlap=0.0)
    c.eq(tuple(base.shape), (1, 64, 96), "mask raster shape")
    c.ok(float(base.min()) > 0, "base covers the frame")
    c.ok(float(p1[0, :, :48].sum()) > 0 and float(p1[0, :, 48:].sum()) == 0, "P1 left half only")
    c.ok(float(p2[0, :, 48:].sum()) > 0 and float(p2[0, :, :48].sum()) == 0, "P2 right half only")

    s_base, s_p1, s_p2 = node.make_masks(latent, Orientation="horizontal", Split=0.5, **{"Swap P1 / P2": True})
    c.ok(torch.equal(s_p1, p2) and torch.equal(s_p2, p1), "swap exchanges P1/P2 spatially")

    v_base, v_p1, v_p2 = node.make_masks(latent, Orientation="vertical", Split=0.5)
    c.ok(float(v_p1[0, :32, :].sum()) > 0 and float(v_p1[0, 32:, :].sum()) == 0, "vertical P1 top")

    ov_base, ov_p1, ov_p2 = node.make_masks(latent, Orientation="horizontal", Split=0.5, Overlap=0.25)
    c.ok(float(ov_p1[0, :, 48:].sum()) > 0, "overlap extends P1 past the split")

    total = base + ov_p1 + ov_p2
    c.ok(float(total.min()) > 0, "no zero-coverage pixel (PPM invariant)")
    return c.report()


def test_shadow_control_curve():
    load_pack()
    from saya_couple.src.nodes.hidream_shadow_control import SayaHiDreamShadowControlMask

    c = Check("shadow_control_curve")
    node = SayaHiDreamShadowControlMask()
    blacks = torch.zeros(1, 8, 8, 3)
    whites = torch.ones(1, 8, 8, 3)

    refine_b, _ = node.run(blacks, 0.05, 0.25, 0.05, 1.0, 0.15, False)
    c.ok(abs(float(refine_b.mean()) - 0.15) < 1e-4, "deep black refined at floor strength")

    refine_w, _ = node.run(whites, 0.05, 0.25, 0.05, 1.0, 0.15, False)
    c.ok(abs(float(refine_w.mean()) - 1.0) < 1e-4, "bright areas fully refined")

    off, _ = node.run(blacks, 0.05, 0.25, 0.05, 0.0, 0.15, False)
    c.ok(abs(float(off.mean()) - 1.0) < 1e-4, "protection 0 disables the mask")

    refine, protect = node.run(blacks, 0.05, 0.25, 0.05, 0.5, 0.15, False)
    c.ok(torch.allclose(refine + protect, torch.ones_like(refine)), "refine + protect == 1")
    return c.report()


def test_resolution_calculators():
    load_pack()
    from saya_couple.src.nodes.saya_resolution_scale import (
        SayaResolutionScaleCalculator,
        SayaUpscaleTargetCalculator,
    )

    c = Check("resolution_calculators")
    calc = SayaResolutionScaleCalculator()
    kwargs = dict(
        resolution_preset="Landscape 3:2 · 1536x1024", no_scale=False,
        scale_from_image=False, aspect_preset_when_not_image="3:2 - Landscape",
        swap_aspect_when_not_image=False, custom_aspect_width=16,
        custom_aspect_height=9, mode="FLUX/SDXL (Div8)", custom_divisor=8,
    )
    w, h, wf, hf = calc.calculate(**kwargs)
    c.eq((w, h), (1536, 1024), "fixed preset verbatim")
    c.eq((wf, hf), (1536.0, 1024.0), "float mirrors int")

    swapped = calc.calculate(**{**kwargs, "swap_aspect_when_not_image": True})
    c.eq((swapped[0], swapped[1]), (1024, 1536), "swap flips dims")

    img = torch.zeros(1, 600, 800, 3)
    passthrough = calc.calculate(**{**kwargs, "no_scale": True}, image=img)
    c.eq((passthrough[0], passthrough[1]), (800, 600), "no_scale uses source dims (W,H order)")

    target = SayaUpscaleTargetCalculator()
    _, tw, th, factor, mpix = target.calculate(img, 1920 * 1080)
    c.eq(tw % 16, 0, "target width multiple of 16")
    c.eq(th % 16, 0, "target height multiple of 16")
    c.ok(abs((tw / th) - (800 / 600)) / (800 / 600) < 0.02, "aspect preserved within 2%")
    c.ok(abs(tw * th - 1920 * 1080) / (1920 * 1080) < 0.05, "pixel budget within 5%")
    c.ok(1.0 <= factor <= 4.0, "model factor clamped")

    # Extreme panorama ratio must still terminate and stay sane.
    pano = torch.zeros(1, 200, 2000, 3)
    _, pw, ph, _, _ = target.calculate(pano, 1920 * 1080)
    c.ok(pw > 0 and ph > 0 and pw % 16 == 0 and ph % 16 == 0, "panorama dims sane")
    c.ok(abs((pw / ph) - 10.0) / 10.0 < 0.05, "panorama aspect preserved")
    return c.report()


def test_attention_couple_regressions():
    """N-1/N-2/N-3: mismatched cond/mask pairs and unsupported model types must
    fail loudly at couple() time, not corrupt the render or no-op silently."""
    load_pack()
    from saya_couple.src.nodes.saya_attention_couple import SayaAttentionCouplePPM

    c = Check("attention_couple_regressions")
    node = SayaAttentionCouplePPM()
    base_cond = _fake_cond()

    def couple(**overrides):
        kwargs = dict(
            model=_FakeModelPatcher(_FakeSDXL),
            base_cond=base_cond, base_mask=_full_mask(),
            cond_1=_fake_cond(), mask_1=_full_mask(),
            cond_2=_fake_cond(), mask_2=_full_mask(),
        )
        kwargs.update(overrides)
        return node.couple(**kwargs)

    # Matched pairs on a supported model still patch the clone (behavior kept).
    patcher = couple()[0]
    c.ok(patcher.attn2_patch is not None, "supported model gets an attn2 patch")
    c.ok(patcher.attn2_output_patch is not None, "supported model gets an output patch")

    # N-1: cond_3 without mask_3 (and the symmetric case) must raise, not
    # silently dilute or crash deep inside the sampler.
    c.raises(ValueError, lambda: couple(cond_3=_fake_cond()),
             "cond_3 without mask_3 refused")
    c.raises(ValueError, lambda: couple(mask_3=_full_mask()),
             "mask_3 without cond_3 refused")
    c.raises(ValueError, lambda: couple(cond_1=None),
             "mask_1 without cond_1 refused")

    # N-3: a model type with no couple implementation must raise, not pass
    # the model through unpatched.
    c.raises(ValueError,
             lambda: couple(model=_FakeModelPatcher(_FakeUnsupportedModel)),
             "unsupported model type refused")
    return c.report()


TESTS = (
    test_oklab_roundtrip,
    test_chroma_anchor_behavior,
    test_naturalize_determinism,
    test_ppm_masks_geometry,
    test_shadow_control_curve,
    test_resolution_calculators,
    test_attention_couple_regressions,
)

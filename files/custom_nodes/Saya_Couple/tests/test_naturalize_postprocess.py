"""SayaNaturalizePostProcess: color_lock / highlight_tame, with the historic grain/dither path unchanged."""

from __future__ import annotations

import torch
import torch.nn.functional as F

from harness import Check, load_pack


def _node():
    load_pack()
    from saya_couple.src.nodes.naturalize_postprocess import SayaNaturalizePostProcess
    return SayaNaturalizePostProcess()


def _historic(image, grain_strength, dither_amount, seed):
    """Verbatim copy of the pre-change run() body — the backward-compatibility oracle."""
    if grain_strength == 0 and dither_amount == 0:
        return image
    result = image.float()
    batch, height, width, _ = image.shape
    random_device = "cpu" if image.device.type == "mps" else image.device
    generator = torch.Generator(device=random_device).manual_seed(seed)
    noise_shape = (batch, 1, height, width)
    if grain_strength > 0:
        noise = torch.randn(noise_shape, device=random_device, generator=generator).to(image.device)
        grain = noise - F.avg_pool2d(noise, 3, stride=1, padding=1, count_include_pad=False)
        grain = grain.movedim(1, -1)
        luminance = (0.2126 * result[..., 0:1] + 0.7152 * result[..., 1:2] + 0.0722 * result[..., 2:3]).clamp(0, 1)
        protection = 4 * luminance * (1 - luminance)
        result = result + grain * protection * grain_strength
    if dither_amount > 0:
        noise = torch.rand(noise_shape, device=random_device, generator=generator).to(image.device)
        result = result + (noise.movedim(1, -1) - 0.5) * (dither_amount / 255)
    return result.clamp(0, 1).to(image.dtype)


def _image(seed=0, h=48, w=40):
    g = torch.Generator().manual_seed(seed)
    return torch.rand((1, h, w, 3), generator=g) * 0.8 + 0.1


def _ab(image):
    from saya_couple.src.nodes.chroma_anchor import rgb_to_oklab
    return rgb_to_oklab(image)[..., 1:]


def test_zero_strengths_are_identity():
    node, c = _node(), Check("naturalize_zero_strengths_identity")
    img, ref = _image(1), _image(2)
    c.ok(node.run(img, 0.0, 0.0, 0)[0] is img, "all zero: the SAME tensor is returned")
    c.ok(node.run(img, 0.0, 0.0, 0, reference=ref, color_lock=0.0, highlight_tame=0.0)[0] is img,
         "color_lock=0 and highlight_tame=0 with a reference connected: still the same tensor")
    return c.report()


def test_zero_new_strengths_match_historic_grain_dither():
    node, c = _node(), Check("naturalize_historic_grain_dither_exact")
    img = _image(3)
    for grain, dither, seed in ((0.003, 0.5, 7), (0.008, 0.0, 1), (0.0, 1.0, 99)):
        new = node.run(img, grain, dither, seed, reference=_image(4), color_lock=0.0, highlight_tame=0.0)[0]
        c.ok(torch.equal(new, _historic(img, grain, dither, seed)), f"grain={grain} dither={dither}: bit-exact vs historic")
    return c.report()


def test_color_lock_full_takes_reference_chroma():
    node, c = _node(), Check("naturalize_color_lock_full")
    img = _image(5) * 0.5 + 0.25
    ref = 0.85 * img + 0.15 * img[..., [2, 0, 1]]  # same content, modest hue/chroma shift: stays in gamut
    out = node.run(img, 0.0, 0.0, 0, reference=ref, color_lock=1.0)[0]
    from saya_couple.src.nodes.chroma_anchor import rgb_to_oklab
    c.ok(float((_ab(out) - _ab(ref)).abs().max()) < 2e-3, "color_lock=1: output a/b == reference a/b")
    c.ok(float((rgb_to_oklab(out)[..., 0] - rgb_to_oklab(img)[..., 0]).abs().max()) < 2e-3, "color_lock=1: lightness is the refine's")
    return c.report()


def test_color_lock_interpolation_is_monotone():
    node, c = _node(), Check("naturalize_color_lock_monotone")
    img, ref = _image(7) * 0.6 + 0.2, _image(8) * 0.6 + 0.2
    dist = [float((_ab(node.run(img, 0.0, 0.0, 0, reference=ref, color_lock=a)[0]) - _ab(ref)).norm(dim=-1).mean())
            for a in (0.0, 0.25, 0.5, 0.75, 1.0)]
    c.ok(all(x > y for x, y in zip(dist, dist[1:])), f"chroma distance to reference strictly decreases: {[round(d, 4) for d in dist]}")
    return c.report()


def test_color_lock_needs_reference():
    node, c = _node(), Check("naturalize_color_lock_needs_reference")
    c.raises(ValueError, lambda: node.run(_image(9), 0.0, 0.0, 0, color_lock=0.5), "color_lock without reference: clear error")
    return c.report()


def test_shapes_finite_and_deterministic():
    node, c = _node(), Check("naturalize_shapes_finite_deterministic")
    img, ref = _image(10, 37, 53), _image(11, 20, 30)  # reference at another size is resized
    a = node.run(img, 0.008, 0.5, 5, reference=ref, color_lock=0.8, highlight_tame=0.3)[0]
    b = node.run(img, 0.008, 0.5, 5, reference=ref, color_lock=0.8, highlight_tame=0.3)[0]
    c.eq(tuple(a.shape), tuple(img.shape), "dimensions unchanged")
    c.ok(bool(torch.isfinite(a).all()), "no NaN/Inf")
    c.ok(torch.equal(a, b), "bit-exact determinism")
    return c.report()


def test_extreme_images():
    node, c = _node(), Check("naturalize_extreme_images")
    black, white = torch.zeros((1, 16, 16, 3)), torch.ones((1, 16, 16, 3))
    red = torch.zeros((1, 16, 16, 3)); red[..., 0] = 1.0
    for name, img in (("black", black), ("white", white), ("saturated red", red)):
        for ref in (black, white, red):
            out = node.run(img, 0.0, 0.0, 0, reference=ref, color_lock=1.0, highlight_tame=1.0)[0]
            c.ok(bool(torch.isfinite(out).all()) and float(out.min()) >= 0 and float(out.max()) <= 1, f"{name}: finite, in [0,1]")
    out = node.run(white, 0.0, 0.0, 0, highlight_tame=1.0)[0]
    c.ok(float((out - white).abs().max()) < 1e-3, "uniform white area is NOT darkened by highlight_tame (no local peak)")
    return c.report()


def test_highlight_tame_spares_midtones_and_tames_peaks():
    node, c = _node(), Check("naturalize_highlight_tame_midtones_peaks")
    img = torch.full((1, 64, 64, 3), 0.45)
    img[:, 30:34, 30:34, :] = 1.0  # small specular peak on a midtone field
    out = node.run(img, 0.0, 0.0, 0, highlight_tame=0.3)[0]
    far = (out - img)[:, :10, :10, :]
    c.ok(float(far.abs().max()) < 1e-4, "midtone field untouched")
    c.ok(float(out[:, 31, 31, :].mean()) < float(img[:, 31, 31, :].mean()) - 0.01, "the local bright peak is reduced")
    peaks = [float(node.run(img, 0.0, 0.0, 0, highlight_tame=a)[0][:, 31, 31, :].mean()) for a in (0.0, 0.3, 0.6, 1.0)]
    c.ok(all(x >= y for x, y in zip(peaks, peaks[1:])), f"peak reduction is monotone in highlight_tame: {[round(p, 4) for p in peaks]}")
    return c.report()


TESTS = (
    test_zero_strengths_are_identity,
    test_zero_new_strengths_match_historic_grain_dither,
    test_color_lock_full_takes_reference_chroma,
    test_color_lock_interpolation_is_monotone,
    test_color_lock_needs_reference,
    test_shapes_finite_and_deterministic,
    test_extreme_images,
    test_highlight_tame_spares_midtones_and_tames_peaks,
)

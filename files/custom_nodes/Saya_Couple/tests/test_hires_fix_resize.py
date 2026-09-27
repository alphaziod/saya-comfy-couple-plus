"""SayaHiresFixResize: identity / model upscaler then exact Lanczos / Lanczos only; never a resize before the model upscaler."""

import torch

from harness import Check, load_pack


def _mod():
    load_pack()
    from saya_couple.src.nodes import hires_fix_resize as m

    return m


class _FakeUpscaler:
    """Stands in for ImageUpscaleWithModel: bilinear x4, logs the input it received."""

    calls: list = []

    @classmethod
    def execute(cls, model, image):
        cls.calls.append(tuple(image.shape))
        up = torch.nn.functional.interpolate(image.movedim(-1, 1), scale_factor=4, mode="bilinear").movedim(1, -1)
        return type("Output", (), {"args": (up,)})()


def _run(m, image, target):
    import comfy_extras.nodes_upscale_model as nm

    real = nm.ImageUpscaleWithModel
    nm.ImageUpscaleWithModel = _FakeUpscaler
    _FakeUpscaler.calls = []
    try:
        return m.SayaHiresFixResize().resize(image, object(), *target)[0], list(_FakeUpscaler.calls)
    finally:
        nm.ImageUpscaleWithModel = real


def test_three_cases():
    m = _mod()
    c = Check("hires_resize_three_cases")
    image = torch.rand(1, 880, 1184, 3)
    out, calls = _run(m, image, (1184, 880))
    c.ok(out is image and not calls, "CASE 1 target == current size: image unchanged, no resize, no model upscaler")
    out, calls = _run(m, image, (1536, 1152))
    c.eq(tuple(out.shape), (1, 1152, 1536, 3), "CASE 2 upscale: exact final size")
    c.eq(calls, [(1, 880, 1184, 3)], "CASE 2: the model upscaler receives the ORIGINAL image (no plain resize before it)")
    out, calls = _run(m, image, (1152, 864))
    c.eq(tuple(out.shape), (1, 864, 1152, 3), "CASE 3 smaller target: exact size")
    c.ok(not calls, "CASE 3: no model upscaler")
    out, calls = _run(m, image, (1200, 896))  # +1.4 %: still an upscale -> model upscaler (locked rule)
    c.eq(tuple(out.shape), (1, 896, 1200, 3), "small upscale: exact size")
    c.eq(calls, [(1, 880, 1184, 3)], "small upscale: model upscaler first (never Lanczos alone to upscale)")
    out, calls = _run(m, image, (1184, 896))  # only one axis larger: still an upscale
    c.ok(len(calls) == 1 and tuple(out.shape) == (1, 896, 1184, 3), "one larger axis: model upscaler then exact size")
    out, calls = _run(m, torch.rand(1, 100, 100, 3), (1000, 1000))
    c.eq(tuple(out.shape), (1, 1000, 1000, 3), "large upscale (x10): exact size")
    c.ok(len(calls) >= 2, f"large upscale: several upscaler passes ({len(calls)})")
    return c.report()


def test_registry():
    pack = load_pack()
    c = Check("hires_resize_registry")
    name = "SayaHiresFixResize"
    c.ok(name in pack.NODE_CLASS_MAPPINGS and name in pack.NODE_DISPLAY_NAME_MAPPINGS, "enregistre")
    c.eq(list(pack.NODE_CLASS_MAPPINGS[name].INPUT_TYPES()["required"]), ["image", "upscale_model", "target_width", "target_height"], "inputs")
    return c.report()


TESTS = (test_three_cases, test_registry)

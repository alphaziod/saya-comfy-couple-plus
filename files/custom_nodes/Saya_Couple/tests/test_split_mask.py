"""SayaSplitMask geometry, sizing, dtype, and complement contracts."""

import torch

from harness import Check, load_pack


def test_split_mask_geometry():
    load_pack()
    from saya_couple.src.nodes.saya_split_mask import SayaSplitMask

    c = Check("split_mask_geometry")
    node = SayaSplitMask()
    schema = node.INPUT_TYPES()
    c.eq(list(schema["required"]), ["direction", "split", "blur", "swap", "latent"], "required input order")
    c.ok("optional" not in schema, "latent is required and no optional dimensions remain")
    latent = {"samples": torch.zeros(1, 4, 6, 8)}

    left, right = node.make_masks("vertical", 50, 0.0, latent)
    c.eq(tuple(left.shape), (1, 48, 64), "latent-derived mask shape")
    c.eq(left.dtype, torch.float32, "mask A float32")
    c.eq(right.dtype, torch.float32, "mask B float32")
    c.ok(torch.equal(left[0, :, :32], torch.ones(48, 32)), "vertical A is left half")
    c.ok(torch.equal(left[0, :, 32:], torch.zeros(48, 32)), "vertical A stops at split")
    c.ok(torch.equal(right, 1.0 - left), "vertical B exact complement")

    top, bottom = node.make_masks("horizontal", 50, 0.0, latent)
    c.ok(torch.equal(top[0, :24, :], torch.ones(24, 64)), "horizontal A is top half")
    c.ok(torch.equal(top[0, 24:, :], torch.zeros(24, 64)), "horizontal A stops at split")
    c.ok(torch.equal(bottom, 1.0 - top), "horizontal B exact complement")

    empty, full = node.make_masks("vertical", 0, 0.0, latent)
    c.ok(torch.count_nonzero(empty) == 0 and torch.all(full == 1), "split 0")
    full, empty = node.make_masks("vertical", 100, 0.0, latent)
    c.ok(torch.all(full == 1) and torch.count_nonzero(empty) == 0, "split 100")
    a0, b0 = node.make_masks("vertical", 30, 1.5, latent, swap=False)
    a1, b1 = node.make_masks("vertical", 30, 1.5, latent, swap=True)
    c.ok(torch.equal(a1, b0) and torch.equal(b1, a0), "swap exchanges only mask_A/mask_B")
    return c.report()


def test_split_mask_latent_and_blur():
    load_pack()
    from saya_couple.src.nodes.saya_split_mask import SayaSplitMask

    c = Check("split_mask_latent_and_blur")
    node = SayaSplitMask()
    latent = {"samples": torch.zeros(2, 4, 5, 7)}
    a, b = node.make_masks("vertical", 50, 0.0, latent)
    c.eq(tuple(a.shape), (2, 40, 56), "latent batch and 8x image dimensions")
    c.ok(torch.equal(a + b, torch.ones_like(a)), "sharp masks tile exactly")

    blurred_a, blurred_b = node.make_masks("vertical", 50, 2.0, latent)
    c.eq(tuple(blurred_a.shape), (2, 40, 56), "blurred latent-derived mask shape")
    c.ok(float(blurred_a.min()) >= 0.0 and float(blurred_a.max()) <= 1.0, "blur range")
    c.ok(torch.any((blurred_a > 0.0) & (blurred_a < 1.0)), "gaussian transition exists")
    c.ok(torch.allclose(blurred_a + blurred_b, torch.ones_like(blurred_a)), "blurred B exact complement")
    return c.report()


TESTS = (test_split_mask_geometry, test_split_mask_latent_and_blur)

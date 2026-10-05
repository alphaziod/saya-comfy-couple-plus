"""MultiMaskCouple mask grid: its patch guesses the attention grid from the latent size, and at sizes
that are not a multiple of 32 SDXL's rounded-up downsampling defeats the guess (1584x2320: 73x50 read
as 50x73, mask transposed). The Saya wrapper hands it the real activation grid."""

import inspect
import math

import torch

from harness import Check, load_pack


def _unet_grids(width, height):
    """Cross-attention grids of an SDXL UNet for an image of width x height (levels /2 and /4)."""
    h, w = height // 8, width // 8
    grids = []
    for _ in range(2):
        h, w = math.ceil(h / 2), math.ceil(w / 2)
        grids.append((h, w))
    return (height // 8, width // 8), grids


def _mmc():
    load_pack()
    from custom_nodes.MultiMaskCouple import attention_couple as mmc
    from saya_couple.src.nodes import saya_multi_couple as couple
    return mmc, couple


def test_grid_exact_for_every_resolution():
    mmc, _ = _mmc()
    c = Check("multimask_grid_every_resolution")
    wrong_before = wrong_after = total = 0
    for width in range(512, 2561, 8):
        for height in range(512, 2561, 8):
            latent, grids = _unet_grids(width, height)
            for h, w in grids:
                q = torch.empty(1, h * w, 1)
                total += 1
                wrong_before += mmc._q_spatial_from_original(q, (1, 4, *latent)) != (h, w)
                wrong_after += mmc._q_spatial_from_original(q, [1, 4, h, w]) != (h, w)
    c.eq(wrong_after, 0, f"real grid: exact on all {total} grids (512..2560 px, step 8)")
    c.ok(wrong_before > 0, f"guess from the latent: {wrong_before} of {total} grids wrong (the bug)")
    latent, grids = _unet_grids(1584, 2320)
    c.eq(mmc._q_spatial_from_original(torch.empty(1, 73 * 50, 1), (1, 4, *latent)), (50, 73),
         "1584x2320 without the fix: 73x50 read as 50x73 (transposed)")
    return c.report()


class _Attn(torch.nn.Module):
    def __init__(self, dim):
        super().__init__()
        self.to_k = torch.nn.Linear(dim, dim, bias=False)
        self.to_v = torch.nn.Linear(dim, dim, bias=False)
        torch.nn.init.eye_(self.to_k.weight)
        torch.nn.init.eye_(self.to_v.weight)


def _run_patch(mmc, couple, grid, latent, wrapped):
    """Two regions, left / right half of the frame; region A's value is +1, region B's -1.
    Returns the per-token output sign on the (h, w) grid: +1 where region A was applied."""
    h, w = grid
    dim = 4
    left = torch.zeros(latent[0] * 8, latent[1] * 8)
    left[:, : left.shape[1] // 2] = 1.0
    cond_a = torch.zeros(1, 1, dim); cond_a[..., 0] = 1.0
    cond_b = torch.zeros(1, 1, dim); cond_b[..., 0] = -1.0
    ac = mmc.AttentionCouple()
    ac.raw_positive = [[cond_a, {"mask": left, "mask_strength": 1.0}], [cond_b, {"mask": 1.0 - left, "mask_strength": 1.0}]]
    ac.raw_negative = [[torch.zeros(1, 1, dim), {"mask": left}], [torch.zeros(1, 1, dim), {"mask": 1.0 - left}]]
    patch = ac.make_patch(_Attn(dim))
    if wrapped:
        patch = couple._on_real_grid(patch)
    q = torch.zeros(1, h * w, dim)
    extra = {"cond_or_uncond": [0], "n_heads": 1, "original_shape": [1, 4, *latent], "activations_shape": [1, 320, h, w]}
    out = patch(q, None, None, extra)
    return out[0, :, 0].reshape(h, w).sign()


def test_patch_places_regions_on_the_real_grid():
    mmc, couple = _mmc()
    c = Check("multimask_patch_regions")
    latent, grids = _unet_grids(1584, 2320)
    grid = grids[1]  # 73x50: the level the guess gets wrong
    expected = torch.ones(grid)
    expected[:, grid[1] // 2:] = -1.0
    fixed = _run_patch(mmc, couple, grid, latent, wrapped=True)
    broken = _run_patch(mmc, couple, grid, latent, wrapped=False)
    agree_fixed = float((fixed == expected).float().mean())
    agree_broken = float((broken == expected).float().mean())
    c.ok(agree_fixed > 0.97, f"1584x2320, grid 73x50, with the fix: {agree_fixed:.1%} tokens in the right region")
    c.ok(agree_broken < 0.8, f"without the fix: only {agree_broken:.1%} (mask laid out transposed)")
    return c.report()


def test_unchanged_where_the_guess_was_right():
    mmc, couple = _mmc()
    c = Check("multimask_bit_identical_multiple_of_32")
    for width, height in ((832, 1216), (1664, 2432), (1024, 1024)):
        latent, grids = _unet_grids(width, height)
        for grid in grids:
            a = _run_patch(mmc, couple, grid, latent, wrapped=True)
            b = _run_patch(mmc, couple, grid, latent, wrapped=False)
            c.ok(torch.equal(a, b), f"{width}x{height} grid {grid}: identical with and without the fix")
    return c.report()


def test_production_couple_is_wrapped():
    _, couple = _mmc()
    c = Check("multimask_production_patches_wrapped")
    c.ok(inspect.signature(couple.apply_multimask_couple).parameters["couple_fn"].default is couple._attention_couple_patch,
         "apply_multimask_couple defaults to the wrapping couple function")
    wrapped = couple._on_real_grid(lambda q, k, v, extra: extra["original_shape"])
    c.eq(wrapped(None, None, None, {"original_shape": [1, 4, 290, 198], "activations_shape": [1, 640, 73, 50]}),
         [1, 640, 73, 50], "the wrapper hands the activation grid as original_shape")
    return c.report()


TESTS = [
    test_grid_exact_for_every_resolution,
    test_patch_places_regions_on_the_real_grid,
    test_unchanged_where_the_guess_was_right,
    test_production_couple_is_wrapped,
]

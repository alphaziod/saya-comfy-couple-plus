"""crop_mask_to_tile (attention_couple.common): per-tile slice, not a full-frame squash."""

import torch

from harness import Check, load_pack


def test_crop_mask_to_tile_left_vs_right_distinct():
    load_pack()
    from saya_couple.src.ppm_vendor.attention_couple.common import crop_mask_to_tile

    c = Check("crop_mask_to_tile_left_vs_right_distinct")
    # mask (num_conds=2, B=1, H=100, W=200): person channel = 0.0 on the left
    # half (P1), 1.0 on the right half (P2) — LEFT_RIGHT layout.
    mask = torch.zeros(2, 1, 100, 200)
    mask[1, :, :, 100:] = 1.0

    left_crop = {"crop_region": [0, 0, 50, 100], "full_width": 200, "full_height": 100}
    right_crop = {"crop_region": [150, 0, 200, 100], "full_width": 200, "full_height": 100}

    tile_left = crop_mask_to_tile(mask, left_crop)
    tile_right = crop_mask_to_tile(mask, right_crop)

    c.ok(torch.all(tile_left[1] == 0.0), "left tile: person channel = P1 (0.0) everywhere")
    c.ok(torch.all(tile_right[1] == 1.0), "right tile: person channel = P2 (1.0) everywhere")
    c.ok(not torch.equal(tile_left, tile_right), "left tile != right tile (real per-tile crop)")
    # neither tile shows the full-frame gradient (~0.5 average):
    c.ok(float(tile_left[1].mean()) != 0.5 and float(tile_right[1].mean()) != 0.5,
         "neither tile sees the full-frame squash (0.5 average)")
    return c.report()


def test_crop_mask_to_tile_never_full_frame_squash():
    load_pack()
    from saya_couple.src.ppm_vendor.attention_couple.common import crop_mask_to_tile

    c = Check("crop_mask_to_tile_never_full_frame_squash")
    mask = torch.rand(2, 1, 80, 160)
    crop = {"crop_region": [0, 0, 40, 40], "full_width": 160, "full_height": 80}
    tile = crop_mask_to_tile(mask, crop)
    c.ok(tuple(tile.shape[-2:]) != tuple(mask.shape[-2:]), "the cropped tile has a different spatial shape than the full mask")
    c.ok(tile.shape[-2] < mask.shape[-2] and tile.shape[-1] < mask.shape[-1], "the tile is strictly smaller (a real slice, not a passthrough)")
    return c.report()


def test_crop_mask_to_tile_fractional_mapping_no_drift():
    load_pack()
    from saya_couple.src.ppm_vendor.attention_couple.common import crop_mask_to_tile

    c = Check("crop_mask_to_tile_fractional_mapping_no_drift")
    # canvas = 2x the mask resolution (upscale_by=2, real USDU case): the
    # crop_region [100,0,200,120] (exact right half of the 200x120 canvas)
    # must map to the exact right half of the 100x60 mask (x[50:100]).
    mask_w, mask_h = 100, 60
    canvas_w, canvas_h = 200, 120
    mask = torch.zeros(1, 1, mask_h, mask_w)
    mask[:, :, :, mask_w // 2:] = 1.0  # right half = 1.0

    crop = {"crop_region": [canvas_w // 2, 0, canvas_w, canvas_h], "full_width": canvas_w, "full_height": canvas_h}
    tile = crop_mask_to_tile(mask, crop)
    c.eq(tuple(tile.shape[-2:]), (mask_h, mask_w // 2), "mapped window = exactly the right half of the mask (no drift)")
    c.ok(torch.all(tile == 1.0), "no contamination from the left half (0.0) into the right window")
    return c.report()


def test_crop_mask_to_tile_incomplete_metadata_degrades_to_full_mask():
    load_pack()
    from saya_couple.src.ppm_vendor.attention_couple.common import crop_mask_to_tile

    c = Check("crop_mask_to_tile_incomplete_metadata_degrades_to_full_mask")
    mask = torch.rand(2, 1, 40, 40)
    c.ok(torch.equal(crop_mask_to_tile(mask, {}), mask), "empty metadata -> degrades to the full mask (no crash)")
    c.ok(torch.equal(crop_mask_to_tile(mask, {"full_width": 40}), mask), "missing full_height -> degrades to the full mask")
    return c.report()


def test_crop_mask_to_tile_clamps_out_of_bounds_region():
    load_pack()
    from saya_couple.src.ppm_vendor.attention_couple.common import crop_mask_to_tile

    c = Check("crop_mask_to_tile_clamps_out_of_bounds_region")
    mask = torch.rand(2, 1, 40, 40)
    # region that slightly overruns the declared canvas (padding/clamp trap):
    crop = {"crop_region": [-5, -5, 45, 45], "full_width": 40, "full_height": 40}
    tile = crop_mask_to_tile(mask, crop)
    c.ok(tile.shape[-2] <= 40 and tile.shape[-1] <= 40, "window clamped within the mask bounds, no IndexError")
    return c.report()


TESTS = (
    test_crop_mask_to_tile_left_vs_right_distinct,
    test_crop_mask_to_tile_never_full_frame_squash,
    test_crop_mask_to_tile_fractional_mapping_no_drift,
    test_crop_mask_to_tile_incomplete_metadata_degrades_to_full_mask,
    test_crop_mask_to_tile_clamps_out_of_bounds_region,
)

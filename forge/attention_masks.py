"""Mask resizing helpers ported from SD Forge Attention Couple.

Upstream:
    https://github.com/Haoming02/sd-forge-couple

The upstream project is GPL-3.0 licensed.  See LICENSE bundled with this
package.  This file keeps the SD/SDXL attention-mask math intentionally small.
"""

from __future__ import annotations

import math

import torch
from torch.nn.functional import interpolate


def repeat_div(value: int, iterations: int) -> int:
    for _ in range(iterations):
        value = math.ceil(value / 2)
    return value


def get_mask(mask: torch.Tensor, batch_size: int, num_tokens: int, shape: tuple[int, ...]):
    """Resize ``[regions, 1, H, W]`` masks to the current attention resolution.

    This is the same sizing rule used by Forge Couple. ``shape`` is ComfyUI's
    ``extra_options['original_shape']`` (normally latent BCHW).
    """
    if len(shape) < 4:
        raise RuntimeError(
            "Saya Forge Couple: extra_options['original_shape'] must contain BCHW"
        )

    width, height = int(shape[3]), int(shape[2])
    if width <= 0 or height <= 0 or num_tokens <= 0:
        raise RuntimeError(
            f"Saya Forge Couple: invalid attention mask geometry "
            f"shape={shape!r} num_tokens={num_tokens}"
        )

    scale = math.ceil(math.log2(math.sqrt(height * width / num_tokens)))
    # The upstream formula assumes num_tokens <= H*W.  Clamp at zero so a
    # surprising high-resolution token count fails by shape later rather than
    # dividing by a negative number of stages.
    scale = max(0, scale)
    size = (repeat_div(height, scale), repeat_div(width, scale))

    num_conds = int(mask.shape[0])
    mask_downsample = interpolate(mask, size=size, mode="nearest")
    expected = int(num_tokens)
    actual = int(mask_downsample.shape[-2] * mask_downsample.shape[-1])
    if actual != expected:
        raise RuntimeError(
            "Saya Forge Couple: Forge mask resize did not match attention token count "
            f"({actual} != {expected}); original_shape={shape!r}, size={size}"
        )

    mask_downsample = mask_downsample.view(num_conds, num_tokens, 1)
    return mask_downsample.repeat_interleave(batch_size, dim=0)


def lcm(a: int, b: int) -> int:
    return a * b // math.gcd(a, b)


def lcm_for_list(numbers: list[int]) -> int:
    if not numbers:
        raise ValueError("Saya Forge Couple: lcm_for_list requires at least one value")
    current_lcm = int(numbers[0])
    for number in numbers[1:]:
        current_lcm = lcm(current_lcm, int(number))
    return current_lcm

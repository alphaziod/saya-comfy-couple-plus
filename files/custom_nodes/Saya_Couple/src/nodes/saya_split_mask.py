"""Two-region split masks with an exact complementary second region."""

from __future__ import annotations

import math

import torch
import torch.nn.functional as functional


class SayaSplitMask:
    """Create a left/right or top/bottom split at image-space resolution."""

    @classmethod
    def INPUT_TYPES(cls):
        return {
            "required": {
                "direction": (["vertical", "horizontal"], {"default": "vertical"}),
                "split": ("INT", {"default": 50, "min": 0, "max": 100, "step": 1}),
                "blur": ("FLOAT", {"default": 0.0, "min": 0.0, "step": 0.1}),
                "swap": ("BOOLEAN", {"default": False, "label_on": "true", "label_off": "false"}),
                "latent": ("LATENT",),
            },
        }

    RETURN_TYPES = ("MASK", "MASK")
    RETURN_NAMES = ("mask_A", "mask_B")
    FUNCTION = "make_masks"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Binary vertical or horizontal split. mask_B stays the exact "
        "complement of mask_A, including with Gaussian blur. swap only swaps "
        "the mask_A/mask_B (P1/P2) outputs, never the prompts or conditionings."
    )

    @staticmethod
    def _gaussian_blur(mask: torch.Tensor, sigma: float) -> torch.Tensor:
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

    def make_masks(self, direction, split, blur, latent, swap=False):
        direction = str(direction).lower()
        if direction not in {"vertical", "horizontal"}:
            direction = "vertical"
        split = max(0, min(100, int(split)))

        samples = latent["samples"]
        batch = int(samples.shape[0])
        height = int(samples.shape[-2]) * 8
        width = int(samples.shape[-1]) * 8

        mask_a = torch.zeros((batch, height, width), dtype=torch.float32)
        if direction == "vertical":
            boundary = round(width * split / 100.0)
            mask_a[:, :, :boundary] = 1.0
        else:
            boundary = round(height * split / 100.0)
            mask_a[:, :boundary, :] = 1.0

        mask_a = self._gaussian_blur(mask_a, blur)
        mask_b = 1.0 - mask_a
        if bool(swap):
            mask_a, mask_b = mask_b, mask_a
        return (mask_a, mask_b)

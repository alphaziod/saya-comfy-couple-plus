"""Two-region split masks with an exact complementary second region."""

from __future__ import annotations


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
        # One implementation for the whole pack (P-E, 2026-10-05): the same kernel / padding / clamp as before,
        # bit-identical (tests/test_fable_audit.py::blur_is_shared), now in region_masks.
        from .region_masks import gaussian_pixel_sigma_blur
        return gaussian_pixel_sigma_blur(mask, sigma)

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

"""SayaHiresFixResize -- applies the Phase 4 Hires Fix target in ONE coherent upscale.

Locked rule: any resolution increase goes through an UPSCALE MODEL; classic
resizing (Lanczos) is only an exact adjustment AFTER it, or the sole
operation for a decrease. Never a classic resize before the upscale model.

1. target == current size                       : image unchanged (no resize, no upscale model);
2. target larger on at least one axis (increase) : upscale model first, then exact Lanczos to the target;
3. target smaller or equal on both axes          : exact Lanczos alone, no upscale model.

The output image is exactly ``target_width x target_height``: it is what the
Couple reconstruction (masks at the final size) and the Hires Refine receive.
"""

from __future__ import annotations

from typing import Any

import comfy.utils

MAX_MODEL_PASSES = 3


def needs_model_upscale(width: int, height: int, target_width: int, target_height: int) -> bool:
    """An increase is needed as soon as the target exceeds the current image on one axis."""
    return target_width > width or target_height > height


class SayaHiresFixResize:
    """Current image -> image at the target size (upscale model as soon as an increase is needed)."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "image": ("IMAGE",),
                "upscale_model": ("UPSCALE_MODEL",),
                "target_width": ("INT", {"forceInput": True}),
                "target_height": ("INT", {"forceInput": True}),
            },
        }

    RETURN_TYPES = ("IMAGE",)
    RETURN_NAMES = ("image",)
    FUNCTION = "resize"
    CATEGORY = "saya/hidream"
    DESCRIPTION = (
        "Applies the Hires Fix target: identity if already at size; upscale model then exact Lanczos "
        "for an increase; exact Lanczos alone for a decrease. Never a classic resize before the upscale model."
    )

    def resize(self, image: Any, upscale_model: Any, target_width: int, target_height: int) -> tuple[Any]:
        _batch, height, width, _channels = (int(size) for size in image.shape)
        if (width, height) == (target_width, target_height):
            return (image,)
        if needs_model_upscale(width, height, target_width, target_height):
            from comfy_extras.nodes_upscale_model import ImageUpscaleWithModel

            for _ in range(MAX_MODEL_PASSES):
                image = ImageUpscaleWithModel.execute(upscale_model, image).args[0]
                if image.shape[2] >= target_width and image.shape[1] >= target_height:
                    break
        exact = comfy.utils.common_upscale(image.movedim(-1, 1), target_width, target_height, "lanczos", "disabled")
        return (exact.movedim(1, -1),)

"""Node: derive this pass' latent grid from the image it is processing.

Each pass of the pipeline can run at a different pixel resolution -- an
upscale, a tiled refine, a downscale back to the original size -- so the
latent grid has to be re-derived every time. This node reads the spatial
size of the image tensor of the CURRENT pass and produces a matching zero
latent, which downstream geometry (region masks in particular) uses as the
grid to rasterize on.

The two model-dependent properties of that latent are inputs, not
constants: the pixel-per-cell downscale and the channel count. A connected
VAE (or MODEL, for the channel count) overrides them with the real values,
because the SDXL side of this pipeline is 8x/4-channel while the HiDream
phase runs a Flux-family VAE with 16 latent channels. See
``src/duo_geometry/shape.py``.
"""

from __future__ import annotations

from typing import Any

from ..duo_geometry.shape import (
    DEFAULT_LATENT_CHANNELS,
    DEFAULT_LATENT_DOWNSCALE,
    empty_latent_like,
)


class SayaDuoLatentShape:
    """IMAGE -> zero LATENT on the grid of the current pass."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "image": ("IMAGE",),
                "latent_downscale": (
                    "INT",
                    {
                        "default": DEFAULT_LATENT_DOWNSCALE,
                        "min": 1,
                        "max": 512,
                        "step": 1,
                        "tooltip": (
                            "Pixels per latent cell. 8 for SD1.x/SDXL/Flux-family "
                            "VAEs. Ignored when a VAE is connected."
                        ),
                    },
                ),
                "latent_channels": (
                    "INT",
                    {
                        "default": DEFAULT_LATENT_CHANNELS,
                        "min": 1,
                        "max": 512,
                        "step": 1,
                        "tooltip": (
                            "Latent channels: 4 for SD1.x/SDXL, 16 for the "
                            "Flux-family VAE used by the HiDream phase. Ignored "
                            "when a VAE or MODEL is connected."
                        ),
                    },
                ),
            },
            "optional": {
                "vae": ("VAE",),
                "model": ("MODEL",),
            },
        }

    RETURN_TYPES = ("LATENT", "INT", "INT", "INT", "INT", "STRING")
    RETURN_NAMES = (
        "latent_shape",
        "width",
        "height",
        "latent_width",
        "latent_height",
        "derived_from",
    )
    FUNCTION = "build"
    CATEGORY = "Saya/Duo"
    DESCRIPTION = (
        "Derive the latent grid from the image of the current pass. The latent "
        "downscale and channel count come from a connected VAE/MODEL when "
        "available, never from a hard-coded resolution."
    )

    def build(
        self,
        image: Any,
        latent_downscale: int = DEFAULT_LATENT_DOWNSCALE,
        latent_channels: int = DEFAULT_LATENT_CHANNELS,
        vae: Any = None,
        model: Any = None,
    ) -> tuple[Any, ...]:
        latent, info = empty_latent_like(
            image,
            int(latent_downscale),
            int(latent_channels),
            vae=vae,
            model=model,
        )
        return (
            latent,
            info["width"],
            info["height"],
            info["latent_width"],
            info["latent_height"],
            info["source"],
        )

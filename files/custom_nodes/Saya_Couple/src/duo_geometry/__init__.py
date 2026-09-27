"""Saya geometry: everything that must match the CURRENT pass.

``shape``  IMAGE -> latent grid, with the model-dependent downscale and
           channel count derived rather than assumed.
``nodes``  the ComfyUI contract for the tiled-upscale pass.
"""

from __future__ import annotations

from .shape import (
    DEFAULT_LATENT_CHANNELS,
    DEFAULT_LATENT_DOWNSCALE,
    GeometryError,
    empty_latent_like,
    image_dimensions,
    latent_grid,
    latent_spec,
)

__all__ = [
    "DEFAULT_LATENT_CHANNELS",
    "DEFAULT_LATENT_DOWNSCALE",
    "GeometryError",
    "empty_latent_like",
    "image_dimensions",
    "latent_grid",
    "latent_spec",
]

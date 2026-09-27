"""Latent shape derivation for the CURRENT pass.

Every pass in the pipeline may run at a different pixel resolution: an
upscale, a tiled refine, a downscale back to the original size. A latent
grid derived once at generation time is therefore wrong for every later
pass, so this module derives it from the image tensor that is actually
being processed right now.

Two properties of a latent are model-dependent and must never be constants:

``downscale``
    Pixels per latent cell. SD1.x / SDXL VAEs use 8. Other families do
    not: a 4x TAESD is 4, several video/audio VAEs are 16, 32 or 512.

``channels``
    Latent channels. SD1.x / SDXL use 4, but the Flux-family VAEs used by
    the HiDream phase of this pipeline use 16, and other formats reach 32,
    64 or 128.

Both are read from the VAE (authoritative) or from the MODEL's latent
format when one is supplied, and otherwise fall back to explicit node
inputs whose defaults describe the SDXL side of the pipeline. Nothing here
is hard-coded to a resolution.
"""

from __future__ import annotations

from typing import Any

__all__ = [
    "DEFAULT_LATENT_CHANNELS",
    "DEFAULT_LATENT_DOWNSCALE",
    "GeometryError",
    "empty_latent_like",
    "image_dimensions",
    "latent_device",
    "latent_spec",
    "latent_grid",
]

# Fallbacks only: they describe the SD1.x / SDXL VAE, which is what the
# Illustrious/SDXL phases of this pipeline use. A connected VAE or MODEL
# always wins over them.
DEFAULT_LATENT_DOWNSCALE = 8
DEFAULT_LATENT_CHANNELS = 4


class GeometryError(ValueError):
    """Raised for an unusable image tensor or an impossible latent spec."""


def image_dimensions(image: Any) -> tuple[int, int, int, int]:
    """Return ``(batch, height, width, channels)`` of a ComfyUI IMAGE.

    ComfyUI IMAGE tensors are ``[batch, height, width, channels]`` floats.
    Anything else is rejected rather than reshaped.
    """
    shape = getattr(image, "shape", None)
    if shape is None or len(shape) != 4:
        raise GeometryError(
            "image must be a 4D IMAGE tensor [batch, height, width, channels], "
            f"got {shape!r}"
        )
    batch, height, width, channels = (int(size) for size in shape)
    if min(batch, height, width, channels) < 1:
        raise GeometryError(f"image has an empty dimension: {tuple(shape)}")
    if channels not in (1, 3, 4):
        raise GeometryError(
            f"image must have 1, 3 or 4 channels last, got {channels}; "
            "this looks like a [batch, channels, height, width] tensor"
        )
    return (batch, height, width, channels)


def _positive_int(value: Any, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise GeometryError(f"{name} must be an int, got {value!r}")
    if value < 1:
        raise GeometryError(f"{name} must be >= 1, got {value}")
    return value


def _spec_from_vae(vae: Any) -> tuple[int | None, int | None]:
    downscale = None
    compression = getattr(vae, "spacial_compression_encode", None)
    if callable(compression):
        try:
            value = compression()
        except Exception:  # pragma: no cover - defensive: exotic VAE wrappers
            value = None
        if isinstance(value, int) and not isinstance(value, bool) and value >= 1:
            downscale = int(value)
    if downscale is None:
        ratio = getattr(vae, "downscale_ratio", None)
        if isinstance(ratio, int) and not isinstance(ratio, bool) and ratio >= 1:
            downscale = int(ratio)
    channels = getattr(vae, "latent_channels", None)
    if not (isinstance(channels, int) and not isinstance(channels, bool) and channels >= 1):
        channels = None
    return (downscale, channels)


def _spec_from_model(model: Any) -> tuple[int | None, int | None]:
    latent_format = getattr(getattr(model, "model", None), "latent_format", None)
    if latent_format is None:
        return (None, None)
    channels = getattr(latent_format, "latent_channels", None)
    if not (isinstance(channels, int) and not isinstance(channels, bool) and channels >= 1):
        channels = None
    # A MODEL knows its latent channel count but not the VAE's spatial
    # compression, so it never supplies a downscale factor.
    return (None, channels)


def latent_spec(
    latent_downscale: int = DEFAULT_LATENT_DOWNSCALE,
    latent_channels: int = DEFAULT_LATENT_CHANNELS,
    *,
    vae: Any = None,
    model: Any = None,
) -> tuple[int, int, str]:
    """Resolve ``(downscale, channels, source)`` for this pass.

    Priority: the VAE (it owns the pixel/latent compression), then the
    MODEL's latent format for the channel count, then the explicit inputs.
    ``source`` names what was actually used, so a node can report it
    instead of silently assuming.
    """
    downscale = _positive_int(latent_downscale, "latent_downscale")
    channels = _positive_int(latent_channels, "latent_channels")
    sources: list[str] = []
    if vae is not None:
        vae_downscale, vae_channels = _spec_from_vae(vae)
        if vae_downscale is not None:
            downscale = vae_downscale
            sources.append("vae.downscale")
        if vae_channels is not None:
            channels = vae_channels
            sources.append("vae.channels")
    if model is not None:
        _, model_channels = _spec_from_model(model)
        if model_channels is not None and "vae.channels" not in sources:
            channels = model_channels
            sources.append("model.channels")
    if not sources:
        sources.append("inputs")
    return (downscale, channels, "+".join(sources))


def latent_grid(height: int, width: int, downscale: int) -> tuple[int, int]:
    """Latent ``(height, width)`` for a pixel size, refusing to round to zero."""
    downscale = _positive_int(downscale, "downscale")
    latent_height = int(height) // downscale
    latent_width = int(width) // downscale
    if latent_height < 1 or latent_width < 1:
        raise GeometryError(
            f"image {height}x{width} is smaller than one latent cell at "
            f"downscale {downscale}; no latent grid exists for this pass"
        )
    return (latent_height, latent_width)


def latent_device() -> Any:
    """The device ComfyUI stores graph latents on.

    ComfyUI's own EmptyLatentImage allocates on
    ``model_management.intermediate_device()`` (normally CPU) and lets the
    sampler move the tensor to the compute device. Following that keeps a
    freshly built latent from pinning VRAM, and keeps it compatible with
    every node that expects a graph latent. Outside ComfyUI, CPU.
    """
    import torch

    try:
        from comfy import model_management  # type: ignore

        return model_management.intermediate_device()
    except (ImportError, AttributeError):
        return torch.device("cpu")


def empty_latent_like(
    image: Any,
    latent_downscale: int = DEFAULT_LATENT_DOWNSCALE,
    latent_channels: int = DEFAULT_LATENT_CHANNELS,
    *,
    vae: Any = None,
    model: Any = None,
) -> tuple[dict[str, Any], dict[str, Any]]:
    """Build a zero LATENT matching ``image`` and report how it was derived.

    Returns ``(latent, info)`` where ``info`` carries the pixel size, the
    latent grid, and which source supplied the downscale/channel counts.
    """
    import torch

    batch, height, width, _channels = image_dimensions(image)
    downscale, channels, source = latent_spec(
        latent_downscale, latent_channels, vae=vae, model=model
    )
    latent_height, latent_width = latent_grid(height, width, downscale)
    samples = torch.zeros(
        (batch, channels, latent_height, latent_width),
        dtype=torch.float32,
        device=latent_device(),
    )
    info = {
        "batch": batch,
        "height": height,
        "width": width,
        "latent_height": latent_height,
        "latent_width": latent_width,
        "downscale": downscale,
        "channels": channels,
        "source": source,
    }
    return ({"samples": samples}, info)

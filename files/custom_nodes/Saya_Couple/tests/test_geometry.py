"""Geometry services: latent grid derivation and image tensor contracts."""

import torch

from harness import Check, load_pack


def _shape():
    load_pack()
    from saya_couple.src.duo_geometry import shape

    return shape


class _FakeVAE:
    def __init__(self, downscale=8, channels=4):
        self.downscale_ratio = downscale
        self.latent_channels = channels


class _FakeModel:
    def __init__(self, channels):
        class _LF:
            latent_channels = channels

        self.model = type("M", (), {"latent_format": _LF()})()


def test_image_dimensions():
    shape = _shape()
    c = Check("image_dimensions")
    img = torch.zeros(2, 64, 96, 3)
    c.eq(shape.image_dimensions(img), (2, 64, 96, 3), "BHWC unpack")
    c.raises(shape.GeometryError, lambda: shape.image_dimensions(torch.zeros(64, 64, 3)), "3D rejected")
    c.raises(shape.GeometryError, lambda: shape.image_dimensions(torch.zeros(1, 3, 64, 64)), "BCHW rejected")
    c.raises(shape.GeometryError, lambda: shape.image_dimensions(torch.zeros(0, 64, 64, 3)), "empty batch rejected")
    c.raises(shape.GeometryError, lambda: shape.image_dimensions(torch.zeros(1, 64, 64, 5)), "5 channels rejected")
    return c.report()


def test_latent_grid():
    shape = _shape()
    c = Check("latent_grid")
    c.eq(shape.latent_grid(1024, 1536, 8), (128, 192), "SDXL grid")
    c.eq(shape.latent_grid(1023, 1535, 8), (127, 191), "floor division")
    c.raises(shape.GeometryError, lambda: shape.latent_grid(4, 4, 8), "sub-cell image rejected")
    c.raises(shape.GeometryError, lambda: shape.latent_grid(64, 64, 0), "zero downscale rejected")
    return c.report()


def test_latent_spec_sources():
    shape = _shape()
    c = Check("latent_spec_sources")
    d, ch, src = shape.latent_spec()
    c.eq((d, ch, src), (8, 4, "inputs"), "defaults are SDXL fallbacks")
    d, ch, src = shape.latent_spec(vae=_FakeVAE(8, 16))
    c.eq((d, ch), (8, 16), "VAE overrides both")
    c.ok("vae" in src, "source names the VAE")
    d, ch, src = shape.latent_spec(vae=_FakeVAE(16, 4), model=_FakeModel(32))
    c.eq((d, ch), (16, 4), "VAE wins over MODEL channels")
    d, ch, src = shape.latent_spec(model=_FakeModel(16))
    c.eq((d, ch), (8, 16), "MODEL supplies channels only")
    return c.report()


def test_empty_latent_like():
    shape = _shape()
    c = Check("empty_latent_like")
    img = torch.zeros(1, 512, 768, 3)
    latent, info = shape.empty_latent_like(img)
    samples = latent["samples"]
    c.eq(tuple(samples.shape), (1, 4, 64, 96), "zero latent grid")
    c.eq(float(samples.abs().sum()), 0.0, "latent is zeros")
    c.eq((info["latent_height"], info["latent_width"]), (64, 96), "info grid")

    latent16, info16 = shape.empty_latent_like(img, vae=_FakeVAE(8, 16))
    c.eq(tuple(latent16["samples"].shape), (1, 16, 64, 96), "16-channel VAE latent")
    c.eq(info16["source"], "vae.downscale+vae.channels", "info source")
    return c.report()


TESTS = (
    test_image_dimensions,
    test_latent_grid,
    test_latent_spec_sources,
    test_empty_latent_like,
)

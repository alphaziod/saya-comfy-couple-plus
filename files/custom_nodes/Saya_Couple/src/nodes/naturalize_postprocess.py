import torch
import torch.nn.functional as F

from .chroma_anchor import oklab_to_rgb, rgb_to_oklab
from .region_masks import gaussian_pixel_sigma_blur

#: OKLab lightness above which highlight_tame starts to act (smoothstep up to 1.0).
HIGHLIGHT_THRESHOLD = 0.7
#: Neighbourhood used to tell a local highlight peak from a large bright area, relative to the long side.
HIGHLIGHT_SIGMA_FRACTION = 0.004


#: Scale of the colour that color_lock corrects, relative to the long side: only the colour of areas (skin, hair,
#: clothes, light), never the colour of lines and small details (eyes, lashes, mouth), which follow the refine.
LOCK_SIGMA_FRACTION = 0.006


def _blur_channels(x, sigma):
    """Separable gaussian over [B, H, W, C] (sigma in pixels), reflect padding, no clamping (a/b are signed)."""
    radius = max(1, int(3 * sigma + 0.5))
    coords = torch.arange(-radius, radius + 1, dtype=torch.float32, device=x.device)
    kernel = torch.exp(-(coords * coords) / (2 * sigma * sigma))
    kernel = (kernel / kernel.sum()).to(x.dtype)
    channels = x.shape[-1]
    work = x.movedim(-1, 1)
    pad_h, pad_w = min(radius, work.shape[2] - 1), min(radius, work.shape[3] - 1)
    k_w = kernel[radius - pad_w: radius + pad_w + 1].view(1, 1, 1, -1).expand(channels, 1, 1, -1)
    k_h = kernel[radius - pad_h: radius + pad_h + 1].view(1, 1, -1, 1).expand(channels, 1, -1, 1)
    work = F.conv2d(F.pad(work, (pad_w, pad_w, 0, 0), mode="reflect" if pad_w else "replicate"), k_w, groups=channels)
    work = F.conv2d(F.pad(work, (0, 0, pad_h, pad_h), mode="reflect" if pad_h else "replicate"), k_h, groups=channels)
    return work.movedim(1, -1)


def lock_chroma(image, reference, amount):
    """Keep ``image``'s OKLab lightness and fine colour, pull its LOW-FREQUENCY a/b toward ``reference``'s by ``amount``.

    Only the blurred colour is corrected (ab + amount * (blur(ref ab) - blur(ab))): the refine redraws lines by a pixel
    or two even at a low denoise, and a per-pixel lock pasted the old line colour next to the new line (colour fringes
    on eyes, lashes, mouth). The colour drift of areas is still removed.
    """
    reference = reference.to(device=image.device, dtype=torch.float32)
    if reference.shape[1:3] != image.shape[1:3]:
        reference = F.interpolate(reference.movedim(-1, 1), size=image.shape[1:3], mode="bilinear", align_corners=False).movedim(1, -1)
    lab = rgb_to_oklab(image.clamp(0, 1))
    reference_ab = rgb_to_oklab(reference.clamp(0, 1))[..., 1:]
    if reference_ab.shape[0] != lab.shape[0]:
        reference_ab = reference_ab.expand(lab.shape[0], -1, -1, -1)
    sigma = max(2.0, LOCK_SIGMA_FRACTION * max(image.shape[1], image.shape[2]))
    drift = _blur_channels(reference_ab - lab[..., 1:], sigma)
    ab = lab[..., 1:] + amount * drift
    return oklab_to_rgb(torch.cat([lab[..., :1], ab], dim=-1))


def tame_highlights(image, amount):
    """Pull local bright peaks (glow, glossy speculars) back toward their neighbourhood.

    Only OKLab lightness above HIGHLIGHT_THRESHOLD and above its blurred surroundings is
    reduced: midtones and large uniformly bright areas (white eyes, white fabric) are kept.
    """
    lab = rgb_to_oklab(image.clamp(0, 1))
    lightness = lab[..., 0]
    sigma = max(2.0, HIGHLIGHT_SIGMA_FRACTION * max(image.shape[1], image.shape[2]))
    surrounding = gaussian_pixel_sigma_blur(lightness, sigma)
    weight = ((lightness - HIGHLIGHT_THRESHOLD) / (1 - HIGHLIGHT_THRESHOLD)).clamp(0, 1)
    weight = weight * weight * (3 - 2 * weight)
    tamed = lightness - amount * weight * (lightness - surrounding).clamp_min(0)
    return oklab_to_rgb(torch.cat([tamed.unsqueeze(-1), lab[..., 1:]], dim=-1))


class SayaNaturalizePostProcess:
    """Deterministic finishing after the final resize: colour lock, highlight taming, fine grain and dither."""

    RETURN_TYPES = ("IMAGE",)
    FUNCTION = "run"
    CATEGORY = "Saya/Image"
    DESCRIPTION = ("Deterministic finishing. color_lock keeps the refine's lightness with the reference's OKLab colour; "
                   "highlight_tame pulls local glow/specular peaks down; then fine grain and dither. "
                   "All strengths at zero preserve the image exactly.")

    @classmethod
    def INPUT_TYPES(cls):
        return {"required": {
            "image": ("IMAGE",),
            "grain_strength": ("FLOAT", {"default": 0.0, "min": 0.0, "max": 0.03, "step": 0.001,
                "tooltip": "Grain amplitude in display RGB. Start with 0.003; shadows and highlights are protected."}),
            "dither_amount": ("FLOAT", {"default": 0.0, "min": 0.0, "max": 2.0, "step": 0.1,
                "tooltip": "Uniform monochrome dither in 8-bit code values. Start with 0.5 before an 8-bit save."}),
            "seed": ("INT", {"default": 0, "min": 0, "max": 0xFFFFFFFFFFFFFFFF}),
        }, "optional": {
            "reference": ("IMAGE", {"tooltip": "Colour reference for color_lock: the image before the refine, aligned with image."}),
            "color_lock": ("FLOAT", {"default": 0.0, "min": 0.0, "max": 1.0, "step": 0.05,
                "tooltip": "0 = refine colours kept, 1 = reference OKLab colour of areas (blurred) with the refine's lightness, lines and fine colour."}),
            "highlight_tame": ("FLOAT", {"default": 0.0, "min": 0.0, "max": 1.0, "step": 0.05,
                "tooltip": "0 = off (recommended). Pulls local highlight peaks toward their surroundings; it also hits the light side of every line and the eye catchlights."}),
        }}

    def run(self, image, grain_strength, dither_amount, seed, reference=None, color_lock=0.0, highlight_tame=0.0):
        if grain_strength == 0 and dither_amount == 0 and color_lock == 0 and highlight_tame == 0:
            return (image,)

        result = image.float()
        if color_lock > 0:
            if reference is None:
                raise ValueError("color_lock > 0 needs the reference image (the image before the refine)")
            result = lock_chroma(result, reference, color_lock)
        if highlight_tame > 0:
            result = tame_highlights(result, highlight_tame)

        batch, height, width, _ = image.shape
        # MPS has no device-local Generator; only that backend needs a transfer.
        random_device = "cpu" if image.device.type == "mps" else image.device
        generator = torch.Generator(device=random_device).manual_seed(seed)
        noise_shape = (batch, 1, height, width)

        if grain_strength > 0:
            noise = torch.randn(noise_shape, device=random_device, generator=generator).to(image.device)
            # Remove low-frequency noise without blurring the image or its linework.
            grain = noise - F.avg_pool2d(noise, 3, stride=1, padding=1, count_include_pad=False)
            grain = grain.movedim(1, -1)
            luminance = (0.2126 * result[..., 0:1] + 0.7152 * result[..., 1:2] + 0.0722 * result[..., 2:3]).clamp(0, 1)
            protection = 4 * luminance * (1 - luminance)
            result = result + grain * protection * grain_strength

        if dither_amount > 0:
            noise = torch.rand(noise_shape, device=random_device, generator=generator).to(image.device)
            result = result + (noise.movedim(1, -1) - 0.5) * (dither_amount / 255)

        return (result.clamp(0, 1).to(image.dtype),)

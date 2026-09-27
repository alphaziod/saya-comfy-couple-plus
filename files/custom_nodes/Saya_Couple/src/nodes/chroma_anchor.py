import torch
import torch.nn.functional as F

from comfy.utils import repeat_to_batch_size


_RGB_TO_LMS = [[0.4122214708, 0.5363325363, 0.0514459929],
               [0.2119034982, 0.6806995451, 0.1073969566],
               [0.0883024619, 0.2817188376, 0.6299787005]]
_LMS_TO_OKLAB = [[0.2104542553, 0.7936177850, -0.0040720468],
                 [1.9779984951, -2.4285922050, 0.4505937099],
                 [0.0259040371, 0.7827717662, -0.8086757660]]
_OKLAB_TO_LMS = [[1.0, 0.3963377774, 0.2158037573],
                 [1.0, -0.1055613458, -0.0638541728],
                 [1.0, -0.0894841775, -1.2914855480]]
_LMS_TO_RGB = [[4.0767416621, -3.3077115913, 0.2309699292],
               [-1.2684380046, 2.6097574011, -0.3413193965],
               [-0.0041960863, -0.7034186147, 1.7076147010]]


def rgb_to_oklab(rgb):
    """sRGB (linearized, then LMS) -> OKLab, batched over the tensor's last axis."""
    linear = torch.where(rgb <= 0.04045, rgb / 12.92, ((rgb + 0.055) / 1.055).pow(2.4))
    lms = linear @ rgb.new_tensor(_RGB_TO_LMS).T
    return lms.clamp_min(0).pow(1 / 3) @ rgb.new_tensor(_LMS_TO_OKLAB).T


def oklab_to_rgb(lab):
    """Inverse of ``rgb_to_oklab``: OKLab -> LMS -> linear -> sRGB, clamped to [0, 1]."""
    lms = (lab @ lab.new_tensor(_OKLAB_TO_LMS).T).pow(3)
    linear = lms @ lab.new_tensor(_LMS_TO_RGB).T
    rgb = torch.where(linear <= 0.0031308, linear * 12.92, 1.055 * linear.clamp_min(0).pow(1 / 2.4) - 0.055)
    return rgb.clamp(0, 1)


class SayaChromaAnchor:
    """Cap the refine's chroma against an aligned reference, in OKLab.

    Keeps the refine's own lightness and hue; only chroma is pulled back
    toward what the reference actually had, so refinement cannot invent
    saturation the source image never contained.
    """

    RETURN_TYPES = ("IMAGE",)
    FUNCTION = "run"
    CATEGORY = "Saya/Image"
    DESCRIPTION = "Limit chroma gain against an aligned reference while retaining the refine's OKLab lightness and hue. Strength 0 bypasses exactly."

    @classmethod
    def INPUT_TYPES(cls):
        return {"required": {
            "refined": ("IMAGE", {"tooltip": "RGB image after refinement."}),
            "reference": ("IMAGE", {"tooltip": "Aligned RGB image before refinement. Resized if needed; a single reference serves the whole batch."}),
            "max_chroma_gain": ("FLOAT", {"default": 1.15, "min": 1.0, "max": 3.0, "step": 0.01,
                "tooltip": "Allowed chroma = reference chroma × gain + neutral allowance."}),
            "neutral_allowance": ("FLOAT", {"default": 0.01, "min": 0.0, "max": 0.2, "step": 0.001,
                "tooltip": "Additional OKLab chroma allowed, including near neutral reference pixels."}),
            "strength": ("FLOAT", {"default": 1.0, "min": 0.0, "max": 1.0, "step": 0.01,
                "tooltip": "0 bypasses; 1 fully applies the chroma cap."}),
        }}

    def run(self, refined, reference, max_chroma_gain, neutral_allowance, strength):
        if strength <= 0:
            return (refined,)

        image = refined.float().clamp(0, 1)
        reference = reference.to(device=image.device, dtype=torch.float32).clamp(0, 1)
        if reference.shape[1:3] != image.shape[1:3]:
            reference = F.interpolate(reference.movedim(-1, 1), size=image.shape[1:3], mode="bilinear", align_corners=False).movedim(1, -1)

        lab = rgb_to_oklab(image)
        reference_chroma = torch.linalg.vector_norm(rgb_to_oklab(reference)[..., 1:], dim=-1, keepdim=True)
        if reference_chroma.shape[0] not in (1, image.shape[0]):
            reference_chroma = repeat_to_batch_size(reference_chroma, image.shape[0])
        chroma = torch.linalg.vector_norm(lab[..., 1:], dim=-1, keepdim=True)
        cap = reference_chroma * max_chroma_gain + neutral_allowance
        scale = (cap / chroma.clamp_min(1e-6)).clamp(max=1)
        # Blend chroma in OKLab so partial strength also retains the refine's L.
        scale = 1 + strength * (scale - 1)
        anchored = torch.cat([lab[..., :1], lab[..., 1:] * scale], dim=-1)
        result = oklab_to_rgb(anchored).to(refined.dtype)
        return (torch.where(chroma > cap, result, refined),)

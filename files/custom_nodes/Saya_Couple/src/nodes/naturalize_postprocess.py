import torch
import torch.nn.functional as F


class SayaNaturalizePostProcess:
    """Optional fine monochrome grain and dither, applied after the final resize."""

    RETURN_TYPES = ("IMAGE",)
    FUNCTION = "run"
    CATEGORY = "Saya/Image"
    DESCRIPTION = "Optional fine monochrome grain. Place after the final resize; zero strengths preserve the image exactly."

    @classmethod
    def INPUT_TYPES(cls):
        return {"required": {
            "image": ("IMAGE",),
            "grain_strength": ("FLOAT", {"default": 0.0, "min": 0.0, "max": 0.03, "step": 0.001,
                "tooltip": "Grain amplitude in display RGB. Start with 0.003; shadows and highlights are protected."}),
            "dither_amount": ("FLOAT", {"default": 0.0, "min": 0.0, "max": 2.0, "step": 0.1,
                "tooltip": "Uniform monochrome dither in 8-bit code values. Start with 0.5 before an 8-bit save."}),
            "seed": ("INT", {"default": 0, "min": 0, "max": 0xFFFFFFFFFFFFFFFF}),
        }}

    def run(self, image, grain_strength, dither_amount, seed):
        if grain_strength == 0 and dither_amount == 0:
            return (image,)

        result = image.float()
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

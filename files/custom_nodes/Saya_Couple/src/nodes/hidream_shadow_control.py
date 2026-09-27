"""Luminance-based refine mask for the HiDream refiner pass.

Deep blacks turning into dense/tarry patches during refine is a spatial problem: some
pixels get resampled at full strength when they should barely move. This node only
produces a MASK from the image's own luminance; it does not touch the VAE, LoRA, sampler,
scheduler or global denoise, and it does not blend pixels after the fact. Wire refine_mask
into the core "Set Latent Noise Mask" node before the refiner KSampler to get a real
per-pixel denoise strength during sampling. See MANUAL_GPU_TESTS or the review notes for
exactly where to insert this in the HiDream subgraph.
"""


class SayaHiDreamShadowControlMask:
    """Build a luminance-based refine/protect MASK pair; touches nothing else (see module docstring)."""

    RETURN_TYPES = ("MASK", "MASK")
    RETURN_NAMES = ("refine_mask", "protect_mask")
    FUNCTION = "run"
    CATEGORY = "saya/hidream"
    DESCRIPTION = (
        "MASK from image luminance: white refines normally, black is mostly protected. "
        "Feed refine_mask into the core Set Latent Noise Mask node, before the refiner "
        "KSampler. protection_strength=0 disables this (mask stays all white)."
    )

    @classmethod
    def INPUT_TYPES(cls):
        return {"required": {
            "image": ("IMAGE",),
            "black_threshold": ("FLOAT", {"default": 0.05, "min": 0.0, "max": 1.0, "step": 0.01,
                "tooltip": "Luminance at or below this is deep black: refine strength drops to minimum_refine_strength."}),
            "shadow_threshold": ("FLOAT", {"default": 0.25, "min": 0.0, "max": 1.0, "step": 0.01,
                "tooltip": "Luminance at or above this refines at full strength. Keep it above black_threshold."}),
            "softness": ("FLOAT", {"default": 0.05, "min": 0.0, "max": 0.5, "step": 0.01,
                "tooltip": "Extra luminance margin added on both sides of the two thresholds to round the transition."}),
            "protection_strength": ("FLOAT", {"default": 1.0, "min": 0.0, "max": 1.0, "step": 0.01,
                "tooltip": "0 disables the mask entirely (full refine everywhere, same as no node). 1 applies the curve as configured."}),
            "minimum_refine_strength": ("FLOAT", {"default": 0.15, "min": 0.0, "max": 1.0, "step": 0.01,
                "tooltip": "Refine strength floor for the deepest blacks, so they are dampened but not frozen solid."}),
            "invert": ("BOOLEAN", {"default": False,
                "tooltip": "Protect the brightest pixels instead of the darkest ones."}),
        }}

    def run(self, image, black_threshold, shadow_threshold, softness,
             protection_strength, minimum_refine_strength, invert):
        luminance = (0.2126 * image[..., 0] + 0.7152 * image[..., 1] + 0.0722 * image[..., 2]).clamp(0.0, 1.0)
        if invert:
            luminance = 1.0 - luminance

        black_threshold = min(max(black_threshold, 0.0), 1.0)
        shadow_threshold = min(max(shadow_threshold, 0.0), 1.0)
        softness = min(max(softness, 0.0), 0.5)
        protection_strength = min(max(protection_strength, 0.0), 1.0)
        floor = min(max(minimum_refine_strength, 0.0), 1.0)

        low_edge = max(0.0, black_threshold - softness)
        high_edge = max(low_edge + 1e-4, min(1.0, shadow_threshold + softness))

        t = ((luminance - low_edge) / (high_edge - low_edge)).clamp(0.0, 1.0)
        ramp = t * t * (3.0 - 2.0 * t)  # smoothstep easing

        curve = floor + (1.0 - floor) * ramp
        refine_mask = 1.0 - protection_strength * (1.0 - curve)
        protect_mask = 1.0 - refine_mask
        return (refine_mask, protect_mask)

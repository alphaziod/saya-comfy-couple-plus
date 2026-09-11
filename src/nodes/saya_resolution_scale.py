from __future__ import annotations

import math
from typing import Any


class SayaResolutionScaleCalculator:
    """Resolution calculator copied from the local DaSiWa node and simplified for Saya."""

    # Clean, exact AI-friendly generation presets only.
    #
    # Every pair below is divisible by 64 (so also by 32 and by 8), which keeps
    # them safe generation sizes for SDXL/FLUX (div-8), WAN/LTX (div-32), and
    # any div-64 latent bucket. The "A:B" ratio embedded in the name before
    # " · " (e.g. "Landscape 16:9") is read by
    # web/saya_resolution_ratio_filter.js, which filters this list down to the
    # presets matching whatever ratio is picked in aspect_preset_when_not_image
    # — matched as plain "A:B" text, so no ratio table is duplicated in JS.
    _RATIO_FAMILIES: dict[str, list[tuple[int, int]]] = {
        "Square 1:1": [(768, 768), (1024, 1024), (1280, 1280)],
        "Landscape 4:3": [(1024, 768), (1280, 960), (1536, 1152)],
        "Portrait 3:4": [(768, 1024), (960, 1280), (1152, 1536)],
        "Landscape 3:2": [(960, 640), (1152, 768), (1216, 832), (1344, 896)],
        "Portrait 2:3": [(640, 960), (768, 1152), (832, 1216), (896, 1344)],
        "Landscape 16:9": [(896, 512), (1152, 640), (1344, 768), (1600, 896)],
        "Portrait 9:16": [(512, 896), (640, 1152), (768, 1344), (896, 1600)],
        "Ultrawide 21:9": [(1344, 576), (1600, 704), (1792, 768)],
        "Ultrawide Portrait 9:21": [(576, 1344), (704, 1600), (768, 1792)],
    }

    FIXED_RESOLUTION_PRESETS = {
        f"{family} · {w}x{h}": (w, h)
        for family, sizes in _RATIO_FAMILIES.items()
        for w, h in sizes
    }

    PRESETS = FIXED_RESOLUTION_PRESETS

    ASPECT_PRESETS = {
        "All (no filter)": (0, 0),
        "1:1 - Square": (1, 1),
        "4:3 - Landscape": (4, 3),
        "3:4 - Portrait": (3, 4),
        "3:2 - Landscape": (3, 2),
        "2:3 - Portrait": (2, 3),
        "16:9 - Landscape": (16, 9),
        "9:16 - Portrait": (9, 16),
        "21:9 - Ultrawide": (21, 9),
        "9:21 - Ultrawide Portrait": (9, 21),
        "CUSTOM": (0, 0),
    }

    DESCRIPTION = """
    Saya Resolution Scale Calculator

    Exact Saya presets only:
    - clean labels
    - AI-friendly divisible sizes
    - exact dimensions for generation
    - no weird CivitAI / Scene naming

    Fixed presets return their dimensions verbatim.
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, Any]:
        return {
            "required": {
                "resolution_preset": (
                    list(cls.PRESETS.keys()),
                    {
                        "default": "Landscape 16:9 · 1344x768",
                        "description": "Exact Saya generation resolutions only.",
                    },
                ),
                "no_scale": (
                    "BOOLEAN",
                    {
                        "default": False,
                        "label_on": "ON (Source Dims)",
                        "label_off": "OFF (Calculated)",
                        "description": (
                            "Bypass calculations and output the source dimensions."
                        ),
                    },
                ),
                "scale_from_image": (
                    "BOOLEAN",
                    {
                        "default": False,
                        "label_on": "IMAGE ASPECT",
                        "label_off": "USE ASPECT BELOW",
                        "description": (
                            "Used by megapixel targets. Fixed presets ignore the "
                            "image aspect."
                        ),
                    },
                ),
                "aspect_preset_when_not_image": (
                    list(cls.ASPECT_PRESETS.keys()),
                    {
                        "default": "16:9 - Landscape",
                        "description": (
                            "Filters resolution_preset down to matching-ratio "
                            "sizes (pick CUSTOM to filter by the width/height "
                            "below). Also used by megapixel targets when IMAGE "
                            "ASPECT is off."
                        ),
                    },
                ),
                "swap_aspect_when_not_image": (
                    "BOOLEAN",
                    {
                        "default": False,
                        "label_on": "yes",
                        "label_off": "no",
                        "description": (
                            "Flip width and height. Also rotates a fixed preset."
                        ),
                    },
                ),
                "custom_aspect_width": (
                    "INT",
                    {
                        "default": 16,
                        "min": 1,
                        "max": 9999,
                        "step": 1,
                        "description": "Custom aspect width when CUSTOM is selected.",
                    },
                ),
                "custom_aspect_height": (
                    "INT",
                    {
                        "default": 9,
                        "min": 1,
                        "max": 9999,
                        "step": 1,
                        "description": "Custom aspect height when CUSTOM is selected.",
                    },
                ),
                "mode": (
                    ["WAN/LTX (Div32)", "FLUX/SDXL (Div8)", "Custom Divisor"],
                    {
                        "default": "WAN/LTX (Div32)",
                        "description": "Divisor snapping mode.",
                    },
                ),
                "custom_divisor": (
                    "INT",
                    {
                        "default": 8,
                        "min": 1,
                        "max": 512,
                        "step": 1,
                        "description": "Divisor used when Custom Divisor is selected.",
                    },
                ),
            },
            "optional": {
                "image": ("IMAGE",),
            },
        }

    RETURN_TYPES = ("INT", "INT", "FLOAT", "FLOAT")
    RETURN_NAMES = ("width_int", "height_int", "width_float", "height_float")
    FUNCTION = "calculate"
    CATEGORY = "Saya/Scaling"

    def calculate(
        self,
        resolution_preset: str,
        no_scale: bool,
        scale_from_image: bool,
        aspect_preset_when_not_image: str,
        swap_aspect_when_not_image: bool,
        custom_aspect_width: int,
        custom_aspect_height: int,
        mode: str,
        custom_divisor: int,
        image=None,
    ):
        if image is not None:
            _, image_h, image_w, _ = image.shape
            source_w, source_h = int(image_w), int(image_h)
        else:
            source_w, source_h = 1024, 1024

        # aspect_preset_when_not_image is authoritative.  The frontend normally
        # filters resolution_preset to the same ratio family, but promoted
        # subgraph widgets in newer ComfyUI builds can keep a stale combo value.
        # Never let a stale 16:9 value force the actual generation back to 16:9.
        del scale_from_image, mode, custom_divisor

        effective_preset = resolution_preset
        requested_ratio: tuple[int, int] | None = None

        if aspect_preset_when_not_image == "CUSTOM":
            custom_w = max(1, int(custom_aspect_width))
            custom_h = max(1, int(custom_aspect_height))
            divisor = math.gcd(custom_w, custom_h)
            requested_ratio = (custom_w // divisor, custom_h // divisor)
        elif aspect_preset_when_not_image != "All (no filter)":
            requested_ratio = self.ASPECT_PRESETS.get(aspect_preset_when_not_image)

        if requested_ratio and requested_ratio != (0, 0):
            ratio_token = f"{requested_ratio[0]}:{requested_ratio[1]}"

            def preset_ratio_token(name: str) -> str | None:
                family = name.split(" · ", 1)[0]
                for token in family.split():
                    if ":" in token:
                        left, _, right = token.partition(":")
                        if left.isdigit() and right.isdigit():
                            return f"{int(left)}:{int(right)}"
                return None

            if preset_ratio_token(effective_preset) != ratio_token:
                candidates = [
                    name
                    for name in self.FIXED_RESOLUTION_PRESETS
                    if preset_ratio_token(name) == ratio_token
                ]
                if candidates:
                    # Preserve the user's approximate resolution tier when the
                    # ratio changes.  Example: 1344x768 (about 1.03 MP) ->
                    # 1024x1024 for 1:1, rather than always taking the smallest.
                    old_dims = self.FIXED_RESOLUTION_PRESETS.get(effective_preset)
                    old_pixels = (
                        old_dims[0] * old_dims[1]
                        if old_dims
                        else self.FIXED_RESOLUTION_PRESETS["Landscape 16:9 · 1344x768"][0]
                        * self.FIXED_RESOLUTION_PRESETS["Landscape 16:9 · 1344x768"][1]
                    )
                    effective_preset = min(
                        candidates,
                        key=lambda name: abs(
                            self.FIXED_RESOLUTION_PRESETS[name][0]
                            * self.FIXED_RESOLUTION_PRESETS[name][1]
                            - old_pixels
                        ),
                    )

        if no_scale:
            width, height = source_w, source_h
        else:
            width, height = self.FIXED_RESOLUTION_PRESETS.get(
                effective_preset,
                self.FIXED_RESOLUTION_PRESETS["Landscape 16:9 · 1344x768"],
            )
        if swap_aspect_when_not_image:
            width, height = height, width
        return int(width), int(height), float(width), float(height)


class SayaUpscalePresetModelLoader:
    """Visible final-upscale settings node: target preset plus upscale model."""

    PRESETS = {
        "1080p · FAST · 1920x1080 equivalent": 1920 * 1080,
        "2K · BALANCED · 2560x1440 equivalent": 2560 * 1440,
        "3K · HIGH · 3200x1800 equivalent": 3200 * 1800,
        "4K · MAX · 3840x2160 equivalent": 3840 * 2160,
    }

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, Any]:
        import folder_paths

        models = list(folder_paths.get_filename_list("upscale_models"))
        default_model = models[0] if models else ""

        model_options: tuple[Any, ...]
        if models:
            model_options = (
                models,
                {
                    "default": default_model,
                    "description": "Upscale model used by the final automatic pass.",
                },
            )
        else:
            model_options = (
                [""],
                {
                    "default": "",
                    "description": "No upscale model was found in models/upscale_models.",
                },
            )

        return {
            "required": {
                "target_preset": (
                    list(cls.PRESETS.keys()),
                    {
                        "default": "4K · MAX · 3840x2160 equivalent",
                        "description": (
                            "Pixel budget equivalent. The source aspect ratio is "
                            "preserved and dimensions are rounded to multiples of 16."
                        ),
                    },
                ),
                "upscale_model": model_options,
            },
        }

    RETURN_TYPES = ("UPSCALE_MODEL", "INT", "STRING")
    RETURN_NAMES = ("upscale_model", "target_pixels", "preset_name")
    FUNCTION = "load"
    CATEGORY = "Saya/Scaling"

    def load(self, target_preset: str, upscale_model: str):
        if not upscale_model:
            raise RuntimeError(
                "No upscale model is available. Put one in "
                "ComfyUI/models/upscale_models."
            )

        from comfy_extras.nodes_upscale_model import UpscaleModelLoader

        loaded_model = UpscaleModelLoader().load_model(upscale_model)[0]
        target_pixels = int(self.PRESETS[target_preset])
        return loaded_model, target_pixels, target_preset


class SayaUpscaleTargetCalculator:
    """
    Calculate an aspect-preserving target close to the selected pixel budget.

    The selected 1080p / 2K / 3K / 4K preset represents a standard 16:9
    pixel budget. Landscape, portrait, square, and wide source ratios are
    preserved. Final dimensions are snapped to multiples of 16.
    """

    DIVISOR = 16

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, Any]:
        return {
            "required": {
                "image": ("IMAGE",),
                "target_pixels": (
                    "INT",
                    {
                        "default": 3840 * 2160,
                        "min": 256 * 256,
                        "max": 16384 * 16384,
                        "step": 1,
                    },
                ),
            },
        }

    RETURN_TYPES = ("IMAGE", "INT", "INT", "FLOAT", "FLOAT")
    RETURN_NAMES = (
        "image",
        "target_width",
        "target_height",
        "model_factor",
        "target_megapixels",
    )
    FUNCTION = "calculate"
    CATEGORY = "Saya/Scaling"

    @classmethod
    def _best_target(
        cls,
        source_w: int,
        source_h: int,
        target_pixels: int,
    ) -> tuple[int, int]:
        source_w = max(1, int(source_w))
        source_h = max(1, int(source_h))
        target_pixels = max(1, int(target_pixels))

        ratio = source_w / source_h
        divisor = cls.DIVISOR

        ideal_w = math.sqrt(target_pixels * ratio)
        ideal_h = ideal_w / ratio

        center_w = max(divisor, round(ideal_w / divisor) * divisor)
        center_h = max(divisor, round(ideal_h / divisor) * divisor)

        candidates: set[tuple[int, int]] = set()

        for offset in range(-24, 25):
            width = max(divisor, center_w + offset * divisor)
            height = max(divisor, round((width / ratio) / divisor) * divisor)
            candidates.add((width, height))

            height = max(divisor, center_h + offset * divisor)
            width = max(divisor, round((height * ratio) / divisor) * divisor)
            candidates.add((width, height))

        def score(candidate: tuple[int, int]) -> tuple[float, float, int]:
            width, height = candidate
            candidate_ratio = width / height
            ratio_error = abs(candidate_ratio - ratio) / ratio
            pixel_error = abs((width * height) - target_pixels) / target_pixels
            dimensional_drift = abs(width - ideal_w) + abs(height - ideal_h)

            # Preserve composition first, then match the selected pixel budget.
            combined = ratio_error * 8.0 + pixel_error
            return combined, dimensional_drift, width * height

        return min(candidates, key=score)

    def calculate(self, image, target_pixels: int):
        _, source_h, source_w, _ = image.shape
        source_w = int(source_w)
        source_h = int(source_h)

        target_w, target_h = self._best_target(
            source_w,
            source_h,
            int(target_pixels),
        )

        required_factor = max(
            target_w / max(1, source_w),
            target_h / max(1, source_h),
        )

        # WLSH supports a floating upscale factor. Keep the useful model range;
        # the exact Lanczos fit directly after it lands on the selected target.
        model_factor = min(4.0, max(1.0, required_factor))
        target_megapixels = (target_w * target_h) / 1_000_000.0

        return (
            image,
            int(target_w),
            int(target_h),
            float(round(model_factor, 4)),
            float(target_megapixels),
        )


# Backward-compatible alias for workflows made before preset support.
SayaNear4KTargetCalculator = SayaUpscaleTargetCalculator

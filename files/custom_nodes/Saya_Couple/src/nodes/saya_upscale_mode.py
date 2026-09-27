"""Saya Upscale Mode: neural upscale model OR classic resize, per lane."""

from __future__ import annotations

import math

MODE_MODEL = "UPSCALE MODEL"
MODE_CLASSIC = "CLASSIC"
# Resize used to come back to the requested size after the neural upscale.
MODEL_RESIZE_METHOD = "lanczos"
DEFAULT_MODEL = "RealESRGAN_x4plus_anime_6B.safetensors"

_MODEL_CACHE: dict[tuple[str, float], object] = {}


def classic_methods() -> list[str]:
    """Exactly the methods the native ImageScale node supports."""
    import nodes

    return list(nodes.ImageScale.upscale_methods)


def upscale_model_names() -> list[str]:
    import folder_paths

    return list(folder_paths.get_filename_list("upscale_models"))


def _load_upscale_model(name: str):
    import os

    import folder_paths
    from comfy_extras.nodes_upscale_model import UpscaleModelLoader

    if name not in upscale_model_names():
        raise ValueError(
            f"SayaUpscaleMode: upscale_model_name {name!r} missing from models/upscale_models"
        )
    key = (name, os.path.getmtime(folder_paths.get_full_path_or_raise("upscale_models", name)))
    if key not in _MODEL_CACHE:
        _MODEL_CACHE.clear()
        (_MODEL_CACHE[key],) = UpscaleModelLoader.execute(name)
    return _MODEL_CACHE[key]


def _resize(image, width: int, height: int, method: str):
    if (int(image.shape[2]), int(image.shape[1])) == (width, height):
        return image
    import nodes

    return nodes.ImageScale().upscale(image, method, width, height, "disabled")[0]


def _upscale_with_model(upscale_model, image):
    from comfy_extras.nodes_upscale_model import ImageUpscaleWithModel

    (out,) = ImageUpscaleWithModel.execute(upscale_model, image)
    return out


class SayaUpscaleMode:
    """Resize the IMAGE to input x upscale_by, with a chosen upscale model or classic method."""

    @classmethod
    def INPUT_TYPES(cls):
        models = upscale_model_names()
        default_model = DEFAULT_MODEL if DEFAULT_MODEL in models else (models[0] if models else "")
        return {
            "required": {
                "image": ("IMAGE",),
                "upscale_mode": ([MODE_MODEL, MODE_CLASSIC], {
                    "default": MODE_CLASSIC,
                    "tooltip": "UPSCALE MODEL = applies upscale_model_name (always, even at "
                               "upscale_by 1.0). CLASSIC = resize with classic_method, model ignored.",
                }),
                "upscale_model_name": (models or [""], {
                    "default": default_model,
                    "tooltip": "Model from models/upscale_models. Used only in UPSCALE MODEL mode.",
                }),
                "classic_method": (classic_methods(), {
                    "default": "lanczos",
                    "tooltip": "Resize method. Used only in CLASSIC mode.",
                }),
                "upscale_by": ("FLOAT", {
                    "default": 1.0, "min": 0.25, "max": 4.0, "step": 0.01, "round": 0.01,
                    "tooltip": "Final size = input size x upscale_by (in both modes).",
                }),
            },
        }

    RETURN_TYPES = ("IMAGE",)
    RETURN_NAMES = ("image",)
    FUNCTION = "prepare"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "UPSCALE MODEL: applies the upscale_model_name model (x4 -> then lanczos back to "
        "input x upscale_by, even at 1.0). CLASSIC: resize with classic_method, model ignored."
    )

    def prepare(self, image, upscale_mode, upscale_model_name, classic_method, upscale_by):
        if upscale_mode not in (MODE_MODEL, MODE_CLASSIC):
            raise ValueError(f"SayaUpscaleMode: invalid upscale_mode {upscale_mode!r}")
        try:
            ratio = float(upscale_by)
        except (TypeError, ValueError):
            ratio = math.nan
        if not math.isfinite(ratio) or ratio <= 0.0:
            raise ValueError(f"SayaUpscaleMode: upscale_by must be a number > 0, got {upscale_by!r}")
        _b, height, width, _c = (int(v) for v in image.shape)
        target_w, target_h = max(1, round(width * ratio)), max(1, round(height * ratio))

        if upscale_mode == MODE_CLASSIC:
            if classic_method not in classic_methods():
                raise ValueError(
                    f"SayaUpscaleMode: invalid classic_method {classic_method!r}, "
                    f"expected {classic_methods()}"
                )
            return (_resize(image, target_w, target_h, classic_method),)

        upscaled = _upscale_with_model(_load_upscale_model(upscale_model_name), image)
        return (_resize(upscaled, target_w, target_h, MODEL_RESIZE_METHOD),)

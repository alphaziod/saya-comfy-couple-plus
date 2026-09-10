"""Prompt bundle helpers for isolated image phases."""

from __future__ import annotations

import json
from typing import Any, Self

import torch


class SayaCouplePromptBundlePack:
    """Pack the four external couple prompts into one manifest-safe string."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        return {
            "required": {
                "base_prompt": ("STRING", {"default": "", "multiline": True}),
                "person_1_prompt": ("STRING", {"default": "", "multiline": True}),
                "person_2_prompt": ("STRING", {"default": "", "multiline": True}),
                "negative_prompt": ("STRING", {"default": "", "multiline": True}),
            }
        }

    RETURN_TYPES = ("STRING",)
    RETURN_NAMES = ("prompt_bundle_json",)
    FUNCTION = "pack"
    CATEGORY = "saya/image phases"

    def pack(
        self: Self,
        base_prompt: str,
        person_1_prompt: str,
        person_2_prompt: str,
        negative_prompt: str,
    ) -> tuple[str]:
        payload = {
            "version": 1,
            "base_prompt": str(base_prompt or ""),
            "person_1_prompt": str(person_1_prompt or ""),
            "person_2_prompt": str(person_2_prompt or ""),
            "negative_prompt": str(negative_prompt or ""),
        }
        return (json.dumps(payload, ensure_ascii=False, separators=(",", ":")),)


class SayaCouplePromptBundleUnpack:
    """Restore the four prompts and a combined metadata prompt."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        return {
            "required": {
                "prompt_bundle_json": (
                    "STRING",
                    {"default": "", "multiline": True, "forceInput": True},
                )
            }
        }

    RETURN_TYPES = ("STRING", "STRING", "STRING", "STRING", "STRING")
    RETURN_NAMES = (
        "base_prompt",
        "person_1_prompt",
        "person_2_prompt",
        "negative_prompt",
        "combined_positive_prompt",
    )
    FUNCTION = "unpack"
    CATEGORY = "saya/image phases"

    def unpack(self: Self, prompt_bundle_json: str) -> tuple[str, str, str, str, str]:
        raw = str(prompt_bundle_json or "")
        base = raw
        person_1 = ""
        person_2 = ""
        negative = ""
        try:
            parsed = json.loads(raw)
            if isinstance(parsed, dict):
                base = str(parsed.get("base_prompt", ""))
                person_1 = str(parsed.get("person_1_prompt", ""))
                person_2 = str(parsed.get("person_2_prompt", ""))
                negative = str(parsed.get("negative_prompt", ""))
        except json.JSONDecodeError:
            pass

        combined = ", ".join(
            text.strip()
            for text in (base, person_1, person_2)
            if text and text.strip()
        )
        return base, person_1, person_2, negative, combined


class SayaLatentShapeFromImage:
    """Create a zero latent used only to rebuild couple masks from image dimensions."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        return {"required": {"image": ("IMAGE",)}}

    RETURN_TYPES = ("LATENT",)
    RETURN_NAMES = ("latent_shape",)
    FUNCTION = "build"
    CATEGORY = "saya/image phases"

    def build(self: Self, image: Any) -> tuple[dict[str, Any]]:
        if not isinstance(image, torch.Tensor) or image.ndim != 4:
            raise ValueError(
                "Saya Latent Shape From Image attend une IMAGE [batch, height, width, channels]."
            )
        batch, height, width, _channels = image.shape
        latent_height = max(1, int(height) // 8)
        latent_width = max(1, int(width) // 8)
        samples = torch.zeros(
            (int(batch), 4, latent_height, latent_width),
            dtype=torch.float32,
            device=image.device,
        )
        return ({"samples": samples},)

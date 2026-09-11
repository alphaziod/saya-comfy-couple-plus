"""Prompt bundle helpers for isolated image phases.

The bundle is the disk-safe transport used between image phases.  It only stores
JSON-safe prompt/config data.  Runtime-only objects such as MODEL, CLIP and
CONDITIONING never enter this JSON payload.
"""

from __future__ import annotations

import json
from typing import Any, Self

import torch

COUPLE_CONFIG_TYPE = "SAYA_COUPLE_CONFIG"


def _json_safe(value: Any) -> Any:
    """Return a JSON-safe copy, dropping runtime-only objects.

    SAYA_COUPLE_CONFIG may be enriched elsewhere with runtime tensors/models.
    Those objects must never be serialized into the phase manifest.
    """
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, dict):
        out: dict[str, Any] = {}
        for key, item in value.items():
            safe = _json_safe(item)
            if safe is not _DROP:
                out[str(key)] = safe
        return out
    if isinstance(value, (list, tuple)):
        out = []
        for item in value:
            safe = _json_safe(item)
            if safe is not _DROP:
                out.append(safe)
        return out
    return _DROP


class _Drop:
    pass


_DROP = _Drop()


class SayaCouplePromptBundlePack:
    """Pack original prompts, Naturalize prompts and public couple config.

    Text / config only. The heavy runtime objects travel by other means:
    the prepared Naturalize MODEL/CONDITIONING through ``services.couple_runtime``
    (in-process), and the HiDream CONDITIONING is re-created in Phase 03 from
    these prompt strings (``services.hidream_cache``). Nothing runtime-only is
    serialized into this JSON string.
    """

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        return {
            "required": {
                "base_prompt": ("STRING", {"default": "", "multiline": True}),
                "person_1_prompt": ("STRING", {"default": "", "multiline": True}),
                "person_2_prompt": ("STRING", {"default": "", "multiline": True}),
                "negative_prompt": ("STRING", {"default": "", "multiline": True}),
            },
            "optional": {
                "couple_config": (COUPLE_CONFIG_TYPE,),
                "naturalize_main_prompt": ("STRING", {"default": "", "multiline": True}),
                "naturalize_person_1_prompt": ("STRING", {"default": "", "multiline": True}),
                "naturalize_person_2_prompt": ("STRING", {"default": "", "multiline": True}),
                "naturalize_negative_prompt": ("STRING", {"default": "", "multiline": True}),
            },
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
        couple_config: Any = None,
        naturalize_main_prompt: str = "",
        naturalize_person_1_prompt: str = "",
        naturalize_person_2_prompt: str = "",
        naturalize_negative_prompt: str = "",
    ) -> tuple[str]:
        safe_config = _json_safe(couple_config if isinstance(couple_config, dict) else {})
        if safe_config is _DROP:
            safe_config = {}

        payload = {
            "version": 3,
            "base_prompt": str(base_prompt or ""),
            "person_1_prompt": str(person_1_prompt or ""),
            "person_2_prompt": str(person_2_prompt or ""),
            "negative_prompt": str(negative_prompt or ""),
            "couple_config": safe_config,
            "naturalize_main_prompt": str(naturalize_main_prompt or ""),
            "naturalize_person_1_prompt": str(naturalize_person_1_prompt or ""),
            "naturalize_person_2_prompt": str(naturalize_person_2_prompt or ""),
            "naturalize_negative_prompt": str(naturalize_negative_prompt or ""),
        }
        return (json.dumps(payload, ensure_ascii=False, separators=(",", ":")),)


class SayaCouplePromptBundleUnpack:
    """Restore the phase bundle.

    Output order is append-only and matches the workflow family:
      0..4 original prompts/combined,
      5 public couple config,
      6..9 final Naturalize prompt strings.
    """

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

    RETURN_TYPES = (
        "STRING", "STRING", "STRING", "STRING", "STRING",
        COUPLE_CONFIG_TYPE,
        "STRING", "STRING", "STRING", "STRING",
    )
    RETURN_NAMES = (
        "base_prompt",
        "person_1_prompt",
        "person_2_prompt",
        "negative_prompt",
        "combined_positive_prompt",
        "couple_config",
        "naturalize_main_prompt",
        "naturalize_person_1_prompt",
        "naturalize_person_2_prompt",
        "naturalize_negative_prompt",
    )
    FUNCTION = "unpack"
    CATEGORY = "saya/image phases"

    def unpack(self: Self, prompt_bundle_json: str) -> tuple[Any, ...]:
        raw = str(prompt_bundle_json or "")
        base = raw
        person_1 = ""
        person_2 = ""
        negative = ""
        couple_config: dict[str, Any] = {}
        naturalize_main = ""
        naturalize_p1 = ""
        naturalize_p2 = ""
        naturalize_negative = ""

        try:
            parsed = json.loads(raw)
            if isinstance(parsed, dict):
                base = str(parsed.get("base_prompt", ""))
                person_1 = str(parsed.get("person_1_prompt", ""))
                person_2 = str(parsed.get("person_2_prompt", ""))
                negative = str(parsed.get("negative_prompt", ""))
                candidate_config = parsed.get("couple_config", {})
                couple_config = dict(candidate_config) if isinstance(candidate_config, dict) else {}
                naturalize_main = str(parsed.get("naturalize_main_prompt", ""))
                naturalize_p1 = str(parsed.get("naturalize_person_1_prompt", ""))
                naturalize_p2 = str(parsed.get("naturalize_person_2_prompt", ""))
                naturalize_negative = str(parsed.get("naturalize_negative_prompt", ""))
        except json.JSONDecodeError:
            pass

        combined = ", ".join(
            text.strip()
            for text in (base, person_1, person_2)
            if text and text.strip()
        )
        return (
            base,
            person_1,
            person_2,
            negative,
            combined,
            couple_config,
            naturalize_main,
            naturalize_p1,
            naturalize_p2,
            naturalize_negative,
        )


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

"""Legacy data-only Couple imprint (schema ``saya.couple.imprint``, version 1).

The imprint describes everything needed to rebuild the coupling (MAIN / P1 /
P2 / NEGATIVE + split parameters) in another phase, without ever serializing a
runtime object (MODEL, CLIP, CONDITIONING, LATENT, IMAGE, MASK). The
``imprint_json`` string is an ordinary STRING value, so it can be written to
PNG metadata (``DaSiWa_MetadataImageSaver.extra_metadata``) or any manifest and
read back by ``SayaCoupleImprintUnpack``.

Schema rules:

* ``person_2`` is ABSENT from the JSON when P2 is absent. It is never replaced
  by MAIN, and an empty string is never read as "present".
* Canonical JSON: ``json.dumps(..., sort_keys=True, separators=(",", ":"),
  ensure_ascii=False)`` so pack -> unpack -> pack round-trips deterministically.
* The whole payload is str/int/float/bool, so it can be checked without ComfyUI.
"""

from __future__ import annotations

import json
from typing import Any

SCHEMA_NAME = "saya.couple.imprint"
SCHEMA_VERSION = 1

_DIRECTIONS = ("vertical", "horizontal")

#: Fields used by the couple; no runtime object.
_DATA_FIELDS = (
    "main_prompt",
    "person_1_prompt",
    "person_2_prompt",
    "negative_prompt",
    "direction",
    "split",
    "blur",
    "strength_1",
    "strength_2",
)


class SayaCoupleImprintError(ValueError):
    """Raised for any malformed imprint payload or field value."""


def _require_text(value: Any, field: str) -> str:
    if not isinstance(value, str):
        raise SayaCoupleImprintError(f"{field}: expected str, got {type(value).__name__}")
    return value


def build_imprint(
    *,
    main_prompt: str,
    person_1_prompt: str,
    negative_prompt: str,
    direction: str,
    split: int,
    blur: float,
    strength_1: float = 1.0,
    strength_2: float = 1.0,
    person_2_prompt: str | None = None,
) -> dict[str, Any]:
    """Validate inputs and return the canonical DATA-ONLY imprint dict."""
    direction = _require_text(direction, "direction").lower()
    if direction not in _DIRECTIONS:
        raise SayaCoupleImprintError(
            f"direction: invalid {direction!r}, expected one of {_DIRECTIONS}"
        )
    split = int(split)
    if not 0 <= split <= 100:
        raise SayaCoupleImprintError(f"split: {split} hors plage 0..100")
    blur = float(blur)
    if blur < 0.0:
        raise SayaCoupleImprintError(f"blur: {blur} negatif")

    imprint: dict[str, Any] = {
        "schema": SCHEMA_NAME,
        "version": SCHEMA_VERSION,
        "main_prompt": _require_text(main_prompt, "main_prompt"),
        "person_1_prompt": _require_text(person_1_prompt, "person_1_prompt"),
        "negative_prompt": _require_text(negative_prompt, "negative_prompt"),
        "direction": direction,
        "split": split,
        "blur": blur,
        "strength_1": float(strength_1),
        "strength_2": float(strength_2),
    }
    # An absent P2 stays absent: no key, no empty string, no fallback to MAIN.
    # A blank string means "absent".
    if person_2_prompt is not None and _require_text(person_2_prompt, "person_2_prompt").strip():
        imprint["person_2_prompt"] = person_2_prompt
    return imprint


def canonical_imprint_json(imprint: dict[str, Any]) -> str:
    """Serialize deterministically (sort_keys, no whitespace)."""
    return json.dumps(imprint, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def parse_imprint_json(payload: str) -> dict[str, Any]:
    """Parse and strictly validate an imprint JSON string."""
    try:
        data = json.loads(payload)
    except json.JSONDecodeError as error:
        raise SayaCoupleImprintError(f"imprint: invalid JSON: {error}") from None
    if not isinstance(data, dict):
        raise SayaCoupleImprintError("imprint: the root must be an object")
    if data.get("schema") != SCHEMA_NAME:
        raise SayaCoupleImprintError(
            f"imprint: unknown schema {data.get('schema')!r}, expected {SCHEMA_NAME!r}"
        )
    if data.get("version") != SCHEMA_VERSION:
        raise SayaCoupleImprintError(
            f"imprint: unsupported version {data.get('version')!r} "
            f"(this build reads version {SCHEMA_VERSION})"
        )
    missing = [key for key in ("main_prompt", "person_1_prompt", "negative_prompt", "direction", "split", "blur") if key not in data]
    if missing:
        raise SayaCoupleImprintError(f"imprint: missing required fields {missing}")
    # Re-validation complete via build_imprint (idempotente sur payload valide).
    return build_imprint(
        main_prompt=data["main_prompt"],
        person_1_prompt=data["person_1_prompt"],
        negative_prompt=data["negative_prompt"],
        direction=data["direction"],
        split=data["split"],
        blur=data["blur"],
        strength_1=data.get("strength_1", 1.0),
        strength_2=data.get("strength_2", 1.0),
        person_2_prompt=data.get("person_2_prompt"),
    )


class SayaCoupleImprintPack:
    """Pack a data-only Couple imprint from primitives."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "main_prompt": ("STRING", {"default": "", "multiline": True}),
                "person_1_prompt": ("STRING", {"default": "", "multiline": True}),
                "negative_prompt": ("STRING", {"default": "", "multiline": True}),
                "direction": (list(_DIRECTIONS), {"default": "vertical"}),
                "split": ("INT", {"default": 50, "min": 0, "max": 100, "step": 1}),
                "blur": ("FLOAT", {"default": 0.0, "min": 0.0, "step": 0.1}),
                "strength_1": ("FLOAT", {"default": 1.0, "min": 0.0, "max": 2.0, "step": 0.05}),
                "strength_2": ("FLOAT", {"default": 1.0, "min": 0.0, "max": 2.0, "step": 0.05}),
            },
            "optional": {
                "person_2_prompt": ("STRING", {"default": "", "multiline": True, "forceInput": True}),
            },
        }

    RETURN_TYPES = ("SAYA_IMPRINT", "STRING")
    RETURN_NAMES = ("imprint", "imprint_json")
    FUNCTION = "pack"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Data-only Couple imprint (schema saya.couple.imprint v1). "
        "A blank/absent P2 stays absent from the canonical JSON."
    )

    def pack(
        self,
        main_prompt: str,
        person_1_prompt: str,
        negative_prompt: str,
        direction: str,
        split: int,
        blur: float,
        strength_1: float = 1.0,
        strength_2: float = 1.0,
        person_2_prompt: str | None = None,
    ) -> tuple[dict[str, Any], str]:
        imprint = build_imprint(
            main_prompt=main_prompt,
            person_1_prompt=person_1_prompt,
            negative_prompt=negative_prompt,
            direction=direction,
            split=split,
            blur=blur,
            strength_1=strength_1,
            strength_2=strength_2,
            person_2_prompt=person_2_prompt,
        )
        return imprint, canonical_imprint_json(imprint)


class SayaCoupleImprintUnpack:
    """Unpack the data-only fields of a Couple imprint."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "imprint": ("SAYA_IMPRINT",),
            },
            "optional": {
                "imprint_json": ("STRING", {"default": "", "multiline": True, "forceInput": True}),
            },
        }

    RETURN_TYPES = (
        "STRING", "STRING", "STRING", "STRING",
        "INT", "FLOAT", "FLOAT", "FLOAT", "INT",
    )
    RETURN_NAMES = (
        "main_prompt", "person_1_prompt", "person_2_prompt", "negative_prompt",
        "split", "blur", "strength_1", "strength_2", "person_2_present",
    )
    FUNCTION = "unpack"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Unpacks the imprint. person_2_prompt is \"\" when P2 is absent and "
        "person_2_present is 0: \"\" is never read as present."
    )

    def unpack(
        self,
        imprint: dict[str, Any],
        imprint_json: str | None = None,
    ) -> tuple[str, str, str, str, int, float, float, float, int]:
        # The runtime imprint is authoritative; the JSON string, when given,
        # must be the canonical serialization of the same payload.
        if imprint_json:
            reparsed = parse_imprint_json(imprint_json)
            if canonical_imprint_json(reparsed) != canonical_imprint_json(imprint):
                raise SayaCoupleImprintError(
                    "imprint: the given JSON does not match the runtime imprint"
                )
        else:
            imprint = parse_imprint_json(canonical_imprint_json(imprint))
        p2_present = 1 if "person_2_prompt" in imprint else 0
        return (
            imprint["main_prompt"],
            imprint["person_1_prompt"],
            imprint.get("person_2_prompt", ""),
            imprint["negative_prompt"],
            int(imprint["split"]),
            float(imprint["blur"]),
            float(imprint["strength_1"]),
            float(imprint["strength_2"]),
            p2_present,
        )


NODE_CLASS_MAPPINGS = {
    "SayaCoupleImprintPack": SayaCoupleImprintPack,
    "SayaCoupleImprintUnpack": SayaCoupleImprintUnpack,
}

NODE_DISPLAY_NAME_MAPPINGS = {
    "SayaCoupleImprintPack": "Saya Couple Imprint · Pack (DATA-ONLY v1)",
    "SayaCoupleImprintUnpack": "Saya Couple Imprint · Unpack (rebuild)",
}

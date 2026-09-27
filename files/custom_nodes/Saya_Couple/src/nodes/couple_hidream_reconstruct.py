"""SayaCoupleHiDreamReconstruct — Couple Phase 3 (HiDream) materialization.

Resolved v2 imprint + Quad CLIP + sampling LATENT -> HiDream-native regional
conditionings + pure-geometry masks, for ``ClownRegionalConditioning3``
(RES4LYF) wired in the graph.

Contract:

* ``main_plus_person``: region text = [trigger, MAIN, person] (comma-joined);
* ``add_global_main=false``: no separate MAIN region — MAIN only appears
  inside each region's composition and in ``conditioning_unmasked``;
* ``on_missing_role=use_base``: when P2 is absent, ``conditioning_unmasked``
  (= BASE text + trigger) covers P1's complement (node ``3`` computes that
  complement); mask B is then EMPTY but stays a valid tensor — B and C both
  empty would leave tokens with no region (runtime: noise);
* the HiDream trigger is prefixed to positives ONLY (never to NEG); it is
  already empty when the LoRA is inactive (``SayaHiDreamLoraSettings``);
* geometry = pure binary raster (feather 0, floor 0) at the LATENT grid
  (``latent * 8``): RES4LYF resamples the masks to the real latent's patch
  grid, never the IMAGE (VAE crop); the PPM weights
  (``person_weight``/``strengths``) are NOT applied — in ``boolean`` mode any
  mask > 0 is a region, amplitude has no meaning here.

"Cache-first" encoding: the 4 conditionings are cached on disk
(``output/conditionings/hidream``, key = identity of the 4 CLIP files + exact
text). The ``clip`` input is LAZY: if everything is cached, the Quad CLIP
loader is never even run (no ~15 GB load); otherwise it encodes, releases the
encoder from the GPU right away (HiDream needs the VRAM), and writes the
cache.

The engine identity (MODEL/Quad CLIP/VAE HiDream) is never compared against
the imprint's Phase 1 identities; the imprint itself is strictly validated.
"""

from __future__ import annotations

from typing import Any, Callable

import torch

from . import conditioning_cache as cache
from .couple_imprint_v2 import SayaCoupleImprintError, validate_imprint_v2
from .region_masks import SayaMaskError, region_mask_rect, regions_from_geometry

#: HiDream VAE factor: pixel = latent * 8.
LATENT_SCALE = 8

FAMILY = "hidream"


class SayaCoupleHiDreamReconstructError(ValueError):
    """Hard failure of the Phase 3 reconstruction (never a silent degradation)."""


def _default_encode_text(clip: Any, text: str) -> Any:
    """Encode a prompt with the Quad CLIP (core CLIPTextEncode)."""
    from nodes import CLIPTextEncode  # type: ignore

    return CLIPTextEncode().encode(clip, text)[0]


#: Test injection point (the runtime never touches this).
_encode_text: Callable[[Any, str], Any] = _default_encode_text


def _join_prompt(*parts: str) -> str:
    return ", ".join(part.strip() for part in parts if part and part.strip())


def compose_prompts(prompts: dict[str, str], trigger: str) -> dict[str, str | None]:
    """Positive texts per role. ``None`` means the role is absent (P2).

    An empty ``trigger`` means the LoRA is inactive (nothing is prefixed).
    """
    trigger = trigger.strip()
    main = prompts["main"]
    person_2 = prompts.get("person_2")
    return {
        "a": _join_prompt(trigger, main, prompts["person_1"]),
        "b": _join_prompt(trigger, main, person_2) if person_2 is not None else _join_prompt(trigger, main),
        "unmasked": _join_prompt(trigger, main),
        "negative": prompts["negative"],
    }


def pure_geometry_masks(
    geometry: dict[str, Any], height: int, width: int
) -> tuple[torch.Tensor, torch.Tensor]:
    """Binary (1,H,W) P1 / P2 masks — P2 absent means an empty mask (never fabricated)."""
    region_1, region_2 = regions_from_geometry(geometry)
    unit = geometry["feather_unit"]
    mask_a = region_mask_rect(region_1, height, width, 0.0, unit, 0.0)
    if region_2 is None:
        mask_b = torch.zeros_like(mask_a)
    else:
        mask_b = region_mask_rect(region_2, height, width, 0.0, unit, 0.0)
    return mask_a.unsqueeze(0), mask_b.unsqueeze(0)


class SayaCoupleHiDreamReconstruct:
    """v2 imprint -> A/B/unmasked + NEG conditionings + A/B masks (Phase 3 HiDream)."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "imprint": ("SAYA_IMPRINT", {"forceInput": True}),
                "clip": ("CLIP", {
                    "lazy": True,
                    "tooltip": "HiDream Quad CLIP (conditioning_llama3 required). Lazy: "
                               "only requested when the cache is incomplete."}),
                "latent": ("LATENT", {
                    "tooltip": "The LATENT that is actually sampled: fixes the mask grid "
                               "(latent x 8), never the IMAGE dimensions."}),
                "clip_identity": ("STRING", {
                    "forceInput": True,
                    "tooltip": "Names of the 4 Quad CLIP files (disk cache key). Must change "
                               "whenever a CLIP file changes."}),
            },
            "optional": {
                "hidream_trigger": ("STRING", {
                    "default": "", "multiline": False, "forceInput": True,
                    "tooltip": "HiDream LoRA trigger (empty = LoRA inactive). Prefixed to "
                               "positives only, never to the negative."}),
                "solo": ("BOOLEAN", {
                    "default": False, "forceInput": True,
                    "tooltip": "Couple Mode OFF: MAIN + PERSON 1 globally, no Couple masks/regions."}),
            },
        }

    RETURN_TYPES = ("CONDITIONING", "CONDITIONING", "CONDITIONING", "CONDITIONING", "MASK", "MASK", "BOOLEAN")
    RETURN_NAMES = ("conditioning_a", "conditioning_b", "conditioning_unmasked", "negative",
                    "mask_a", "mask_b", "regional_enabled")
    FUNCTION = "reconstruct"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Couple HiDream Phase 3: MAIN/P1/P2/NEG from the v2 imprint re-encoded "
        "with the Quad CLIP (trigger on positives), pure geometry masks at the "
        "latent grid. A -> conditioning_a/mask_a, B -> conditioning_b/mask_b, "
        "complement -> conditioning_unmasked (ClownRegionalConditioning3). "
        "Couple Mode OFF: MAIN + PERSON 1 global, empty Couple masks, "
        "regional_enabled=false."
    )

    @staticmethod
    def _prepare(
        imprint: dict[str, Any], hidream_trigger: str, solo: bool = False
    ) -> tuple[dict[str, Any], dict[str, str | None]]:
        """Valid imprint + exact texts required by Couple or Solo mode."""
        try:
            data = validate_imprint_v2(imprint)
        except SayaCoupleImprintError as error:
            raise SayaCoupleHiDreamReconstructError(f"imprint: {error}") from error
        couple = data["couple_imprint"]
        prompts = couple["prompts"]
        if solo:
            # True SOLO means MAIN + PERSON 1 globally. Branch before
            # compose_prompts so the solo path never even reads person_2 —
            # Couple regionalization does not participate in Phase 3 at all.
            global_positive = _join_prompt(hidream_trigger or "", prompts["main"], prompts["person_1"])
            if not global_positive:
                raise SayaCoupleHiDreamReconstructError("MAIN and PERSON_1 are both empty: no text for region A")
            texts = {
                "a": global_positive,
                "b": global_positive,
                "unmasked": global_positive,
                "negative": prompts["negative"],
            }
            return couple, texts
        texts = compose_prompts(prompts, hidream_trigger or "")
        if not texts["a"]:
            raise SayaCoupleHiDreamReconstructError("MAIN and PERSON_1 are both empty: no text for region A")
        return couple, texts

    def check_lazy_status(self, imprint, clip, latent, clip_identity, hidream_trigger="", solo=False):
        """Load the Quad CLIP only when the selected mode has a cache miss."""
        _couple, texts = self._prepare(imprint, hidream_trigger, bool(solo))
        unique_texts = tuple(dict.fromkeys(texts.values()))
        cached = all(cache.load(FAMILY, cache.cache_key(clip_identity, text)) is not None for text in unique_texts)
        return [] if cached or clip is not None else ["clip"]

    def reconstruct(
        self,
        imprint: dict[str, Any],
        clip: Any,
        latent: dict[str, Any],
        clip_identity: str,
        hidream_trigger: str = "",
        solo: bool = False,
    ) -> tuple[Any, Any, Any, Any, torch.Tensor, torch.Tensor, bool]:
        if not clip_identity or not clip_identity.strip():
            raise SayaCoupleHiDreamReconstructError("clip_identity empty: cannot build a cache key (names of the 4 CLIP files)")
        couple, texts = self._prepare(imprint, hidream_trigger, bool(solo))

        samples = latent.get("samples") if isinstance(latent, dict) else None
        if samples is None or samples.ndim != 4:
            raise SayaCoupleHiDreamReconstructError(
                f"latent: expected LATENT [B,C,h,w], got {getattr(samples, 'shape', None)!r}"
            )
        height, width = int(samples.shape[-2]) * LATENT_SCALE, int(samples.shape[-1]) * LATENT_SCALE
        try:
            mask_a, mask_b = pure_geometry_masks(couple["geometry"], height, width)
        except SayaMaskError as error:
            raise SayaCoupleHiDreamReconstructError(f"geometry: {error}") from error
        if solo:
            mask_a = torch.zeros_like(mask_a)
            mask_b = torch.zeros_like(mask_b)

        keys = {role: cache.cache_key(clip_identity, text) for role, text in texts.items()}
        encoded = {role: cache.load(FAMILY, key) for role, key in keys.items()}
        missing = [role for role, conditioning in encoded.items() if conditioning is None]
        if missing:
            if clip is None:
                raise SayaCoupleHiDreamReconstructError("cache incomplete and no Quad CLIP connected")
            encoded_by_key: dict[str, Any] = {}
            for role in missing:
                key = keys[role]
                if key in encoded_by_key:
                    encoded[role] = encoded_by_key[key]
                    continue
                encoded[role] = _encode_text(clip, texts[role])
                self._require_llama3(role, encoded[role])
                encoded_by_key[key] = encoded[role]
                cache.save(FAMILY, key, encoded[role])
            cache.release_clip(clip)
        for role, conditioning in encoded.items():
            self._require_llama3(role, conditioning)
        return (
            encoded["a"], encoded["b"], encoded["unmasked"], encoded["negative"],
            mask_a, mask_b, not bool(solo),
        )

    @staticmethod
    def _require_llama3(role: str, conditioning: Any) -> None:
        if "conditioning_llama3" not in conditioning[0][1]:
            raise SayaCoupleHiDreamReconstructError(f"{role}: conditioning_llama3 missing — connect the HiDream Quad CLIP")

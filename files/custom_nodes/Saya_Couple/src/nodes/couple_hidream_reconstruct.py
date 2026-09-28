"""SayaCoupleHiDreamReconstruct — Couple Phase 3 (HiDream) materialization.

Resolved v2 imprint + Quad CLIP + sampling LATENT -> HiDream-native regional
conditionings + pure-geometry masks, for ``ClownRegionalConditioning_AB``
(RES4LYF) wired in the graph.

Contract:

* Couple: region text = [trigger, MAIN, person] (comma-joined), exactly two
  regions that partition the frame. RES4LYF's HiDream text mask is a parity
  checkerboard, which isolates regions only when there are two (with a third
  region, P1 text tokens attend P2 text tokens);
* P2 absent: region B is trigger + MAIN on P1's complement;
* Solo: one global conditioning (``conditioning_solo`` = trigger + MAIN + P1),
  no region conditioning, no mask, ``regional_enabled=false``;
* the HiDream trigger is prefixed to positives only (never to NEG); it is
  already empty when the LoRA is inactive (``SayaHiDreamLoraSettings``);
* geometry = pure binary raster (feather 0, floor 0) at the LATENT grid
  (``latent * 8``): RES4LYF resamples the masks to the real latent's patch
  grid, never the IMAGE (VAE crop). The PPM weights (``person_weight`` /
  ``strengths``) are not applied: in ``boolean`` mode any mask > 0 is a
  region, amplitude has no meaning here.

Cache-first encoding: conditionings are cached on disk
(``output/conditionings/hidream``, key = identity of the 4 CLIP files + exact
text). The ``clip`` input is lazy: if everything is cached, the Quad CLIP
loader never runs (no ~15 GB load); otherwise it encodes, releases the encoder
from the GPU right away (HiDream needs the VRAM), and writes the cache.

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


def compose_prompts(prompts: dict[str, str], trigger: str) -> dict[str, str]:
    """Couple texts per role: region A (P1), region B (P2, or MAIN alone when P2 is absent), NEG.

    An empty ``trigger`` means the LoRA is inactive (nothing is prefixed).
    """
    trigger = trigger.strip()
    main = prompts["main"]
    return {
        "a": _join_prompt(trigger, main, prompts["person_1"]),
        "b": _join_prompt(trigger, main, prompts.get("person_2") or ""),
        "negative": prompts["negative"],
    }


def pure_geometry_masks(
    geometry: dict[str, Any], height: int, width: int
) -> tuple[torch.Tensor, torch.Tensor]:
    """Binary (1,H,W) P1 / P2 masks that exactly partition the frame.

    P2 absent: B is P1's complement (it carries trigger + MAIN).
    """
    region_1, region_2 = regions_from_geometry(geometry)
    unit = geometry["feather_unit"]
    mask_a = region_mask_rect(region_1, height, width, 0.0, unit, 0.0)
    if region_2 is None:
        mask_b = 1.0 - mask_a
    else:
        mask_b = region_mask_rect(region_2, height, width, 0.0, unit, 0.0)
    if not bool((mask_a + mask_b == 1.0).all()):
        # A pixel outside both regions would attend no token at all in the
        # two-region HiDream mask; an overlap would see both persons.
        raise SayaMaskError("P1 and P2 regions must partition the frame (gap or overlap found)")
    return mask_a.unsqueeze(0), mask_b.unsqueeze(0)


class SayaCoupleHiDreamReconstruct:
    """v2 imprint -> P1/P2 region conditionings + masks (Couple) or one global conditioning (Solo), + NEG."""

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
                    "tooltip": "Couple Mode OFF: MAIN + PERSON 1 as one global conditioning "
                               "(conditioning_solo); region outputs are None."}),
            },
        }

    RETURN_TYPES = ("CONDITIONING", "CONDITIONING", "CONDITIONING", "CONDITIONING", "MASK", "MASK", "BOOLEAN")
    RETURN_NAMES = ("conditioning_a", "conditioning_b", "conditioning_solo", "negative",
                    "mask_a", "mask_b", "regional_enabled")
    FUNCTION = "reconstruct"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "HiDream Phase 3 from the v2 imprint, re-encoded with the Quad CLIP (trigger on "
        "positives). Couple: conditioning_a/mask_a (P1) and conditioning_b/mask_b (P2) "
        "for ClownRegionalConditioning_AB, masks partitioning the frame at the latent "
        "grid, regional_enabled=true. Solo: conditioning_solo = MAIN + PERSON 1, region "
        "outputs None, regional_enabled=false."
    )

    @staticmethod
    def _prepare(
        imprint: dict[str, Any], hidream_trigger: str, solo: bool = False
    ) -> tuple[dict[str, Any], dict[str, str]]:
        """Valid imprint + exact texts required by Couple or Solo mode."""
        try:
            data = validate_imprint_v2(imprint)
        except SayaCoupleImprintError as error:
            raise SayaCoupleHiDreamReconstructError(f"imprint: {error}") from error
        couple = data["couple_imprint"]
        prompts = couple["prompts"]
        if solo:
            # Solo never reads person_2: one global MAIN + PERSON 1 text.
            solo_positive = _join_prompt(hidream_trigger or "", prompts["main"], prompts["person_1"])
            if not solo_positive:
                raise SayaCoupleHiDreamReconstructError("MAIN and PERSON_1 are both empty: no Solo text")
            return couple, {"solo": solo_positive, "negative": prompts["negative"]}
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
    ) -> tuple[Any, Any, Any, Any, torch.Tensor | None, torch.Tensor | None, bool]:
        if not clip_identity or not clip_identity.strip():
            raise SayaCoupleHiDreamReconstructError("clip_identity empty: cannot build a cache key (names of the 4 CLIP files)")
        couple, texts = self._prepare(imprint, hidream_trigger, bool(solo))

        if not solo:
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

        keys = {role: cache.cache_key(clip_identity, text) for role, text in texts.items()}
        encoded = {role: cache.load(FAMILY, key) for role, key in keys.items()}
        missing = [role for role, conditioning in encoded.items() if conditioning is None]
        if missing:
            if clip is None:
                raise SayaCoupleHiDreamReconstructError("cache incomplete and no Quad CLIP connected")
            encoded_by_key: dict[str, Any] = {}
            for role in missing:
                key = keys[role]
                if key not in encoded_by_key:
                    encoded_by_key[key] = _encode_text(clip, texts[role])
                    self._require_llama3(role, encoded_by_key[key])
                    cache.save(FAMILY, key, encoded_by_key[key])
                encoded[role] = encoded_by_key[key]
            cache.release_clip(clip)
        for role, conditioning in encoded.items():
            self._require_llama3(role, conditioning)
        if solo:
            # Plain global refine: nothing here can reach the regional nodes.
            return (None, None, encoded["solo"], encoded["negative"], None, None, False)
        return (encoded["a"], encoded["b"], None, encoded["negative"], mask_a, mask_b, True)

    @staticmethod
    def _require_llama3(role: str, conditioning: Any) -> None:
        if "conditioning_llama3" not in conditioning[0][1]:
            raise SayaCoupleHiDreamReconstructError(f"{role}: conditioning_llama3 missing — connect the HiDream Quad CLIP")

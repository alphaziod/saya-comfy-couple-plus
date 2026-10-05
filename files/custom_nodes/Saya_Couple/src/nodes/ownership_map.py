"""Sampler 1 ownership map -> imprint + MODEL_2, so every later pass reads the same P1/P2 split.

Phase 1 Sampler 1 (``SayaMultiCouple`` ownership = dynamic) decides, step by step, which pixels
belong to P1, to P2, to the background or to nobody yet (engine state in
``src/engine/dual_attention.py::_SAYA_OWNERSHIP_STATE``). This node runs right after Sampler 1
(its LATENT input orders it), reads that final state once and:

* writes it into the imprint as ``couple_imprint.ownership_map`` -- read by
  ``region_masks._rasterize_regions`` (Hires Fix, USDU, Detailers, Phase 6) and by
  ``couple_hidream_reconstruct.pure_geometry_masks`` (Phase 3);
* rebuilds MODEL_2 (Sampler 2) from ``COUPLE_RECIPE`` on the masks of that same imprint.

No map (static ownership, Solo, no confirmed step, batch > 1): the imprint is returned
unchanged and MODEL_2 is the static patch -- the split is never guessed.
"""

from __future__ import annotations

from typing import Any, Callable

from .couple_imprint_v2 import (
    OWNERSHIP_BACKGROUND,
    OWNERSHIP_P1,
    OWNERSHIP_P2,
    OWNERSHIP_STATIC,
    build_ownership_map,
    canonical_imprint_json,
    parse_imprint_json,
    validate_imprint_v2,
)
from .region_masks import attention_weights, derive_background_mask, derive_raw_region_masks
from .saya_multi_couple import apply_multimask_couple


def _default_engine_state() -> dict[Any, dict[str, Any]]:
    from ..engine import dual_attention
    return dual_attention._SAYA_OWNERSHIP_STATE


#: Test injection point (the runtime never touches this).
_engine_state: Callable[[], dict[Any, dict[str, Any]]] = _default_engine_state

#: Engine ``active`` value -> map cell (-1 = no confirmed owner, the static split stays).
_CELL = {-1: OWNERSHIP_STATIC, 0: OWNERSHIP_P1, 1: OWNERSHIP_P2, 2: OWNERSHIP_BACKGROUND}

#: Last capture: the engine state is consumed once, so a re-run on a cached Sampler 1 LATENT
#: (same tensor object) reuses the map it produced instead of reading another sampling's state.
_LAST: dict[str, Any] = {}


def _recipe_state(states: dict[Any, dict[str, Any]], recipe: dict[str, Any] | None) -> dict[str, Any] | None:
    """The engine state of the recipe's MODEL_1. The engine keys its state by the P1 conditioning tensor
    (``id(dual["p1"])``), the very object SayaMultiCouple put in both the MODEL_1 payload and the recipe,
    so a recipe selects its own sampling's state and never another model's. Without a recipe the only
    state there is (if there is exactly one) is taken."""
    if recipe is None:
        return next(iter(states.values())) if len(states) == 1 else None
    return states.get(id(recipe["pos_1"][0][0]))


def map_from_state(state: dict[str, Any] | None) -> tuple[dict[str, Any] | None, str]:
    """Engine state -> (ownership_map, reason). The map is None when nothing can be transported."""
    if state is None or state.get("active") is None or state.get("map") is None:
        return None, "no confirmed dynamic step in Sampler 1"
    labels = state["active"].clone()
    block = state.get("block")
    if block is not None and block.shape == labels.shape:
        # zone_fallback: undecided pixels the engine itself gave to a person by block majority.
        labels[(labels < 0) & (block >= 0)] = block[(labels < 0) & (block >= 0)]
    if labels.shape[0] != 1:
        return None, f"batch {labels.shape[0]}: one map per imprint, the static split is kept"
    height, width = labels.shape[1:]
    rows = ["".join(_CELL[int(value)] for value in row) for row in labels[0].tolist()]
    return build_ownership_map(grid=[int(height), int(width)], rows=rows), "captured"


def _same_aspect(grid: list[int], latent_shape: Any) -> bool:
    height, width = int(latent_shape[-2]), int(latent_shape[-1])
    return abs(grid[0] * width - grid[1] * height) <= 0.1 * grid[1] * height


class SayaOwnershipMapCapture:
    """After Phase 1 Sampler 1: Sampler 1 ownership map -> imprint_json + MODEL_2 for Sampler 2."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "samples": ("LATENT", {"tooltip": "Sampler 1 output: orders this node right after Sampler 1."}),
                "imprint_json": ("STRING", {"forceInput": True, "multiline": True}),
            },
            "optional": {
                "couple_recipe": ("SAYA_COUPLE_RECIPE", {"tooltip": "SayaMultiCouple COUPLE_RECIPE: MODEL_2 is rebuilt on the map."}),
            },
        }

    RETURN_TYPES = ("STRING", "MODEL", "STRING")
    RETURN_NAMES = ("imprint_json", "MODEL_2_PATCHED", "report")
    FUNCTION = "capture"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Reads the Sampler 1 dynamic P1/P2 ownership once and writes it into the imprint "
        "(ownership_map), so Sampler 2, Hires/USDU, HiDream, Detailers and Phase 6 all split P1/P2 "
        "the same way. No map -> imprint unchanged and static MODEL_2."
    )

    def capture(self, samples: dict[str, Any], imprint_json: str, couple_recipe: dict[str, Any] | None = None):
        data = parse_imprint_json(imprint_json)
        couple = data["couple_imprint"]
        couple.pop("ownership_map", None)
        latent = samples["samples"]
        if _LAST.get("samples") is latent:
            ownership, reason = _LAST["map"], "reused (Sampler 1 cached)"
        else:
            states = _engine_state()
            state = _recipe_state(states, couple_recipe)
            foreign = state is None and bool(states)
            states.clear()
            ownership, reason = map_from_state(state)
            if foreign:
                reason = "the engine state belongs to another MODEL_1 (or several), not to this recipe's Sampler 1"
            if ownership is not None and not _same_aspect(ownership["grid"], latent.shape):
                ownership, reason = None, f"map grid {ownership['grid']} does not match the latent {list(latent.shape[-2:])}"
            _LAST.clear()
            _LAST.update(samples=latent, map=ownership)
        # Checked on every run, cached or not: the imprint may have turned Solo / static since.
        if couple_recipe is not None and not couple_recipe.get("dynamic"):
            ownership, reason = None, "static ownership"
        elif "person_2" not in couple["prompts"]:
            ownership, reason = None, "Solo"
        if ownership is not None:
            couple["ownership_map"] = ownership
            validate_imprint_v2(data, context="imprint(ownership_map)")
            cells = "".join(ownership["rows"])
            share = {name: round(cells.count(code) / len(cells), 3) for name, code in
                     (("p1", OWNERSHIP_P1), ("p2", OWNERSHIP_P2), ("background", OWNERSHIP_BACKGROUND), ("static", OWNERSHIP_STATIC))}
            report = f"ownership_map {reason}: grid {ownership['grid']} {share}"
        else:
            report = f"no ownership_map ({reason}): static split everywhere"
        model_2 = None
        if couple_recipe is not None:
            model_2 = couple_recipe["model_2_patched"]
            if ownership is not None:
                height, width = int(latent.shape[-2]) * 8, int(latent.shape[-1]) * 8
                scene = derive_background_mask(data, height, width)
                mask_1, mask_2 = derive_raw_region_masks(data, height, width, batch=1, background=scene is not None)
                base, person = attention_weights(data)
                r = couple_recipe
                model_2 = apply_multimask_couple(
                    r["model_2"], r["clip"], mask_1, mask_2, r["pos_1"], r["neg_1"], r["pos_2"], r["neg_2"],
                    r["strength_1"], r["strength_2"], base, person, main=r["main"],
                    scene=r.get("scene", r["main"]), background_mask=scene,
                )
                report += " | MODEL_2 rebuilt on the map" + (" + scene-only background" if scene is not None else "")
        return (canonical_imprint_json(data), model_2, report)

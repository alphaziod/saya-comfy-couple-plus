"""Saya Couple imprint v2 — three DATA-ONLY blocks.

Extends the pattern of ``couple_imprint.py`` (schema ``saya.couple.imprint``)
without ever modifying the v1 module. The schema stays ``saya.couple.imprint``;
the version becomes 2. Reading a v1 imprint is an EXPLICIT ERROR (never a
silent migration).

The three blocks:

``couple_imprint``
    Couple semantics: ``prompts`` (main / person_1 / person_2 ABSENT when
    absent / negative "" stays "" — empty text is content), ``geometry``,
    ``strengths`` ({strength_1, strength_2} 0..2, default 1.0 — the PPM
    channel used in Phase 2).

``reconstruction_recipe``
    WHAT to re-encode WITH WHAT: ``checkpoint_identity`` / ``clip_identity``
    ({role, identifier, source}; ``fingerprint`` optional, not populated in
    v1), ``lora_effective_chain`` (INFORMATIONAL — never compared against the
    runtime), ``lora_saved_snapshot`` (provenance only, NEVER reapplied),
    ``encode_options`` (explicit empty allowlist in v1), ``merge_concat_rules``
    ("separate_conds" is the v1 target), ``attention_params``.

``provenance``
    ``workflow_version``, ``created_at`` (UTC ISO), ``pack_discriminant``
    (deterministic hash of registry.py + the src/ tree — see
    ``manifest_hook.py``).

Cross-cutting rules:

* DATA-ONLY: no runtime object (MODEL, CLIP, CONDITIONING, LATENT, IMAGE,
  MASK, tensor). ``assert_data_only`` walks the structure and rejects any
  other type, across all three blocks.
* ``regions`` is the geometric TRUTH (normalized fractions against a
  reference frame, origin top-left); ``orientation`` (LEFT_RIGHT/TOP_BOTTOM
  only — never vertical/horizontal), ``split_fraction`` and ``swap`` are
  DERIVED fields with a coherence assertion on both WRITE and READ
  (incoherence is an explicit error, never a silent correction).
* ``feather`` always comes with an explicit ``feather_unit``
  ("axis_fraction" | "pixel_sigma") — never a bare float; no default is
  assumed.
* Deterministic canonical JSON: ``sort_keys=True, separators=(",", ":"),
  ensure_ascii=False`` (same rules as v1, round-trip proven).
"""

from __future__ import annotations

import json
import math
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

SCHEMA_NAME = "saya.couple.imprint"
SCHEMA_VERSION = 2

#: Canonical orientation language only. Legacy words ("vertical"/"horizontal")
#: are rejected, with the mapping spelled out in the error message.
ORIENTATION_LEFT_RIGHT = "LEFT_RIGHT"
ORIENTATION_TOP_BOTTOM = "TOP_BOTTOM"
_ORIENTATIONS = (ORIENTATION_LEFT_RIGHT, ORIENTATION_TOP_BOTTOM)

#: Feather units. Both semantics are implemented (see masks_v2).
FEATHER_AXIS_FRACTION = "axis_fraction"
FEATHER_PIXEL_SIGMA = "pixel_sigma"
_FEATHER_UNITS = (FEATHER_AXIS_FRACTION, FEATHER_PIXEL_SIGMA)

#: Frozen identity roles.
ROLE_PHASE_MODEL = "phase_model"
ROLE_MODEL_2 = "model_2"
ROLE_USDU_MODEL = "usdu_model"
ROLE_BASE_CLIP = "base_clip"
_IDENTITY_SOURCES = ("checkpoint_hub_widget",)

#: Comparison tolerance for derived fields vs. regions (serialized floats).
_COHERENCE_TOLERANCE = 1e-6


class SayaCoupleImprintError(ValueError):
    """Raised for any malformed imprint payload or field value."""


# ---------------------------------------------------------------------------
# Data-only validator (same rule as v1, extended to all three blocks)
# ---------------------------------------------------------------------------

def assert_data_only(value: Any, path: str = "imprint") -> None:
    """Reject anything that is not plain JSON data (str/int/float/bool/None/list/dict).

    Runtime objects (tensors, conditionings, models) are never JSON types:
    any other object is an error. The message names the exact path.
    """
    if value is None or isinstance(value, (str, int, float, bool)):
        return
    if isinstance(value, (list, tuple)):
        for index, item in enumerate(value):
            assert_data_only(item, f"{path}[{index}]")
        return
    if isinstance(value, dict):
        for key, item in value.items():
            if not isinstance(key, str):
                raise SayaCoupleImprintError(
                    f"{path}: non-string keys are forbidden ({key!r}) — data-only"
                )
            assert_data_only(item, f"{path}.{key}")
        return
    # Duck-type first for a useful message on the common runtime tensor case.
    if hasattr(value, "dtype") and hasattr(value, "shape"):
        raise SayaCoupleImprintError(
            f"{path}: runtime tensor forbidden in the imprint "
            f"(shape={getattr(value, 'shape', None)!r}) — DATA-ONLY"
        )
    raise SayaCoupleImprintError(
        f"{path}: runtime object forbidden in the imprint "
        f"({type(value).__name__}) — DATA-ONLY"
    )


def _require_text(value: Any, field: str) -> str:
    if not isinstance(value, str):
        raise SayaCoupleImprintError(f"{field}: expected str, got {type(value).__name__}")
    return value


def _require_float(value: Any, field: str, *, low: float, high: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise SayaCoupleImprintError(f"{field}: expected float, got {type(value).__name__}")
    result = float(value)
    if not math.isfinite(result):
        raise SayaCoupleImprintError(f"{field}: must be finite, got {result!r}")
    if result < low or result > high:
        raise SayaCoupleImprintError(f"{field}: {result} outside range {low}..{high}")
    return result


# ---------------------------------------------------------------------------
# Block — prompts
# ---------------------------------------------------------------------------

def build_prompts(
    *,
    main: str,
    person_1: str,
    negative: str,
    person_2: str | None = None,
    action: str | None = None,
    quality: str | None = None,
) -> dict[str, str]:
    """DATA-ONLY prompts. P2 empty/absent -> key ABSENT; negative "" stays "".
    ACTION (pose / act kept apart from MAIN) empty/absent -> key ABSENT (imprints written before it have none)."""
    prompts: dict[str, str] = {
        "main": _require_text(main, "prompts.main"),
        "person_1": _require_text(person_1, "prompts.person_1"),
        "negative": _require_text(negative, "prompts.negative"),
    }
    # v1 rule kept: P2 absent STAYS absent (no key, no empty string, no
    # fallback to MAIN). Asymmetric with negative on purpose.
    if person_2 is not None and _require_text(person_2, "prompts.person_2").strip():
        prompts["person_2"] = person_2
    if action is not None and _require_text(action, "prompts.action").strip():
        prompts["action"] = action
    # 2.2: QUALITY (quality tags + LoRA triggers) kept apart from MAIN, which is then the background only.
    # Empty/absent -> key ABSENT (imprints written before 2.2 have none: MAIN holds prefix + background).
    if quality is not None and _require_text(quality, "prompts.quality").strip():
        prompts["quality"] = quality
    return prompts


def _join(*parts: str | None) -> str:
    return ",\n".join(p for p in parts if p and p.strip())


def scene_text(prompts: dict[str, str]) -> str:
    """The persons' scene text in a reconstructed phase: MAIN, followed by ACTION when the imprint has one
    (Phase 1 encodes them apart; the phases rebuilt from the imprint keep both, so the pose is never lost).
    2.2 imprint with QUALITY: QUALITY, ACTION, MAIN (quality read first, background last, as in Phase 1)."""
    action = prompts.get("action", "")
    if prompts.get("quality"):
        return _join(prompts["quality"], action, prompts["main"])
    return f"{prompts['main']},\n{action}" if action.strip() else prompts["main"]


def background_text(prompts: dict[str, str]) -> str:
    """The scene-only MAIN of the background cells: MAIN, or QUALITY, MAIN for a 2.2 imprint (never ACTION)."""
    return _join(prompts.get("quality"), prompts["main"]) if prompts.get("quality") else prompts["main"]


def solo_text(prompts: dict[str, str]) -> str:
    """Solo positive rebuilt from the imprint: MAIN, ACTION, PERSON 1 (historic) or, for a 2.2 imprint,
    QUALITY, ACTION, PERSON 1, MAIN (same order as the Phase 1 Solo conditioning)."""
    if prompts.get("quality"):
        return _join(prompts["quality"], prompts.get("action", ""), prompts["person_1"], prompts["main"])
    main_text = scene_text(prompts)
    return f"{main_text}, {prompts['person_1']}" if prompts["person_1"] else main_text


# ---------------------------------------------------------------------------
# Block — geometry (regions are the truth, derived fields asserted WRITE+READ)
# ---------------------------------------------------------------------------

def _validate_region(region: Any, field: str) -> dict[str, float]:
    if not isinstance(region, dict):
        raise SayaCoupleImprintError(f"{field}: expected a region object, got {type(region).__name__}")
    result: dict[str, float] = {}
    for key in ("x0", "y0", "x1", "y1"):
        result[key] = _require_float(region.get(key), f"{field}.{key}", low=0.0, high=1.0)
    if result["x1"] <= result["x0"] or result["y1"] <= result["y0"]:
        raise SayaCoupleImprintError(
            f"{field}: empty or inverted region x[{result['x0']}:{result['x1']}] "
            f"y[{result['y0']}:{result['y1']}]"
        )
    return result


def _spans_full_axis(p1: dict[str, float], p2: dict[str, float], lo_key: str, hi_key: str) -> bool:
    """True if p1 and p2 together cover the whole [0, 1] axis on (lo_key, hi_key)."""
    return (
        abs(p1[lo_key]) <= _COHERENCE_TOLERANCE
        and abs(p1[hi_key] - 1.0) <= _COHERENCE_TOLERANCE
        and abs(p2[lo_key]) <= _COHERENCE_TOLERANCE
        and abs(p2[hi_key] - 1.0) <= _COHERENCE_TOLERANCE
    )


def derive_orientation(regions: dict[str, dict[str, float]]) -> str:
    """Derive the orientation from the regions — never the other way around."""
    p1 = regions["person_1"]
    p2 = regions.get("person_2")
    if p2 is None:
        # A single region: the axis is the one that does not span the whole frame.
        spans_x = p1["x1"] - p1["x0"]
        spans_y = p1["y1"] - p1["y0"]
        return ORIENTATION_LEFT_RIGHT if spans_x <= spans_y else ORIENTATION_TOP_BOTTOM
    full_y = _spans_full_axis(p1, p2, "y0", "y1")
    full_x = _spans_full_axis(p1, p2, "x0", "x1")
    if full_y and not full_x:
        return ORIENTATION_LEFT_RIGHT
    if full_x and not full_y:
        return ORIENTATION_TOP_BOTTOM
    raise SayaCoupleImprintError(
        "geometry.regions: the two regions do not tile the frame on any axis "
        "(v1 target = 1D split with two regions) — refused"
    )


def derive_split_and_swap(regions: dict[str, dict[str, float]], orientation: str) -> tuple[float, bool]:
    """Derive (split_fraction, swap) from the regions.

    swap = False when P1 precedes P2 on the axis; True when P1 follows P2.
    """
    p1 = regions["person_1"]
    p2 = regions.get("person_2")
    axis = "x" if orientation == ORIENTATION_LEFT_RIGHT else "y"
    if p2 is None:
        return float(p1[f"{axis}1"]), False
    p1_lo, p1_hi = p1[f"{axis}0"], p1[f"{axis}1"]
    p2_lo, p2_hi = p2[f"{axis}0"], p2[f"{axis}1"]
    if abs(p1_hi - p2_lo) <= _COHERENCE_TOLERANCE and p1_lo <= p2_lo:
        return float(p1_hi), False
    if abs(p2_hi - p1_lo) <= _COHERENCE_TOLERANCE and p2_lo <= p1_lo:
        return float(p2_hi), True
    raise SayaCoupleImprintError(
        f"geometry.regions: P1/P2 boundary is not contiguous on axis {axis} "
        f"(p1[{p1_lo}:{p1_hi}] vs p2[{p2_lo}:{p2_hi}]) — refused"
    )


def build_geometry(
    *,
    reference_width: int,
    reference_height: int,
    person_1_region: dict[str, float],
    person_2_region: dict[str, float] | None = None,
    feather: float = 0.0,
    feather_unit: str | None = None,
    mask_floor: float = 0.0,
) -> dict[str, Any]:
    """Build the geometry block. ``regions`` is the truth; derived fields are asserted on WRITE."""
    if isinstance(reference_width, bool) or not isinstance(reference_width, int) or reference_width < 1:
        raise SayaCoupleImprintError(f"reference_width: expected int >= 1, got {reference_width!r}")
    if isinstance(reference_height, bool) or not isinstance(reference_height, int) or reference_height < 1:
        raise SayaCoupleImprintError(f"reference_height: expected int >= 1, got {reference_height!r}")

    regions: dict[str, dict[str, float]] = {
        "person_1": _validate_region(person_1_region, "geometry.regions.person_1"),
    }
    if person_2_region is not None:
        regions["person_2"] = _validate_region(person_2_region, "geometry.regions.person_2")

    # Derived fields: always recomputed from regions (never accepted as
    # input — the state stays self-verifiable).
    orientation = derive_orientation(regions)
    split_fraction, swap = derive_split_and_swap(regions, orientation)

    # feather NEVER bare: the unit is mandatory even at 0.0.
    feather_value = _require_float(feather, "geometry.feather", low=0.0, high=1.0e9)
    if feather_unit not in _FEATHER_UNITS:
        raise SayaCoupleImprintError(
            f"geometry.feather_unit: {feather_unit!r} invalid, expected one of "
            f"{_FEATHER_UNITS} — a feather without an explicit unit is refused"
        )
    if feather_value > 1.0:
        raise SayaCoupleImprintError(
            f"geometry.feather: {feather_value} outside range 0..1 for unit "
            f"{feather_unit} (axis_fraction: fraction of the axis; pixel_sigma: "
            "sigma normalized by the reference size)"
        )

    # floor < 0.5 strict: a person's own region must dominate the residual it
    # leaves elsewhere (region-mask semantics).
    floor = _require_float(mask_floor, "geometry.mask_floor", low=0.0, high=0.5)
    if floor >= 0.5:
        raise SayaCoupleImprintError("geometry.mask_floor: must stay < 0.5")
    geometry = {
        "reference_width": reference_width,
        "reference_height": reference_height,
        "regions": regions,
        "derived": {
            "orientation": orientation,
            "split_fraction": split_fraction,
            "swap": swap,
        },
        "feather": feather_value,
        "feather_unit": feather_unit,
        "mask_floor": floor,
    }
    assert_geometry_coherence(geometry, context="geometry(build)")
    return geometry


def assert_geometry_coherence(geometry: Any, context: str = "geometry") -> None:
    """Assert derived fields recomputed == derived fields announced, on WRITE and READ.

    Any incoherence is an explicit error (never a silent correction). Called
    at construction AND at every read (parse).
    """
    if not isinstance(geometry, dict):
        raise SayaCoupleImprintError(f"{context}: expected a geometry object")
    regions = geometry.get("regions")
    if not isinstance(regions, dict) or "person_1" not in regions:
        raise SayaCoupleImprintError(f"{context}.regions: person_1 is mandatory")
    clean_regions = {
        "person_1": _validate_region(regions["person_1"], f"{context}.regions.person_1")
    }
    if "person_2" in regions:
        clean_regions["person_2"] = _validate_region(regions["person_2"], f"{context}.regions.person_2")

    orientation = derive_orientation(clean_regions)
    split_fraction, swap = derive_split_and_swap(clean_regions, orientation)

    derived = geometry.get("derived")
    if not isinstance(derived, dict):
        raise SayaCoupleImprintError(f"{context}.derived: object expected (orientation/split_fraction/swap)")
    if derived.get("orientation") != orientation:
        raise SayaCoupleImprintError(
            f"{context}: incoherent derived orientation — announced "
            f"{derived.get('orientation')!r}, recomputed from regions {orientation!r}"
        )
    announced_split = derived.get("split_fraction")
    if not isinstance(announced_split, (int, float)) or isinstance(announced_split, bool) \
            or abs(float(announced_split) - split_fraction) > _COHERENCE_TOLERANCE:
        raise SayaCoupleImprintError(
            f"{context}: incoherent split_fraction — announced {announced_split!r}, "
            f"recomputed from regions {split_fraction!r}"
        )
    announced_swap = derived.get("swap")
    if not isinstance(announced_swap, bool) or announced_swap != swap:
        raise SayaCoupleImprintError(
            f"{context}: incoherent swap — announced {announced_swap!r}, "
            f"recomputed from regions {swap!r}"
        )

    # The unit MUST exist, even for feather 0.0.
    unit = geometry.get("feather_unit")
    if unit not in _FEATHER_UNITS:
        raise SayaCoupleImprintError(
            f"{context}.feather_unit: {unit!r} invalid — feather is NEVER bare"
        )
    _require_float(geometry.get("feather"), f"{context}.feather", low=0.0, high=1.0)


# ---------------------------------------------------------------------------
# Block — strengths
# ---------------------------------------------------------------------------

def build_strengths(strength_1: float = 1.0, strength_2: float = 1.0) -> dict[str, float]:
    """{strength_1, strength_2} 0..2 default 1.0 — the PPM channel (cond[0][1]) in Phase 2."""
    return {
        "strength_1": _require_float(strength_1, "strengths.strength_1", low=0.0, high=2.0),
        "strength_2": _require_float(strength_2, "strengths.strength_2", low=0.0, high=2.0),
    }


# ---------------------------------------------------------------------------
# Block — reconstruction_recipe
# ---------------------------------------------------------------------------

def build_identity(*, role: str, identifier: str, source: str = "checkpoint_hub_widget") -> dict[str, str]:
    """Structured identity. ``fingerprint`` optional, not populated in v1."""
    if role not in (ROLE_PHASE_MODEL, ROLE_MODEL_2, ROLE_USDU_MODEL, ROLE_BASE_CLIP):
        raise SayaCoupleImprintError(f"identity.role: {role!r} is outside the frozen roles")
    if not identifier or not isinstance(identifier, str):
        raise SayaCoupleImprintError("identity.identifier: expected the exact .safetensors basename")
    if source not in _IDENTITY_SOURCES:
        raise SayaCoupleImprintError(f"identity.source: {source!r} is outside the frozen sources")
    return {"role": role, "identifier": identifier, "source": source}


def build_lora_entry(name: str, strength_model: float, strength_clip: float) -> dict[str, Any]:
    return {
        "name": _require_text(name, "lora.name"),
        "strength_model": _require_float(strength_model, "lora.strength_model", low=-8.0, high=8.0),
        "strength_clip": _require_float(strength_clip, "lora.strength_clip", low=-8.0, high=8.0),
    }


def build_snapshot_entry(name: str, strength: float, active: bool) -> dict[str, Any]:
    return {
        "name": _require_text(name, "snapshot.name"),
        "strength": _require_float(strength, "snapshot.strength", low=-8.0, high=8.0),
        "active": bool(active),
    }


DEFAULT_ATTENTION_PARAMS: dict[str, float] = {
    # Values inherited from the legacy SayaPPMMasks widgets of the injection —
    # weights BEFORE the PPM sum normalization; documented candidates, not
    # measured preferences.
    "base_weight": 0.65,
    "person_weight": 0.35,
}


def build_reconstruction_recipe(
    *,
    checkpoint_identity: dict[str, str],
    clip_identity: dict[str, str],
    lora_effective_chain: list[dict[str, Any]] | None = None,
    lora_saved_snapshot: list[dict[str, Any]] | None = None,
    encode_options: dict[str, Any] | None = None,
    merge_concat_rules: str = "separate_conds",
    attention_params: dict[str, float] | None = None,
) -> dict[str, Any]:
    """``encode_options`` is an explicit empty allowlist in v1 — any key is an error."""
    identity_fields = ("role", "identifier", "source")
    for label, identity in (("checkpoint_identity", checkpoint_identity), ("clip_identity", clip_identity)):
        if not isinstance(identity, dict) or any(f not in identity for f in identity_fields):
            raise SayaCoupleImprintError(
                f"recipe.{label}: {{role, identifier, source}} are mandatory, got {identity!r}"
            )
    chain = [build_lora_entry(**entry) for entry in (lora_effective_chain or [])]
    snapshot = [build_snapshot_entry(**entry) for entry in (lora_saved_snapshot or [])]
    # The v1 allowlist is explicitly EMPTY. Any non-empty key is an explicit
    # error (unknown key, no fallback). Conditioning manipulation is RUNTIME
    # (node code), never part of the recipe.
    if encode_options is None:
        encode_options = {}
    if not isinstance(encode_options, dict) or encode_options != {}:
        raise SayaCoupleImprintError(
            f"recipe.encode_options: the v1 allowlist is EMPTY — {encode_options!r} refused"
        )
    if merge_concat_rules != "separate_conds":
        raise SayaCoupleImprintError(
            f"recipe.merge_concat_rules: {merge_concat_rules!r} invalid — the v1 "
            "target is 'separate_conds' (the historical text-merge behavior)"
        )
    params = dict(DEFAULT_ATTENTION_PARAMS)
    if attention_params is not None:
        if not isinstance(attention_params, dict):
            raise SayaCoupleImprintError("recipe.attention_params: object expected")
        for key, value in attention_params.items():
            params[key] = _require_float(value, f"recipe.attention_params.{key}", low=0.0, high=1.0)
    return {
        "checkpoint_identity": dict(checkpoint_identity),
        "clip_identity": dict(clip_identity),
        "lora_effective_chain": chain,       # INFORMATIONAL — never compared against the runtime
        "lora_saved_snapshot": snapshot,     # provenance only — NEVER reapplied
        "encode_options": {},
        "merge_concat_rules": merge_concat_rules,
        "attention_params": params,
    }


# ---------------------------------------------------------------------------
# Block — provenance
# ---------------------------------------------------------------------------

def build_provenance(
    *,
    workflow_version: str,
    pack_discriminant: str,
    created_at: str | None = None,
) -> dict[str, str]:
    if not workflow_version or not isinstance(workflow_version, str):
        raise SayaCoupleImprintError("provenance.workflow_version: expected a non-empty str")
    if not pack_discriminant or not isinstance(pack_discriminant, str):
        raise SayaCoupleImprintError(
            "provenance.pack_discriminant: expected a hash of registry.py + the "
            "src/ tree (a plain VERSION.txt is not discriminant enough)"
        )
    stamp = created_at or datetime.now(timezone.utc).isoformat(timespec="seconds")
    return {
        "workflow_version": workflow_version,
        "created_at": stamp,
        "pack_discriminant": pack_discriminant,
    }


# ---------------------------------------------------------------------------
# Optional block — couple_imprint.ownership_map (Phase 1 dynamic ownership)
# ---------------------------------------------------------------------------

#: One character per cell of the Sampler 1 ownership grid (row-major, top-left origin).
OWNERSHIP_P1, OWNERSHIP_P2, OWNERSHIP_BACKGROUND, OWNERSHIP_STATIC = "1", "2", "b", "s"
OWNERSHIP_CELLS = OWNERSHIP_P1 + OWNERSHIP_P2 + OWNERSHIP_BACKGROUND + OWNERSHIP_STATIC
OWNERSHIP_SOURCE = "s1_dynamic"
_OWNERSHIP_MAX_SIDE = 512


def build_ownership_map(*, grid: Any, rows: Any, source: str = OWNERSHIP_SOURCE) -> dict[str, Any]:
    """Validate the Sampler 1 ownership map: who owns each cell, read by every later pass.

    ``1``/``2`` = P1/P2 (binary, hard edges), ``b`` = background and ``s`` = undecided: both keep
    the static split of ``geometry`` (the map never invents an owner the engine did not confirm).
    """
    if source != OWNERSHIP_SOURCE:
        raise SayaCoupleImprintError(f"ownership_map.source: {source!r} invalid, expected {OWNERSHIP_SOURCE!r}")
    if (not isinstance(grid, list) or len(grid) != 2
            or any(isinstance(v, bool) or not isinstance(v, int) or not 1 <= v <= _OWNERSHIP_MAX_SIDE for v in grid)):
        raise SayaCoupleImprintError(f"ownership_map.grid: [height, width] in 1..{_OWNERSHIP_MAX_SIDE} expected, got {grid!r}")
    height, width = grid
    if not isinstance(rows, list) or len(rows) != height:
        raise SayaCoupleImprintError(f"ownership_map.rows: {height} rows expected")
    for index, row in enumerate(rows):
        if not isinstance(row, str) or len(row) != width or set(row) - set(OWNERSHIP_CELLS):
            raise SayaCoupleImprintError(
                f"ownership_map.rows[{index}]: {width} characters among {OWNERSHIP_CELLS!r} expected"
            )
    return {"grid": [height, width], "rows": list(rows), "source": source}


# ---------------------------------------------------------------------------
# Full imprint + canon + parse
# ---------------------------------------------------------------------------

def build_imprint_v2(
    *,
    prompts: dict[str, str],
    geometry: dict[str, Any],
    strengths: dict[str, float],
    reconstruction_recipe: dict[str, Any],
    provenance: dict[str, Any],
) -> dict[str, Any]:
    """Assemble and validate the complete v2 imprint (three DATA-ONLY blocks)."""
    imprint = {
        "schema": SCHEMA_NAME,
        "version": SCHEMA_VERSION,
        "couple_imprint": {
            "prompts": prompts,
            "geometry": geometry,
            "strengths": strengths,
        },
        "reconstruction_recipe": reconstruction_recipe,
        "provenance": provenance,
    }
    return validate_imprint_v2(imprint, context="imprint(build)")


def validate_imprint_v2(data: Any, context: str = "imprint") -> dict[str, Any]:
    """Full STRICT validation: structure, data-only, geometry coherence."""
    if not isinstance(data, dict):
        raise SayaCoupleImprintError(f"{context}: the root must be an object")
    if data.get("schema") != SCHEMA_NAME:
        raise SayaCoupleImprintError(
            f"{context}: unknown schema {data.get('schema')!r}, expected {SCHEMA_NAME!r}"
        )
    version = data.get("version")
    if version == 1:
        raise SayaCoupleImprintError(
            f"{context}: version 1 imprint is not supported — no silent "
            "migration. Regenerate the imprint as v2 from Phase 1."
        )
    if version != SCHEMA_VERSION:
        raise SayaCoupleImprintError(
            f"{context}: version {version!r} is not supported (this build reads version {SCHEMA_VERSION})"
        )
    for block in ("couple_imprint", "reconstruction_recipe", "provenance"):
        if not isinstance(data.get(block), dict):
            raise SayaCoupleImprintError(f"{context}.{block}: mandatory block missing")
    couple = data["couple_imprint"]
    for field in ("prompts", "geometry", "strengths"):
        if not isinstance(couple.get(field), dict):
            raise SayaCoupleImprintError(f"{context}.couple_imprint.{field}: mandatory sub-block missing")
    assert_data_only(data, context)
    assert_geometry_coherence(couple["geometry"], context=f"{context}.couple_imprint.geometry")
    # Re-validation through the builders (idempotent on a valid payload).
    try:
        _revalidate_blocks(data, context)
    except TypeError as error:
        # A key missing from / unexpected in a block reaches the builders' signatures.
        raise SayaCoupleImprintError(f"{context}: malformed block ({error})") from error
    return data


def _revalidate_blocks(data: dict[str, Any], context: str) -> None:
    couple = data["couple_imprint"]
    rebuilt_prompts = build_prompts(**couple["prompts"])
    if set(rebuilt_prompts) != set(couple["prompts"]):
        raise SayaCoupleImprintError(
            f"{context}.couple_imprint.prompts: an empty/blank person_2 must be "
            "ABSENT (key present with an empty value — P2 incoherence, never a "
            "silent correction)"
        )
    build_strengths(**couple["strengths"])
    if "ownership_map" in couple:
        if "person_2" not in couple["prompts"]:
            raise SayaCoupleImprintError(f"{context}.couple_imprint.ownership_map: a Solo imprint has no P1/P2 map")
        ownership = couple["ownership_map"]
        if not isinstance(ownership, dict):
            raise SayaCoupleImprintError(f"{context}.couple_imprint.ownership_map: object expected")
        build_ownership_map(**ownership)
    recipe = data["reconstruction_recipe"]
    build_reconstruction_recipe(
        checkpoint_identity=recipe.get("checkpoint_identity"),
        clip_identity=recipe.get("clip_identity"),
        lora_effective_chain=recipe.get("lora_effective_chain"),
        lora_saved_snapshot=recipe.get("lora_saved_snapshot"),
        encode_options=recipe.get("encode_options"),
        merge_concat_rules=recipe.get("merge_concat_rules"),
        attention_params=recipe.get("attention_params"),
    )
    prov = data["provenance"]
    build_provenance(
        workflow_version=prov.get("workflow_version"),
        pack_discriminant=prov.get("pack_discriminant"),
        created_at=prov.get("created_at"),
    )


def canonical_imprint_json(imprint: dict[str, Any]) -> str:
    """Serialize deterministically (sort_keys, no whitespace) — v1 rules kept."""
    return json.dumps(imprint, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def parse_imprint_json(payload: str) -> dict[str, Any]:
    """Parse + strict v2 validation. v1 -> explicit error, never a guess."""
    try:
        data = json.loads(payload)
    except json.JSONDecodeError as error:
        raise SayaCoupleImprintError(f"imprint: invalid JSON: {error}") from None
    return validate_imprint_v2(data, context="imprint(parse)")


# ---------------------------------------------------------------------------
# Convenience: minimal coherent imprint (Phase 1 / tests usage)
# ---------------------------------------------------------------------------

def build_two_region_geometry(
    *,
    orientation: str,
    reference_width: int,
    reference_height: int,
    split_fraction: float = 0.5,
    swap: bool = False,
    feather: float = 0.0,
    feather_unit: str | None = None,
    person_2_present: bool = True,
    mask_floor: float = 0.0,
) -> dict[str, Any]:
    """Two-region 1D geometry (v1 target: a 1D split)."""
    split = _require_float(split_fraction, "split_fraction", low=0.0, high=1.0)
    if orientation == ORIENTATION_LEFT_RIGHT:
        p1 = {"x0": 0.0, "y0": 0.0, "x1": split, "y1": 1.0}
        p2 = {"x0": split, "y0": 0.0, "x1": 1.0, "y1": 1.0}
    elif orientation == ORIENTATION_TOP_BOTTOM:
        p1 = {"x0": 0.0, "y0": 0.0, "x1": 1.0, "y1": split}
        p2 = {"x0": 0.0, "y0": split, "x1": 1.0, "y1": 1.0}
    else:
        raise SayaCoupleImprintError(
            f"orientation: {orientation!r} invalid — LEFT_RIGHT/TOP_BOTTOM only"
        )
    if swap:
        p1, p2 = p2, p1
    return build_geometry(
        reference_width=reference_width,
        reference_height=reference_height,
        person_1_region=p1,
        person_2_region=p2 if person_2_present else None,
        feather=feather,
        feather_unit=feather_unit,
        mask_floor=mask_floor,
    )


class SayaCoupleImprintPackV2:
    """Build the strict Phase-1 v2 imprint from the live Couple primitives."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "main_prompt": ("STRING", {"default": "", "multiline": True}),
                "person_1_prompt": ("STRING", {"default": "", "multiline": True}),
                "negative_prompt": ("STRING", {"default": "", "multiline": True}),
                "direction": (["vertical", "horizontal"], {"default": "vertical"}),
                "split": ("INT", {"default": 50, "min": 1, "max": 99, "step": 1}),
                "blur": ("FLOAT", {"default": 0.0, "min": 0.0, "step": 0.1}),
                "strength_1": ("FLOAT", {"default": 1.0, "min": 0.0, "max": 2.0, "step": 0.05}),
                "strength_2": ("FLOAT", {"default": 1.0, "min": 0.0, "max": 2.0, "step": 0.05}),
                "reference_width": ("INT", {"forceInput": True}),
                "reference_height": ("INT", {"forceInput": True}),
                "phase_model_identifier": ("STRING", {"default": ""}),
                "base_clip_identifier": ("STRING", {"default": ""}),
                "workflow_version": ("STRING", {"default": ""}),
            },
            "optional": {
                "person_2_prompt": ("STRING", {"default": "", "multiline": True, "forceInput": True}),
                "swap": ("BOOLEAN", {"default": False, "label_on": "true", "label_off": "false"}),
                "solo": ("BOOLEAN", {"forceInput": True, "tooltip": "Couple Mode OFF. When connected, Couple with an empty PERSON_2 is refused here instead of in Phase 2."}),
                "action_prompt": ("STRING", {"default": "", "multiline": True, "forceInput": True,
                                             "tooltip": "ACTION text (pose / act) of Phase 1, kept so the later phases rebuild MAIN + ACTION."}),
                "quality_prompt": ("STRING", {"default": "", "multiline": True, "forceInput": True,
                                              "tooltip": "2.2: quality tags + LoRA triggers (Saya Main Prompt 'prefix'). When wired, main_prompt "
                                                         "is the background only ('scene'), and every later phase reads QUALITY first and "
                                                         "the background last, like Phase 1."}),
            },
        }

    RETURN_TYPES = ("SAYA_IMPRINT", "STRING")
    RETURN_NAMES = ("imprint", "imprint_json")
    FUNCTION = "pack"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Produces the canonical Couple v2 imprint natively for Phase 1. "
        "Identities come from the real checkpoint hub widgets; no v1 migration."
    )

    def pack(
        self,
        main_prompt: str,
        person_1_prompt: str,
        negative_prompt: str,
        direction: str,
        split: int,
        blur: float,
        strength_1: float,
        strength_2: float,
        reference_width: int,
        reference_height: int,
        phase_model_identifier: str,
        base_clip_identifier: str,
        workflow_version: str,
        person_2_prompt: str | None = None,
        swap: bool = False,
        solo: bool | None = None,
        action_prompt: str | None = None,
        quality_prompt: str | None = None,
    ) -> tuple[dict[str, Any], str]:
        from ..services.imprint_integrity import pack_discriminant

        orientation_by_legacy_direction = {
            "vertical": ORIENTATION_LEFT_RIGHT,
            "horizontal": ORIENTATION_TOP_BOTTOM,
        }
        try:
            orientation = orientation_by_legacy_direction[direction]
        except KeyError:
            raise SayaCoupleImprintError(
                f"direction: {direction!r} invalid, expected vertical|horizontal"
            ) from None
        if isinstance(blur, bool) or not isinstance(blur, (int, float)) or not math.isfinite(float(blur)):
            raise SayaCoupleImprintError(f"blur: expected a finite float >= 0, got {blur!r}")
        if float(blur) < 0.0:
            raise SayaCoupleImprintError(f"blur: must be >= 0, got {blur!r}")
        # The imprint records an empty PERSON_2 as absent, and the Couple
        # reconstruct of Phase 2 refuses that: fail before Phase 1 is validated.
        if solo is False and not (person_2_prompt and person_2_prompt.strip()):
            raise SayaCoupleImprintError(
                "Couple mode needs a PERSON_2 prompt (Phase 2 cannot rebuild a couple without it); "
                "fill PERSON_2 or switch Couple Mode OFF"
            )
        if reference_width < 1 or reference_height < 1:
            raise SayaCoupleImprintError(
                f"reference size: expected positive dimensions, got {reference_width}x{reference_height}"
            )

        # SayaSplitMask expresses blur as a sigma in pixels. The v2 contract
        # normalizes it by max(width, height), then reconstructs it exactly
        # in region_masks.py.
        feather = float(blur) / float(max(reference_width, reference_height))
        imprint = build_imprint_v2(
            prompts=build_prompts(
                main=main_prompt,
                person_1=person_1_prompt,
                person_2=person_2_prompt,
                negative=negative_prompt,
                action=action_prompt,
                quality=quality_prompt,
            ),
            geometry=build_two_region_geometry(
                orientation=orientation,
                reference_width=reference_width,
                reference_height=reference_height,
                split_fraction=float(split) / 100.0,
                swap=bool(swap),
                feather=feather,
                feather_unit=FEATHER_PIXEL_SIGMA,
                person_2_present=bool(person_2_prompt and person_2_prompt.strip()),
            ),
            strengths=build_strengths(strength_1, strength_2),
            reconstruction_recipe=build_reconstruction_recipe(
                checkpoint_identity=build_identity(
                    role=ROLE_PHASE_MODEL,
                    identifier=phase_model_identifier,
                ),
                clip_identity=build_identity(
                    role=ROLE_BASE_CLIP,
                    identifier=base_clip_identifier,
                ),
            ),
            provenance=build_provenance(
                workflow_version=workflow_version,
                pack_discriminant=pack_discriminant(Path(__file__).resolve().parents[2]),
            ),
        )
        return imprint, canonical_imprint_json(imprint)

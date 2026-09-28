"""Saya USDU engine -- explicit config_pass.

ONE implementation, TWO explicit configs: no ``if pass == 2`` branch anywhere
in the engine. This module is the data contract shared by both passes; the
wrapper node (``usdu_pass_node.SayaCoupleUSDUPass``) delegates to the
installed engine ``UltimateSDUpscaleCustomSample`` using the pack's proven
pattern (``duo_geometry/nodes.py:233-312``: lazy lookup, MODEL clone, kw-only
kwargs).

Frozen engine facts (verified against the locally installed build, not the
upstream docs):

* LIVE_GRID_BEHAVIOR = **CEIL**, imposed (``ultimate-upscale.py:39-40``,
  consumed at :138/:145) -- not a parameter. The ``round`` grid (banker's
  rounding, ``processing.py:231-232``) only feeds the tqdm counter
  (:288-294); on 1792x2304 with 512 tiles: 20 real tiles vs 16 displayed.
* per-tile WORK size = ``round8(tile + padding)`` (``usdu_patch.py:65-69``,
  round8 at :38-39) = 544 px at 512+32 -- this overrides the upstream
  ceil/64 (``ultimate-upscale.py:157-158``). The padding is the local build's
  own padding (added to the tile, not doubled) -- a verified local-build
  fact, not an assumption.
* final canvas = ``round8(image x upscale_by)`` (``usdu_patch.py:53-54``).
* ``upscale_model`` is OPTIONAL (``usdu_nodes.py:220-221,237``): absent ->
  internal Lanczos (``upscaler.py:8-19``; ``upscale_by=1`` -> zero iterations).
  Open question: which prior pixel-upscale pass 1 should default to (internal
  Lanczos vs an optional model).
* sigmas OR (sampler, scheduler, steps, denoise) are EXCLUSIVE; in sigmas
  mode the tuple would only be an inert fallback -- config_pass carries
  exactly one of the two, never an ambiguous mix. ``cfg`` stays LIVE even in
  sigmas mode (``SamplerCustom.execute(cfg=...)``, ``processing.py:322-344``).
* seam fix: ``seam_fix_denoise`` is INERT in this port
  (``ultimate-upscale.py:274,325,359`` writes ``p.denoising_strength`` but
  ``processing.py:411`` reads ``p.denoise``) -- v1 locks it to "None".
* ``restore_to_base``: per pass; USDU1 must be False so the USDU1->USDU2
  continuation exists. The historical True (both workflows rendered at
  W0xH0) is the documented default for pass 2 (an open, configurable Saya
  decision).
* ``couple_crop`` = ``not solo`` (set by ``SayaCoupleUSDUPass``): each tile
  receives its slice of the couple mask (engine gate: the ``saya_couple_crop``
  metadata, ``crop_model_patch.py``; consumer:
  ``ppm_vendor/attention_couple/common.py::crop_mask_to_tile``).
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any

#: v1 target: seam fix is locked to "None" (seam_fix_denoise is inert in this port).
SEAM_FIX_V1 = "None"

#: Pass category (informational only -- never branched on in the engine).
PASS_IDS = ("usdu_1", "usdu_2")


class SayaUSDUConfigError(ValueError):
    """Raised for an ambiguous or out-of-contract USDU config_pass."""


def round8(value: float) -> int:
    """round(x/8)*8 -- banker's rounding, matching the local build's round_length."""
    return int(round(float(value) / 8.0)) * 8


def tile_work_size(tile: int, padding: int) -> int:
    """Per-tile work size = round8(tile + padding) -- usdu_patch.py:65-69."""
    for name, value in (("tile", tile), ("padding", padding)):
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            raise SayaUSDUConfigError(f"{name} must be a non-negative int, got {value!r}")
    return round8(tile + padding)


def canvas_size(width: int, height: int, upscale_by: float) -> tuple[int, int]:
    """Final canvas = round8(W x upscale_by) x round8(H x upscale_by) -- usdu_patch.py:53-54."""
    factor = float(upscale_by)
    if factor <= 0.0:
        raise SayaUSDUConfigError(f"upscale_by must be > 0, got {factor}")
    return round8(width * factor), round8(height * factor)


def live_grid(width: int, height: int, tile: int) -> tuple[int, int]:
    """LIVE grid (rows, cols) = (ceil(H/tile), ceil(W/tile)) -- CEIL imposed.

    Fixed convention: rows first (H axis), cols second (W axis).
    """
    if isinstance(tile, bool) or not isinstance(tile, int) or tile < 1:
        raise SayaUSDUConfigError(f"tile must be a positive int, got {tile!r}")
    rows = math.ceil(height / tile)
    cols = math.ceil(width / tile)
    return rows, cols


@dataclass(frozen=True)
class SamplerFallback:
    """Fallback tuple (sampler, scheduler, steps, denoise) -- EXCLUSIVE with sigmas."""

    sampler: str
    scheduler: str
    steps: int
    denoise: float


@dataclass(frozen=True)
class ConfigPass:
    """Explicit config_pass for one USDU pass. Immutable and validatable.

    ``sigmas`` and ``sampler_fallback`` are EXCLUSIVE: exactly one of the two.
    ``cfg`` stays LIVE even in sigmas mode. ``upscale_model`` is OPTIONAL
    (None -> internal Lanczos). ``couple_crop`` is derived from the Couple/Solo
    mode by the node. ``seam_fix`` is locked.
    """

    pass_id: str
    cfg: float                                   # always LIVE
    upscale_by: float = 1.0
    tile_size: int = 512
    padding: int = 32
    mask_blur: int = 8
    structure_preservation: float = 0.75
    couple_crop: bool = True                     # always set by the node: not solo
    restore_to_base: bool = True                 # open decision -- documented historical default
    seam_fix: str = SEAM_FIX_V1                  # locked in v1
    upscale_model: str | None = None             # None -> internal Lanczos
    sigmas: Any = None                           # SIGMAS (or None)
    sampler_fallback: SamplerFallback | None = None
    #: INERT fallback in sigmas mode: mandatory placeholders for the upstream
    #: widget signature. Out-of-semantics, documented as such.
    inert_widget_fallback: SamplerFallback | None = None


def validate_config_pass(config: ConfigPass) -> ConfigPass:
    """Validate a config_pass: exclusivity, LIVE cfg, v1 locks."""
    if not isinstance(config, ConfigPass):
        raise SayaUSDUConfigError(f"config_pass: expected ConfigPass, got {type(config).__name__}")
    if config.pass_id not in PASS_IDS:
        raise SayaUSDUConfigError(f"pass_id: {config.pass_id!r} not in {PASS_IDS}")
    if isinstance(config.cfg, bool) or not isinstance(config.cfg, (int, float)) or config.cfg < 0.0:
        raise SayaUSDUConfigError(f"cfg (LIVE): expected float >= 0, got {config.cfg!r}")
    if config.upscale_by <= 0.0:
        raise SayaUSDUConfigError(f"upscale_by: expected > 0, got {config.upscale_by}")
    if config.seam_fix != SEAM_FIX_V1:
        raise SayaUSDUConfigError(
            f"seam_fix: {config.seam_fix!r} rejected -- v1 locks it to "
            f"{SEAM_FIX_V1!r} (seam_fix_denoise is inert in this port)"
        )
    has_sigmas = config.sigmas is not None
    has_tuple = config.sampler_fallback is not None
    if has_sigmas and has_tuple:
        raise SayaUSDUConfigError(
            "config_pass: both sigmas AND (sampler, scheduler, steps, denoise) "
            "were supplied -- they are EXCLUSIVE. config_pass carries one or "
            "the other, never a mix."
        )
    if not has_sigmas and not has_tuple:
        raise SayaUSDUConfigError(
            "config_pass: neither sigmas nor (sampler, scheduler, steps, "
            "denoise) was supplied -- exactly one of the two is required"
        )
    if has_tuple:
        fb = config.sampler_fallback
        if not isinstance(fb, SamplerFallback):
            raise SayaUSDUConfigError("sampler_fallback: expected a SamplerFallback")
        if isinstance(fb.steps, bool) or not isinstance(fb.steps, int) or fb.steps < 1:
            raise SayaUSDUConfigError(f"steps: expected int >= 1, got {fb.steps!r}")
        if isinstance(fb.denoise, bool) or not isinstance(fb.denoise, (int, float)) \
                or not (0.0 <= float(fb.denoise) <= 1.0):
            raise SayaUSDUConfigError(f"denoise: expected float 0..1, got {fb.denoise!r}")
    if config.inert_widget_fallback is not None and not has_sigmas:
        raise SayaUSDUConfigError(
            "inert_widget_fallback: upstream placeholders are only allowed in "
            "sigmas mode (otherwise they would be ambiguous with sampler_fallback)"
        )
    if config.structure_preservation < 0.0 or config.structure_preservation > 1.0:
        raise SayaUSDUConfigError(
            f"structure_preservation: expected 0..1, got {config.structure_preservation}"
        )
    return config


def to_engine_kwargs(
    config: ConfigPass,
    *,
    inert_default_fallback: SamplerFallback | None = None,
) -> dict[str, Any]:
    """Map a validated config_pass to the installed engine's kwargs.

    Pattern (documented, not imported): ``duo_geometry/nodes.py:109-175``
    (``_usdu_call_kwargs``) -- the call MUST be kw-only (the upstream
    positional order would swap seam_fix_width and seam_fix_mask_blur).
    ``inert_default_fallback`` = upstream placeholders for sigmas mode
    (inert widgets, out of semantics). This is a PURE function (testable
    without ComfyUI): the wrapper node adds image/model/conds/vae/seed and
    the real SIGMAS object.
    """
    config = validate_config_pass(config)
    fallback = config.sampler_fallback or config.inert_widget_fallback or inert_default_fallback
    if fallback is None:
        raise SayaUSDUConfigError(
            "to_engine_kwargs: no upstream fallback available (sigmas mode "
            "without inert_widget_fallback) -- the upstream signature requires "
            "widget values"
        )
    return {
        "upscale_by": config.upscale_by,
        "steps": fallback.steps,
        "cfg": config.cfg,                      # LIVE even in sigmas mode
        "sampler_name": fallback.sampler,
        "scheduler": fallback.scheduler,
        "denoise": fallback.denoise,
        "tile_width": config.tile_size,
        "tile_height": config.tile_size,
        "mask_blur": config.mask_blur,
        "tile_padding": config.padding,
        "mode_type": "Linear",                   # tile ordering -- engine default, not a Saya decision
        "seam_fix_mode": SEAM_FIX_V1,
        # seam_fix_*: INERT while seam_fix_mode="None" -- engine defaults
        # (UltimateSDUpscaleCustomSample.INPUT_TYPES) carried as-is, never read.
        "seam_fix_denoise": 1.0,
        "seam_fix_width": 64,
        "seam_fix_mask_blur": 8,
        "seam_fix_padding": 16,
        "force_uniform_tiles": True,
        "tiled_decode": False,
        "batch_size": 1,
        "upscale_model": config.upscale_model,  # optional -- None -> internal Lanczos
        "structure_preservation": config.structure_preservation,
    }


__all__ = [
    "SEAM_FIX_V1",
    "ConfigPass",
    "SamplerFallback",
    "SayaUSDUConfigError",
    "canvas_size",
    "live_grid",
    "round8",
    "tile_work_size",
    "to_engine_kwargs",
    "validate_config_pass",
]

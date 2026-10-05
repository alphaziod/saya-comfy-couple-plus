"""SayaCoupleUSDUPass -- one config-explicit USDU pass, registered and live.

Delegates EVERY pixel to the installed engine ``UltimateSDUpscaleCustomSample``
-- tiling, seam fixing and compositing all stay in ONE implementation -- via
the pack's proven pattern (``duo_geometry/nodes.py:233-312``):

* lazy lookup through ``nodes.NODE_CLASS_MAPPINGS`` (never imported at module
  scope: this file must stay importable without ComfyUI);
* ``model.clone()`` -- the caller's MODEL is never mutated (re-cloned once
  per pass; the couple patch lives on the input clone);
* the call is KW-ONLY (the upstream positional order would swap
  seam_fix_width and seam_fix_mask_blur, ``nodes.py:305-308``).

Contracts carried by the config:

* couple crop = ``not solo``: the global Couple/Solo mode is the only
  authority (no widget). Couple -> each tile receives ITS own slice of the
  couple mask; Solo -> no crop (the model carries no couple patch). The
  ``mask_base``/``mask_p1``/``mask_p2`` inputs below
  are only a WIRING GUARD for the crop: this node checks that they are
  connected, but never reads their tensor values itself. The actual crop is
  performed by the engine reading the couple mask already baked into the
  cloned MODEL's patch (``transformer_options['saya_couple_crop']``, written
  by the local build's ``crop_model_patch.py:23-64``), activated ONLY for the
  duration of the delegated call (``_delegate_with_couple_crop_env``, env var
  ``SAYA_USDU_COUPLE_CROP`` set then restored, never leaked globally). The
  consumer (``src/ppm_vendor/attention_couple/unet_couple.py``,
  ``attn2_output_patch``) reads ``extra_options['saya_couple_crop']`` and
  calls ``attention_couple/common.py::crop_mask_to_tile`` (fractional slice
  canvas->mask FIRST, then NEAREST via ``reshape_mask`` -- never the reverse).
* ``restore_to_base`` (open decision, historical default True): resizes back
  to the input size in bicubic AFTER the pass -- False keeps the full-resolution
  chain. USDU1 must run with False so the continuation exists.
* sigmas OR (sampler, scheduler, steps, denoise) are EXCLUSIVE (validated by
  ``usdu_engine_config.validate_config_pass``); the ``cfg`` widget stays LIVE.
* seam fix: locked to "None" in v1 (``seam_fix_denoise`` is inert in this
  port) -- no widget is exposed for it.
* LIVE_GRID_BEHAVIOR = CEIL is imposed (``usdu_engine_config.live_grid``) --
  no grid parameter is exposed.
"""

from __future__ import annotations

import logging
from typing import Any

from .usdu_engine_config import (
    PASS_IDS,
    ConfigPass,
    SamplerFallback,
    SayaUSDUConfigError,
    canvas_size,
    live_grid,
    tile_work_size,
    to_engine_kwargs,
    validate_config_pass,
)

_UPSCALE_NODE_NAME = "UltimateSDUpscaleCustomSample"
_UPSCALE_INSTALL_HINT = (
    "Install or enable ComfyUI_UltimateSDUpscale "
    "(https://github.com/ssitu/ComfyUI_UltimateSDUpscale)."
)


def sampler_options() -> tuple[list[str], list[str]]:
    """Live sampler/scheduler names, with a no-ComfyUI fallback (pack pattern)."""
    try:
        from comfy.samplers import KSampler  # type: ignore

        return (list(KSampler.SAMPLERS), list(KSampler.SCHEDULERS))
    except (ImportError, AttributeError):  # pragma: no cover
        return (["euler", "euler_ancestral", "euler_cfg_pp"], ["normal", "beta", "karras", "simple"])


def _resolve_upscale_node() -> Any:
    """Lazy lookup of the installed engine (pattern from duo_geometry/nodes.py:49-73)."""
    try:
        import nodes as comfy_nodes  # type: ignore

        found = comfy_nodes.NODE_CLASS_MAPPINGS.get(_UPSCALE_NODE_NAME)
        if found is not None:
            return found
    except (ImportError, AttributeError):
        pass
    try:
        from usdu_nodes import UltimateSDUpscaleCustomSample  # type: ignore

        return UltimateSDUpscaleCustomSample
    except ImportError:
        raise ImportError(
            f"SayaCoupleUSDUPass needs {_UPSCALE_NODE_NAME}, which is not loaded. "
            f"{_UPSCALE_INSTALL_HINT}"
        ) from None


#: EXACT name of the env var read by crop_model_patch.py (layer 1, on the
#: ComfyUI_UltimateSDUpscale side) -- do not rename without updating that reader.
_SAYA_COUPLE_CROP_ENV = "SAYA_USDU_COUPLE_CROP"


def _couple_crop_gate_missing(node_class: Any) -> str | None:
    """Why the installed engine cannot honour the couple-crop gate, or None when it can.

    The gate lives in an optional patch of ComfyUI_UltimateSDUpscale
    (tools/core_patches/ultimate_sd_upscale_saya_couple_crop.patch in the release).
    A stock build ignores the variable and every tile then squashes the
    full-frame P1/P2 masks into itself.
    """
    from pathlib import Path

    # Not inspect.getfile: the USDU pack removes its modules from sys.modules after import.
    upscale = getattr(node_class, "upscale", None)
    if upscale is None:
        return f"{getattr(node_class, '__name__', node_class)!r} has no upscale method"
    reader = Path(upscale.__code__.co_filename).resolve().parent / "crop_model_patch.py"
    if not reader.is_file():
        return f"{reader} not found"
    if _SAYA_COUPLE_CROP_ENV not in reader.read_text(encoding="utf-8", errors="replace"):
        return f"{reader} does not read {_SAYA_COUPLE_CROP_ENV}"
    return None


def _delegate_with_couple_crop_env(node_class: Any, call: dict[str, Any], couple_crop: bool) -> Any:
    """Set the engine's couple-crop gate to ``couple_crop`` for this delegated call only.

    couple_crop=True: the saya_couple_crop metadata is written by the local
    build during THIS call and read by its consumer (attn2_output_patch).
    couple_crop=False: the gate is forced closed, even when the variable is
    exported globally by the user's shell. The previous value is restored
    even if the call raises.
    """
    import os

    if couple_crop:
        missing = _couple_crop_gate_missing(node_class)
        if missing:
            logging.warning(
                "[Saya Couple] USDU couple crop unavailable (%s): P1/P2 masks are applied full-frame to every tile. "
                "Apply the optional tools/core_patches/ultimate_sd_upscale_saya_couple_crop.patch from the Saya Couple pack (git apply inside custom_nodes/ComfyUI_UltimateSDUpscale).",
                missing,
            )
    previous = os.environ.get(_SAYA_COUPLE_CROP_ENV)
    if couple_crop:
        os.environ[_SAYA_COUPLE_CROP_ENV] = "1"
    else:
        os.environ.pop(_SAYA_COUPLE_CROP_ENV, None)
    try:
        return node_class().upscale(**call)
    finally:
        if previous is None:
            os.environ.pop(_SAYA_COUPLE_CROP_ENV, None)
        else:
            os.environ[_SAYA_COUPLE_CROP_ENV] = previous


class SayaCoupleUSDUPass:
    """One tiled USDU pass, config-explicit -- ONE implementation, N instances.

    Two instances of THIS node = USDU 1 + USDU 2 (no ``if pass == 2`` branch
    in the engine). Instance 2's image input must be instance 1's image
    output (wired in the workflow).
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        samplers, schedulers = sampler_options()
        fallback_tooltip = (
            "Fallback only: ignored when BOTH custom_sampler and custom_sigmas "
            "are connected (sigmas mode -- these widgets become inert)."
        )
        return {
            "required": {
                "image": ("IMAGE",),
                "model": ("MODEL", {
                    "tooltip": "model_patched from SayaCoupleReconstruct (couple patch "
                               "already applied once -- this pass' clone keeps it).",
                }),
                "positive": ("CONDITIONING", {
                    "tooltip": "Global MAIN (positive output of SayaCoupleReconstruct).",
                }),
                "negative": ("CONDITIONING",),
                "vae": ("VAE",),
                "seed": ("INT", {"default": 0, "min": 0, "max": 0xFFFFFFFFFFFFFFFF}),
                "pass_id": (list(PASS_IDS), {
                    "default": "usdu_1",
                    "tooltip": "Config identifier (informational only -- no "
                               "if pass==2 branch exists in the engine).",
                }),
                "upscale_by": ("FLOAT", {"default": 2.0, "min": 0.05, "max": 4.0, "step": 0.05}),
                "tile_size": ("INT", {"default": 512, "min": 64, "max": 8192, "step": 8}),
                "padding": ("INT", {"default": 32, "min": 0, "max": 8192, "step": 8,
                                    "tooltip": "Per-tile work size = round8(tile+padding) (local build)."}),
                "mask_blur": ("INT", {"default": 8, "min": 0, "max": 64, "step": 1}),
                "cfg": ("FLOAT", {"default": 4.0, "min": 0.0, "max": 100.0,
                                  "tooltip": "LIVE even in sigmas mode (SamplerCustom.execute)."}),
                "steps": ("INT", {"default": 12, "min": 1, "max": 10000, "step": 1, "tooltip": fallback_tooltip}),
                "sampler_name": (samplers, {"default": "euler_cfg_pp", "tooltip": fallback_tooltip}),
                "scheduler": (schedulers, {"default": "beta", "tooltip": fallback_tooltip}),
                "denoise": ("FLOAT", {"default": 0.2, "min": 0.0, "max": 1.0, "step": 0.01, "tooltip": fallback_tooltip}),
                "structure_preservation": ("FLOAT", {"default": 0.75, "min": 0.0, "max": 1.0, "step": 0.01}),
                "restore_to_base": ("BOOLEAN", {
                    "default": True,
                    "tooltip": "Open decision: True = output resized back to the input "
                               "size (bicubic, historical behaviour); False = keep the "
                               "full-resolution chain. USDU1 must be False (so the "
                               "USDU1->USDU2 continuation exists).",
                }),
                "solo": ("BOOLEAN", {
                    "forceInput": True,
                    "tooltip": "Global Couple/Solo mode (Solo output of the Couple Mode "
                               "toggle). Couple -> per-tile couple crop; Solo -> no crop.",
                }),
            },
            "optional": {
                "upscale_model": ("UPSCALE_MODEL", {
                    "tooltip": "OPTIONAL: absent -> internal Lanczos "
                               "(upscaler.py:8-19). Pixel-prior choice only.",
                }),
                "custom_sampler": ("SAMPLER",),
                "custom_sigmas": ("SIGMAS",),
                "mask_base": ("MASK", {"forceInput": True}),
                "mask_p1": ("MASK", {"forceInput": True}),
                "mask_p2": ("MASK", {"forceInput": True}),
                "base_image": ("IMAGE", {
                    "tooltip": "Target of restore_to_base = the PHASE image (W0xH0), "
                               "NOT this pass' input. Needed on USDU2, whose input IS "
                               "already USDU1's 2W0x2H0 output (otherwise "
                               "restore_to_base=True would be a silent no-op there). "
                               "Absent -> falls back to `image` (correct for USDU1, "
                               "whose input IS the phase size).",
                }),
            },
        }

    RETURN_TYPES = ("IMAGE", "INT", "INT")
    RETURN_NAMES = ("image", "width", "height")
    FUNCTION = "upscale"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Config-explicit tiled USDU pass. LIVE grid = CEIL is imposed; "
        "per-tile work size = round8(tile+padding); couple crop = not solo; "
        "continuation USDU1->USDU2 through direct wiring (asserted at the builder)."
    )

    def upscale(
        self,
        image: Any,
        model: Any,
        positive: Any,
        negative: Any,
        vae: Any,
        seed: int,
        pass_id: str = "usdu_1",
        upscale_by: float = 2.0,
        tile_size: int = 512,
        padding: int = 32,
        mask_blur: int = 8,
        cfg: float = 4.0,
        steps: int = 12,
        sampler_name: str = "euler_cfg_pp",
        scheduler: str = "beta",
        denoise: float = 0.2,
        structure_preservation: float = 0.75,
        restore_to_base: bool = True,
        solo: bool = False,
        upscale_model: Any = None,
        custom_sampler: Any = None,
        custom_sigmas: Any = None,
        mask_base: Any = None,
        mask_p1: Any = None,
        mask_p2: Any = None,
        base_image: Any = None,
    ) -> tuple[Any, ...]:
        # -- explicit config_pass, validated BEFORE any work -----------------------
        mode_sigmas = custom_sigmas is not None
        config = ConfigPass(
            pass_id=pass_id,
            cfg=float(cfg),
            upscale_by=float(upscale_by),
            tile_size=int(tile_size),
            padding=int(padding),
            mask_blur=int(mask_blur),
            structure_preservation=float(structure_preservation),
            couple_crop=not bool(solo),
            restore_to_base=bool(restore_to_base),
            seam_fix="None",
            upscale_model=None,  # runtime object, not data -- see the engine note below
            sigmas=custom_sigmas if mode_sigmas else None,
            sampler_fallback=None if mode_sigmas else SamplerFallback(
                sampler_name, scheduler, int(steps), float(denoise)
            ),
            inert_widget_fallback=SamplerFallback(
                sampler_name, scheduler, int(steps), float(denoise)
            ) if mode_sigmas else None,
        )
        config = validate_config_pass(config)

        # -- Couple mode: the three masks are a wiring guard only; this node
        # never reads their tensor values. The actual crop is done by the
        # engine against the couple mask already carried by the cloned
        # MODEL's patch (see the module docstring).
        if config.couple_crop and (mask_base is None or mask_p1 is None or mask_p2 is None):
            raise SayaUSDUConfigError(
                "Couple mode needs mask_base/mask_p1/mask_p2 connected "
                "(SayaCoupleRegionMasks) as a wiring guard for the per-tile "
                "couple crop, even though this node does not read their values."
            )

        # -- continuation / resolution ---------------------------------------------
        shape = getattr(image, "shape", None)
        if shape is None or len(shape) != 4:
            raise SayaUSDUConfigError(f"image: expected a 4D tensor, got {shape!r}")
        _batch, in_height, in_width, _channels = (int(size) for size in shape)

        # Target of restore_to_base = the PHASE image (W0xH0), NOT necessarily
        # `image` (which IS already the previous pass' output on USDU2 --
        # without an explicit base_image, restoring towards `image` would be
        # a silent no-op).
        base_shape = getattr(base_image, "shape", None) if base_image is not None else None
        if base_shape is not None:
            if len(base_shape) != 4:
                raise SayaUSDUConfigError(f"base_image: expected a 4D tensor, got {base_shape!r}")
            _bb, base_height, base_width, _bc = (int(size) for size in base_shape)
        else:
            base_height, base_width = in_height, in_width

        # -- engine delegation (pack pattern; kw-only kwargs) -----------------------
        node_class = _resolve_upscale_node()
        clone = model.clone() if hasattr(model, "clone") else model
        call = to_engine_kwargs(config)
        call.update({
            "image": image,
            "model": clone,
            "positive": positive,
            "negative": negative,
            "vae": vae,
            "seed": int(seed),
            "upscale_model": upscale_model,   # runtime object -- pixel prior
            "custom_sampler": custom_sampler,
            "custom_sigmas": custom_sigmas,
        })
        # The imposed LIVE grid (CEIL) is recomputed here only for the report:
        # the engine sets it itself (ultimate-upscale.py:39-40).
        # Fixed reporting bug: the REAL canvas the engine processes is
        # round8(in x upscale_by), NOT the raw `in_width/in_height` (wrong for
        # upscale_by != 1 -- e.g. USDU1 used to report 1x2/2 tiles while the
        # engine actually processed 2x3/6 tiles on the x2 canvas).
        canvas_w, canvas_h = canvas_size(in_width, in_height, config.upscale_by)
        rows, cols = live_grid(canvas_w, canvas_h, config.tile_size)

        # Only activates the engine's gate (saya_couple_crop metadata, in
        # ComfyUI_UltimateSDUpscale's crop_model_patch.py layer) for THIS
        # execution, and only if couple_crop=ON -- never leaked globally to
        # a concurrent or later execution.
        result = _delegate_with_couple_crop_env(node_class, call, config.couple_crop)
        upscaled = result[0]

        # -- restore_to_base: bicubic back to the PHASE size (base_image) ----------
        out_height, out_width = int(upscaled.shape[1]), int(upscaled.shape[2])
        if config.restore_to_base and (out_width, out_height) != (base_width, base_height):
            import torch  # local: only loads torch at actual execution time
            import torch.nn.functional as torch_functional

            restored = torch_functional.interpolate(
                upscaled.movedim(-1, 1), size=(base_height, base_width), mode="bicubic",
                align_corners=False,
            ).movedim(1, -1).clamp(0.0, 1.0)
            upscaled = restored.to(device=upscaled.device, dtype=upscaled.dtype)
            out_height, out_width = base_height, base_width

        # Report via ComfyUI's stdout (no additional output socket for it).
        print(
            f"[SayaCoupleUSDUPass {config.pass_id}] grid={rows}x{cols} (CEIL live, "
            f"{rows * cols} tiles) work={tile_work_size(tile_size, padding)} "
            f"cfg={config.cfg} couple_crop={config.couple_crop} "
            f"restore_to_base={config.restore_to_base} out={out_width}x{out_height}"
        )
        return (upscaled, out_width, out_height)

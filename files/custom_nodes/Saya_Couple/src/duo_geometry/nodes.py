"""ComfyUI node for the tiled-upscale pass.

``SayaDuoTiledUpscale`` replaces the two legacy identity-safe upscale
bridges. It delegates every pixel to the installed Ultimate SD Upscale
pack -- tiling, seam fixing and compositing stay in exactly one
implementation -- and adds only what the legacy bridge really contributed:

* it clones the MODEL instead of mutating the caller's;
* it discards any geometry inherited from an earlier pass, because tile
  coordinates from an upscale pass have nothing to do with the crop
  coordinates a detailer produced at another resolution.

The legacy bridge expressed the last point by writing flags into a
globally patched model. This one does not: nothing is shared between
passes.
"""

from __future__ import annotations

from typing import Any

from .shape import GeometryError, image_dimensions

# Tiling and seam-fix option names, mirrored from the Ultimate SD Upscale
# node so the COMBO values in a saved workflow keep resolving. The node is
# lazy-imported at execution time, so these must be declarable without it.
REDRAW_MODES = ("Linear", "Chess", "None")
SEAM_FIX_MODES = ("None", "Band Pass", "Half Tile", "Half Tile + Intersections")

_UPSCALE_NODE_NAME = "UltimateSDUpscaleCustomSample"
_UPSCALE_INSTALL_HINT = (
    "Install or enable ComfyUI_UltimateSDUpscale "
    "(https://github.com/ssitu/ComfyUI_UltimateSDUpscale)."
)

MAX_TILE = 8192


def sampler_options() -> tuple[list[str], list[str]]:
    """Live sampler/scheduler names, with a no-ComfyUI fallback."""
    try:
        from comfy.samplers import KSampler  # type: ignore

        return (list(KSampler.SAMPLERS), list(KSampler.SCHEDULERS))
    except (ImportError, AttributeError):
        return (["euler", "euler_ancestral"], ["normal", "karras", "beta", "simple"])


def _resolve_upscale_node() -> Any:
    """Lazily fetch the installed upscale node class.

    Resolved through ComfyUI's global mappings so it is found after all
    custom nodes have loaded, with a direct import as a fallback for
    unit tests. Never imported at module scope: this pack must still
    register when the upscale pack is absent.
    """
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
            f"Saya Duo Tiled Upscale needs {_UPSCALE_NODE_NAME}, which is not "
            f"loaded. {_UPSCALE_INSTALL_HINT}"
        ) from None


def _supports_structure_preservation(node_class: Any) -> bool:
    schema = node_class.INPUT_TYPES()
    return "structure_preservation" in schema.get("optional", {})


def _validate_upscale_modes(mode_type: str, seam_fix_mode: str) -> None:
    """Reject a COMBO value that does not match this pack's declared choices."""
    if mode_type not in REDRAW_MODES:
        raise GeometryError(f"unknown mode_type {mode_type!r}, expected one of {REDRAW_MODES}")
    if seam_fix_mode not in SEAM_FIX_MODES:
        raise GeometryError(
            f"unknown seam_fix_mode {seam_fix_mode!r}, expected one of {SEAM_FIX_MODES}"
        )


def _resolve_usdu(structure_preservation: float) -> tuple[Any, bool]:
    """Fetch the installed Ultimate SD Upscale node and its capabilities.

    Returns ``(node_class, supports_structure_preservation)``.
    """
    node_class = _resolve_upscale_node()
    # Each probe rebuilds the upstream node's whole INPUT_TYPES schema, and
    # the answer cannot change within one call: resolve it once.
    supports_structure = _supports_structure_preservation(node_class)
    if structure_preservation and not supports_structure:
        raise GeometryError(
            "structure_preservation > 0 needs an Ultimate SD Upscale build that "
            f"exposes it; the installed {_UPSCALE_NODE_NAME} does not. Set it to "
            "0 to run the stock behaviour."
        )
    return node_class, supports_structure


def _usdu_call_kwargs(
    *,
    image: Any,
    clone: Any,
    positive: Any,
    negative: Any,
    vae: Any,
    upscale_by: float,
    seed: int,
    steps: int,
    cfg: float,
    sampler_name: str,
    scheduler: str,
    denoise: float,
    mode_type: str,
    tile_width: int,
    tile_height: int,
    mask_blur: int,
    tile_padding: int,
    seam_fix_mode: str,
    seam_fix_denoise: float,
    seam_fix_width: int,
    seam_fix_mask_blur: int,
    seam_fix_padding: int,
    force_uniform_tiles: bool,
    tiled_decode: bool,
    batch_size: int,
    upscale_model: Any,
    custom_sampler: Any,
    custom_sigmas: Any,
    structure_preservation: float,
    supports_structure: bool,
) -> dict[str, Any]:
    """Translate this node's own inputs into the upstream node's kwargs."""
    call: dict[str, Any] = {
        "image": image,
        "model": clone,
        "positive": positive,
        "negative": negative,
        "vae": vae,
        "upscale_by": upscale_by,
        "seed": seed,
        "steps": steps,
        "cfg": cfg,
        "sampler_name": sampler_name,
        "scheduler": scheduler,
        "denoise": denoise,
        "mode_type": mode_type,
        "tile_width": tile_width,
        "tile_height": tile_height,
        "mask_blur": mask_blur,
        "tile_padding": tile_padding,
        "seam_fix_mode": seam_fix_mode,
        "seam_fix_denoise": seam_fix_denoise,
        "seam_fix_width": seam_fix_width,
        "seam_fix_mask_blur": seam_fix_mask_blur,
        "seam_fix_padding": seam_fix_padding,
        "force_uniform_tiles": force_uniform_tiles,
        "tiled_decode": tiled_decode,
        "batch_size": batch_size,
        "upscale_model": upscale_model,
        "custom_sampler": custom_sampler,
        "custom_sigmas": custom_sigmas,
    }
    if supports_structure:
        call["structure_preservation"] = structure_preservation
    return call


class SayaDuoTiledUpscale:
    """Tiled image-to-image upscale."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        samplers, schedulers = sampler_options()
        fallback = (
            "Fallback only: ignored when BOTH custom_sampler and custom_sigmas "
            "are connected."
        )
        return {
            "required": {
                "image": ("IMAGE",),
                "model": ("MODEL",),
                "positive": ("CONDITIONING",),
                "negative": ("CONDITIONING",),
                "vae": ("VAE",),
                "upscale_by": ("FLOAT", {"default": 2.0, "min": 0.05, "max": 4.0, "step": 0.05}),
                "seed": ("INT", {"default": 0, "min": 0, "max": 0xFFFFFFFFFFFFFFFF}),
                "steps": ("INT", {"default": 20, "min": 1, "max": 10000, "step": 1, "tooltip": fallback}),
                "cfg": ("FLOAT", {"default": 8.0, "min": 0.0, "max": 100.0}),
                "sampler_name": (samplers, {"tooltip": fallback}),
                "scheduler": (schedulers, {"tooltip": fallback}),
                "denoise": ("FLOAT", {"default": 0.2, "min": 0.0, "max": 1.0, "step": 0.01, "tooltip": fallback}),
                "mode_type": (list(REDRAW_MODES),),
                "tile_width": ("INT", {"default": 512, "min": 64, "max": MAX_TILE, "step": 8}),
                "tile_height": ("INT", {"default": 512, "min": 64, "max": MAX_TILE, "step": 8}),
                "mask_blur": ("INT", {"default": 8, "min": 0, "max": 64, "step": 1}),
                "tile_padding": ("INT", {"default": 32, "min": 0, "max": MAX_TILE, "step": 8}),
                "seam_fix_mode": (list(SEAM_FIX_MODES),),
                "seam_fix_denoise": ("FLOAT", {"default": 1.0, "min": 0.0, "max": 1.0, "step": 0.01, "tooltip": "Ignored while seam_fix_mode is None."}),
                "seam_fix_width": ("INT", {"default": 64, "min": 0, "max": MAX_TILE, "step": 8, "tooltip": "Only used by the Band Pass seam fix modes; ignored otherwise."}),
                "seam_fix_mask_blur": ("INT", {"default": 8, "min": 0, "max": 64, "step": 1, "tooltip": "Ignored while seam_fix_mode is None."}),
                "seam_fix_padding": ("INT", {"default": 16, "min": 0, "max": MAX_TILE, "step": 8, "tooltip": "Ignored while seam_fix_mode is None."}),
                "force_uniform_tiles": ("BOOLEAN", {"default": True}),
                "tiled_decode": ("BOOLEAN", {"default": False}),
                "batch_size": ("INT", {"default": 1, "min": 1, "max": 4096, "step": 1}),
            },
            "optional": {
                "structure_preservation": ("FLOAT", {"default": 0.75, "min": 0.0, "max": 1.0, "step": 0.01}),
                "upscale_model": ("UPSCALE_MODEL",),
                "custom_sampler": ("SAMPLER",),
                "custom_sigmas": ("SIGMAS",),
            },
        }

    RETURN_TYPES = ("IMAGE", "INT", "INT")
    RETURN_NAMES = ("image", "width", "height")
    FUNCTION = "upscale"
    CATEGORY = "Saya"
    DESCRIPTION = (
        "Tiled image-to-image upscale delegated to Ultimate SD Upscale, on a "
        "MODEL clone so the caller's model is never mutated."
    )

    def upscale(
        self,
        image: Any,
        model: Any,
        positive: Any,
        negative: Any,
        vae: Any,
        upscale_by: float,
        seed: int,
        steps: int,
        cfg: float,
        sampler_name: str,
        scheduler: str,
        denoise: float,
        mode_type: str,
        tile_width: int,
        tile_height: int,
        mask_blur: int,
        tile_padding: int,
        seam_fix_mode: str,
        seam_fix_denoise: float,
        seam_fix_width: int,
        seam_fix_mask_blur: int,
        seam_fix_padding: int,
        force_uniform_tiles: bool,
        tiled_decode: bool,
        batch_size: int,
        structure_preservation: float = 0.75,
        upscale_model: Any = None,
        custom_sampler: Any = None,
        custom_sigmas: Any = None,
    ) -> tuple[Any, ...]:

        image_dimensions(image)
        _validate_upscale_modes(mode_type, seam_fix_mode)
        node_class, supports_structure = _resolve_usdu(structure_preservation)

        # Never mutate the caller's MODEL: this pass gets its own clone.
        clone = model.clone() if hasattr(model, "clone") else model

        call = _usdu_call_kwargs(
            image=image,
            clone=clone,
            positive=positive,
            negative=negative,
            vae=vae,
            upscale_by=upscale_by,
            seed=seed,
            steps=steps,
            cfg=cfg,
            sampler_name=sampler_name,
            scheduler=scheduler,
            denoise=denoise,
            mode_type=mode_type,
            tile_width=tile_width,
            tile_height=tile_height,
            mask_blur=mask_blur,
            tile_padding=tile_padding,
            seam_fix_mode=seam_fix_mode,
            seam_fix_denoise=seam_fix_denoise,
            seam_fix_width=seam_fix_width,
            seam_fix_mask_blur=seam_fix_mask_blur,
            seam_fix_padding=seam_fix_padding,
            force_uniform_tiles=force_uniform_tiles,
            tiled_decode=tiled_decode,
            batch_size=batch_size,
            upscale_model=upscale_model,
            custom_sampler=custom_sampler,
            custom_sigmas=custom_sigmas,
            structure_preservation=structure_preservation,
            supports_structure=supports_structure,
        )
        # Keywords only: the upstream node's own INPUT_TYPES order and its
        # method signature disagree on seam_fix_width vs seam_fix_mask_blur,
        # so a positional call would silently swap them.
        result = node_class().upscale(**call)
        upscaled = result[0]

        _batch, height, width, _channels = image_dimensions(upscaled)
        return (upscaled, width, height)

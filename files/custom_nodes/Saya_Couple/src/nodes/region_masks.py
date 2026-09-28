"""Saya region masks v2 -- pure torch, no ComfyUI import required.

Geometry source: ``imprint.couple_imprint.geometry`` -- ``regions`` (fractions
normalized against the reference frame, top-left origin) is the ground truth;
derived fields are re-asserted on READ via
``saya_couple_imprint_v2.assert_geometry_coherence`` (one single definition of
coherence, shared by WRITE and READ).

Frozen conventions:

* white (1.0) = active; the BASE mask covers the full frame (MAIN weight),
  P1/P2 carry the person weight -- amplitudes are normalized DOWNSTREAM by the
  PPM (sum > 0 required), never here.
* rasterized at the CURRENT pass' resolution -- no raster is ever stored or
  carried between passes; the imprint's reference frame is only used to
  compute the fractions.
* NEAREST for the attention raster (consistent with the vendor's /8
  downsample in ``common.py:24``); the image compositing side stays BICUBIC
  in the engine (``processing.py:387-388,443-457`` -- documented convention,
  not reimplemented here).
* shapes are (B,H,W) at the interface; P2 absent -> None (never fabricated).
* feather: TWO non-comparable semantics, selected by ``geometry.feather_unit``:
  - "axis_fraction": C1 smoothstep in axis fraction CENTERED on the edge
    (crosses 0.5 exactly at the edge; frame edges stay HARD; floor residual)
    -- adapted from the REM rasterizer ``gen1/masks.py``;
  - "pixel_sigma": Gaussian blur with sigma in raster PIXELS, edges may bleed
    through replicate padding -- adapted from ``saya_split_mask.py:34-47``.
  At feather 0.0 both produce the same hard rectangle.

Per-tile couple crop: ``mask_for_tile`` / ``mask_for_tile_work`` slice the
REAL region of the full-image mask for each tile (the full P1->P2 gradient is
NEVER resampled inside a tile). The engine consumer (``saya_couple_crop``
metadata, ``crop_model_patch.py:14-26``) must use exactly this path: slice
first, resize NEAREST second.

Engine LIVE grid: LIVE_GRID_BEHAVIOR = CEIL, imposed by the engine -- see
``usdu_engine_config.live_grid``; ``tile_windows`` follows the same
convention.
"""

from __future__ import annotations

import math
from enum import Enum
from typing import Any

import torch
import torch.nn.functional as functional

from .couple_imprint_v2 import (
    FEATHER_AXIS_FRACTION,
    FEATHER_PIXEL_SIGMA,
    assert_geometry_coherence,
)


class SayaMaskError(ValueError):
    """Raised for an unusable grid, geometry or mask control."""


class Orientation(Enum):
    """Orientation stated in terms of the resulting REGIONS. Never vertical/horizontal."""

    LEFT_RIGHT = "LEFT_RIGHT"
    TOP_BOTTOM = "TOP_BOTTOM"

    @classmethod
    def from_geometry(cls, geometry: dict[str, Any]) -> "Orientation":
        value = geometry.get("derived", {}).get("orientation")
        try:
            return cls(str(value))
        except ValueError:
            raise SayaMaskError(
                f"invalid orientation {value!r} -- LEFT_RIGHT/TOP_BOTTOM only"
            ) from None


_LEGACY_HINT = (
    "legacy words are forbidden here -- historical mapping: SplitMask "
    "'vertical'=LEFT_RIGHT ; PPMMasks 'vertical'=TOP_BOTTOM, 'horizontal'=LEFT_RIGHT"
)


def _check_grid(height: int, width: int) -> None:
    if isinstance(height, bool) or isinstance(width, bool) \
            or not isinstance(height, int) or not isinstance(width, int):
        raise SayaMaskError(
            f"mask grid must be ints, got {type(height).__name__}x{type(width).__name__}"
        )
    if height < 1 or width < 1:
        raise SayaMaskError(f"mask grid must be positive, got {height}x{width}")


def _check_unit(value: Any, name: str, *, high: float = 1.0) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise SayaMaskError(f"{name} must be a number, got {value!r}")
    value = float(value)
    if value != value or value in (float("inf"), float("-inf")) or value < 0.0 or value > high:
        raise SayaMaskError(f"{name} must be finite in [0, {high}], got {value}")
    return value


def _smoothstep(values: torch.Tensor) -> torch.Tensor:
    """C1-continuous 0->1 ramp; flat at both ends (adapted from REM masks.py:86-89)."""
    clamped = values.clamp(0.0, 1.0)
    return clamped * clamped * (3.0 - 2.0 * clamped)


def _axis_profile(
    size: int,
    low: float,
    high: float,
    transition: float,
    *,
    device: torch.device,
    dtype: torch.dtype,
) -> torch.Tensor:
    """1-D coverage of [low, high] at pixel centres, smoothstep centered on the edge.

    An edge that sits on the frame stays HARD (there is nothing outside the
    frame to blend with). Adapted from REM src/gen1/masks.py:92-124 --
    "axis_fraction" semantics.
    """
    coordinate = (torch.arange(size, device=device, dtype=torch.float32) + 0.5) / size
    if transition <= 0.0:
        return ((coordinate >= low) & (coordinate < high)).to(dtype)
    half = transition / 2.0
    ones = torch.ones_like(coordinate)
    rising = ones if low <= 0.0 else _smoothstep((coordinate - (low - half)) / transition)
    falling = (
        ones if high >= 1.0 else 1.0 - _smoothstep((coordinate - (high - half)) / transition)
    )
    return torch.minimum(rising, falling).to(dtype)


def gaussian_pixel_sigma_blur(mask: torch.Tensor, sigma: float) -> torch.Tensor:
    """Gaussian blur, sigma in raster PIXELS -- adapted from saya_split_mask.py:34-47.

    "pixel_sigma" semantics: support truncated at 3 sigma, replicate padding
    (frame edges may bleed), clamped to 0..1. Accepts (H,W) or (B,H,W) and
    returns the same shape.
    """
    sigma = float(sigma)
    if sigma <= 0.0:
        return mask
    squeeze_back = mask.ndim == 2
    work = mask.unsqueeze(0).unsqueeze(0) if squeeze_back else mask.unsqueeze(1)
    radius = max(1, int(math.ceil(3.0 * sigma)))
    coordinates = torch.arange(-radius, radius + 1, dtype=torch.float32, device=mask.device)
    kernel_1d = torch.exp(-(coordinates * coordinates) / (2.0 * sigma * sigma))
    kernel_1d /= kernel_1d.sum()
    kernel_2d = torch.outer(kernel_1d, kernel_1d)[None, None, :, :]
    padded = functional.pad(work, (radius, radius, radius, radius), mode="replicate")
    blurred = functional.conv2d(padded, kernel_2d).clamp(0.0, 1.0)
    return blurred.squeeze(0).squeeze(0) if squeeze_back else blurred.squeeze(1)


def region_mask_rect(
    region: dict[str, float],
    height: int,
    width: int,
    feather: float = 0.0,
    feather_unit: str = FEATHER_AXIS_FRACTION,
    floor: float = 0.0,
    *,
    device: torch.device | None = None,
    dtype: torch.dtype = torch.float32,
) -> torch.Tensor:
    """Rasterize ONE normalized rectangle into [height, width] (orientation-agnostic).

    The rectangle itself carries the orientation: this function knows neither
    "vertical" nor "horizontal". White (1.0) = active; ``floor`` (< 0.5)
    leaves a residual presence outside the region (REM semantics, inert at 0.0).
    """
    _check_grid(height, width)
    feather = _check_unit(feather, "feather", high=1.0)
    if not (0.0 <= float(floor) < 0.5):
        raise SayaMaskError(f"mask floor must be in [0, 0.5), got {floor!r}")
    floor = float(floor)
    device = device or torch.device("cpu")
    x0, y0 = float(region["x0"]), float(region["y0"])
    x1, y1 = float(region["x1"]), float(region["y1"])

    if feather_unit == FEATHER_AXIS_FRACTION:
        columns = _axis_profile(width, x0, x1, feather, device=device, dtype=torch.float32)
        rows = _axis_profile(height, y0, y1, feather, device=device, dtype=torch.float32)
        shape = rows.view(height, 1) * columns.view(1, width)
        return (floor + (1.0 - floor) * shape).to(dtype)
    if feather_unit == FEATHER_PIXEL_SIGMA:
        if feather > 1.0:
            raise SayaMaskError(
                f"feather {feather} out of range 0..1 for the pixel_sigma unit "
                "(sigma is normalized by the reference size)"
            )
        xs = torch.arange(width, device=device, dtype=torch.float32)
        ys = torch.arange(height, device=device, dtype=torch.float32)
        inside_x = (xs + 0.5) / width
        inside_y = (ys + 0.5) / height
        columns = ((inside_x >= x0) & (inside_x < x1)).to(dtype=torch.float32)
        rows = ((inside_y >= y0) & (inside_y < y1)).to(dtype=torch.float32)
        binary = rows.view(height, 1) * columns.view(1, width)
        # sigma is expressed in pixels of the REFERENCE grid: rescale to the
        # current grid so the feather stays proportional (geometry lives in
        # normalized units, never in absolute pixels).
        sigma_px = feather * max(width, height)
        blurred = gaussian_pixel_sigma_blur(binary, sigma_px)
        return (floor + (1.0 - floor) * blurred).to(dtype)
    raise SayaMaskError(
        f"invalid feather_unit {feather_unit!r} -- axis_fraction|pixel_sigma "
        f"only ({_LEGACY_HINT})"
    )


def regions_from_geometry(geometry: dict[str, Any]) -> tuple[dict[str, float], dict[str, float] | None]:
    """Extract (region_1, region_2|None), asserting geometry coherence on READ."""
    assert_geometry_coherence(geometry, context="masks.geometry(read)")
    regions = geometry["regions"]
    return regions["person_1"], regions.get("person_2")


def to_batch(mask_hw: torch.Tensor, batch: int) -> torch.Tensor:
    """(H,W) -> (B,H,W) contiguous. Batch >= 1, same geometry per item."""
    if isinstance(batch, bool) or not isinstance(batch, int) or batch < 1:
        raise SayaMaskError(f"batch must be a positive int, got {batch!r}")
    if mask_hw.ndim != 2:
        raise SayaMaskError(f"expected (H,W) mask, got shape {tuple(mask_hw.shape)}")
    return mask_hw.unsqueeze(0).expand(batch, *mask_hw.shape).contiguous()


def attention_weights(
    imprint: dict[str, Any],
    *,
    base_weight: float | None = None,
    person_weight: float | None = None,
) -> tuple[float, float]:
    """(base, person) weights -- from the imprint's ``reconstruction_recipe.attention_params``
    (archaeological 0.7/0.3 fallback if the block is somehow absent) unless explicitly overridden.
    Shared by ``derive_masks`` (PPM amplitude) and ``derive_raw_region_masks`` (MultiMaskCouple
    ``mask_strength``) -- one single reading of the imprint's weights for both consumers.
    """
    params = imprint.get("reconstruction_recipe", {}).get("attention_params", {}) if isinstance(imprint, dict) else {}
    base = float(base_weight if base_weight is not None else params.get("base_weight", 0.7))
    person = float(person_weight if person_weight is not None else params.get("person_weight", 0.3))
    return base, person


def _rasterize_regions(
    imprint: dict[str, Any],
    height: int,
    width: int,
    *,
    device: torch.device | None,
    dtype: torch.dtype,
) -> tuple[torch.Tensor, torch.Tensor | None]:
    """Geometry imprint -> (mask_1, mask_2|None) UNWEIGHTED (H,W) rects, feather applied.

    Shared rasterization step for both ``derive_masks`` (PPM amplitude, weight baked
    into the mask) and ``derive_raw_region_masks`` (MultiMaskCouple, weight carried on
    the CONDITIONING instead -- see ``saya_multi_couple.py``).
    """
    couple = imprint.get("couple_imprint") if isinstance(imprint, dict) else None
    if not isinstance(couple, dict) or "geometry" not in couple:
        raise SayaMaskError("expected a v2 imprint (couple_imprint.geometry block)")
    geometry = couple["geometry"]
    feather = float(geometry.get("feather", 0.0))
    unit = geometry.get("feather_unit")
    floor = float(geometry.get("mask_floor", 0.0))
    region_1, region_2 = regions_from_geometry(geometry)
    mask_1 = region_mask_rect(region_1, height, width, feather, unit, floor, device=device, dtype=dtype)
    mask_2 = None
    if region_2 is not None:
        mask_2 = region_mask_rect(region_2, height, width, feather, unit, floor, device=device, dtype=dtype)
    return mask_1, mask_2


def derive_masks(
    imprint: dict[str, Any],
    height: int,
    width: int,
    batch: int = 1,
    *,
    base_weight: float | None = None,
    person_weight: float | None = None,
    device: torch.device | None = None,
    dtype: torch.dtype = torch.float32,
) -> "SayaRegionMasks":
    """Geometry imprint -> masks (B,H,W) at the CURRENT pass' resolution.

    ``base`` = full frame (MAIN weight), ``person_1``/``person_2`` = person
    weight within their region -- weight is BAKED INTO the mask amplitude here
    (the PPM/``SayaAttentionCouplePPM`` contract). For the MultiMaskCouple
    reconstruction path (``couple_reconstruct.py``), use
    ``derive_raw_region_masks`` instead -- weight travels on the CONDITIONING
    there, never on the mask.
    """
    _check_grid(height, width)
    base, person = attention_weights(imprint, base_weight=base_weight, person_weight=person_weight)
    mask_1, mask_2 = _rasterize_regions(imprint, height, width, device=device, dtype=dtype)
    mask_1 = mask_1 * person
    if mask_2 is not None:
        mask_2 = mask_2 * person
    base_mask = torch.full((height, width), base, dtype=dtype, device=device or torch.device("cpu"))
    return SayaRegionMasks(
        base=to_batch(base_mask, batch),
        person_1=to_batch(mask_1, batch),
        person_2=to_batch(mask_2, batch) if mask_2 is not None else None,
        height=height,
        width=width,
    )


def derive_raw_region_masks(
    imprint: dict[str, Any],
    height: int,
    width: int,
    batch: int = 1,
    *,
    device: torch.device | None = None,
    dtype: torch.dtype = torch.float32,
) -> tuple[torch.Tensor, torch.Tensor | None]:
    """Geometry imprint -> UNWEIGHTED (B,H,W) region masks (mask_1, mask_2|None).

    The MultiMaskCouple contract (``saya_multi_couple.py::apply_multimask_couple``):
    the mask carries only the region's geometry (amplitude ~1 inside, feathered
    edge), never the base/person weight -- that weight is applied as the
    ``ConditioningSetMask`` strength on MAIN/person conditioning instead, exactly
    like Phase 1's ``SayaMultiCouple.apply()``. ``person_2`` is None when the
    imprint has no P2 (never fabricated).
    """
    _check_grid(height, width)
    mask_1, mask_2 = _rasterize_regions(imprint, height, width, device=device, dtype=dtype)
    return (
        to_batch(mask_1, batch),
        to_batch(mask_2, batch) if mask_2 is not None else None,
    )


class SayaRegionMasks:
    """A pass' masks: (B,H,W), white = active, P2 is None when absent."""

    def __init__(self, base: torch.Tensor, person_1: torch.Tensor,
                 person_2: torch.Tensor | None, height: int, width: int) -> None:
        for name, mask in (("base", base), ("person_1", person_1)):
            if mask.ndim != 3 or mask.shape[1:] != (height, width):
                raise SayaMaskError(
                    f"{name} must be (B,{height},{width}), got {tuple(mask.shape)}"
                )
        if person_2 is not None and (person_2.ndim != 3 or person_2.shape[1:] != (height, width)):
            raise SayaMaskError(
                f"person_2 must be (B,{height},{width}), got {tuple(person_2.shape)}"
            )
        self.base = base
        self.person_1 = person_1
        self.person_2 = person_2
        self.height = height
        self.width = width

    def as_ppm_inputs(self) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor | None]:
        """(base, p1, p2|None) as (B,H,W) -- the PPM patch's interface."""
        return self.base, self.person_1, self.person_2


# ---------------------------------------------------------------------------
# Tile windows (LIVE grid = CEIL) and per-tile mask
# ---------------------------------------------------------------------------

def tile_windows(width: int, height: int, tile: int) -> list["TileWindow"]:
    """All tile windows on the LIVE CEIL grid (rows x cols).

    Fixed convention: (rows, cols) = (ceil(H/tile), ceil(W/tile)) -- the test
    case 1792x2304/512 gives rows=5, cols=4, 20 tiles total (the tqdm
    round-based grid would only display 16; it is NOT the grid that actually
    executes).
    """
    _check_grid(height, width)
    if isinstance(tile, bool) or not isinstance(tile, int) or tile < 1:
        raise SayaMaskError(f"tile must be a positive int, got {tile!r}")
    windows: list[TileWindow] = []
    rows = math.ceil(height / tile)
    cols = math.ceil(width / tile)
    for row in range(rows):
        for col in range(cols):
            windows.append(TileWindow(
                x0=col * tile,
                y0=row * tile,
                x1=min((col + 1) * tile, width),
                y1=min((row + 1) * tile, height),
                row=row,
                col=col,
            ))
    return windows


class TileWindow:
    """Pixel window [x0,x1) x [y0,y1) of a tile on the current canvas."""

    def __init__(self, x0: int, y0: int, x1: int, y1: int, row: int = 0, col: int = 0) -> None:
        for name, value in (("x0", x0), ("y0", y0), ("x1", x1), ("y1", y1)):
            if isinstance(value, bool) or not isinstance(value, int):
                raise SayaMaskError(f"tile {name} must be int, got {value!r}")
        if x1 <= x0 or y1 <= y0 or x0 < 0 or y0 < 0:
            raise SayaMaskError(f"invalid tile window x[{x0}:{x1}] y[{y0}:{y1}]")
        self.x0, self.y0, self.x1, self.y1 = x0, y0, x1, y1
        self.row, self.col = row, col

    @property
    def width(self) -> int:
        return self.x1 - self.x0

    @property
    def height(self) -> int:
        return self.y1 - self.y0


def mask_for_tile(mask_hw: torch.Tensor, window: TileWindow) -> torch.Tensor:
    """The REAL slice of the full-image mask for one canvas window."""
    if mask_hw.ndim != 2:
        raise SayaMaskError(f"expected (H,W) mask, got {tuple(mask_hw.shape)}")
    if window.x1 > mask_hw.shape[1] or window.y1 > mask_hw.shape[0]:
        raise SayaMaskError(
            f"tile x[{window.x0}:{window.x1}] y[{window.y0}:{window.y1}] does not fit "
            f"mask {tuple(mask_hw.shape)} -- the window belongs to another resolution"
        )
    return mask_hw[window.y0:window.y1, window.x0:window.x1].contiguous()


def padded_tile_window(window: TileWindow, padding: int, canvas_w: int, canvas_h: int) -> TileWindow:
    """The engine's CROP window: core tile + padding band, clamped to the canvas.

    Mirrors ``get_crop_region`` (``usdu_utils.py:50-63``): padding is added on
    EACH side of the white rectangle, then clamped -- so the window the tile
    actually processes is LARGER than the core tile. Note: the engine then
    applies ``expand_crop`` (uniform ratio) -- the exact FINAL window is the
    one that layer 1 carries in ``saya_couple_crop.crop_region``; this
    approximation is only for tests/auditing, never for guessing the crop.
    """
    x0 = max(window.x0 - padding, 0)
    y0 = max(window.y0 - padding, 0)
    x1 = min(window.x1 + padding, canvas_w)
    y1 = min(window.y1 + padding, canvas_h)
    return TileWindow(x0=x0, y0=y0, x1=x1, y1=y1, row=window.row, col=window.col)


def mask_for_tile_work(mask_hw: torch.Tensor, window: TileWindow, work_h: int, work_w: int) -> torch.Tensor:
    """The mask as seen by the tile at its WORK size (round8(tile+padding)).

    Contractual order: slice the REAL window FIRST, resize NEAREST SECOND --
    never the other way around (resampling the full frame first would crush
    the whole P1->P2 gradient into the tile, a known past defect).

    ``window`` MUST be the engine's CROP window -- core tile + padding band
    (see ``padded_tile_window``; in production: the ``crop_region`` value of
    the ``saya_couple_crop`` metadata written by layer 1,
    ``crop_model_patch.py:50-58``). Resizing the bare core-tile slice to the
    padded size would stretch the geometry (the padding band is spatial
    context, not content) -- never pass the bare core tile.
    """
    _check_grid(work_h, work_w)
    tile_mask = mask_for_tile(mask_hw, window)
    return functional.interpolate(
        tile_mask.unsqueeze(0).unsqueeze(0), size=(work_h, work_w), mode="nearest"
    ).squeeze(0).squeeze(0)


def downsample_nearest(mask: torch.Tensor, size: tuple[int, int]) -> torch.Tensor:
    """Downsample NEAREST to the attention grid (staircase contour is expected).

    Accepts (H,W), (B,H,W) or (1,1,H,W); returns the same dimensionality.
    """
    squeeze2d = mask.ndim == 2
    squeeze3d = mask.ndim == 3
    work = mask.unsqueeze(0).unsqueeze(0) if squeeze2d else (mask.unsqueeze(0) if squeeze3d else mask)
    down = functional.interpolate(work, size=size, mode="nearest")
    if squeeze2d:
        return down.squeeze(0).squeeze(0)
    if squeeze3d:
        return down.squeeze(0)
    return down


# ---------------------------------------------------------------------------
# ComfyUI node (purely declarative class, importable without ComfyUI)
# ---------------------------------------------------------------------------

class SayaCoupleRegionMasks:
    """Node: geometry imprint -> mask_base / mask_p1 / mask_p2 (B,H,W).

    Resolution = that of the CURRENT pass' IMAGE (never the imprint's
    reference frame). Registered in registry.py.
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "imprint": ("SAYA_IMPRINT", {"forceInput": True}),
                "image": ("IMAGE", {
                    "tooltip": "Image of the CURRENT pass: sets the rasterization "
                               "resolution (masks are re-derived, never carried over)."
                }),
            },
        }

    RETURN_TYPES = ("MASK", "MASK", "MASK")
    RETURN_NAMES = ("mask_base", "mask_p1", "mask_p2")
    FUNCTION = "masks"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Region masks from the v2 imprint (regions are ground truth), "
        "rasterized at the current image's resolution. White = active. "
        "mask_p2 is an empty mask when P2 is absent (never fabricated)."
    )

    @staticmethod
    def _image_grid(image: Any) -> tuple[int, int]:
        shape = getattr(image, "shape", None)
        if shape is None or len(shape) != 4:
            raise SayaMaskError(
                f"image must be a 4D IMAGE tensor [B,H,W,C], got {shape!r}"
            )
        _batch, height, width, _channels = (int(size) for size in shape)
        _check_grid(height, width)
        return height, width

    def masks(self, imprint: dict[str, Any], image: Any) -> tuple[Any, Any, Any]:
        height, width = self._image_grid(image)
        batch = int(image.shape[0])
        derived = derive_masks(imprint, height, width, batch)
        empty = torch.zeros_like(derived.base)
        p2 = derived.person_2 if derived.person_2 is not None else empty
        return (derived.base, derived.person_1, p2)

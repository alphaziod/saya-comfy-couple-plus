"""Pure geometry helpers for the HiDream Architecture-E conditioning surface.

Imported by `src/nodes/couple_conditioning_hidream.py`. These build
the per-person ownership masks that are attached to native HiDream CONDITIONING
entries. The ComfyUI sampler (`comfy/samplers.py:_calc_cond_batch` /
`get_area_and_mult`) then composites the regional predictions itself — this
module never touches the model, the sampler, or attention.

--------------------------------------------------------------------------------
COORDINATE SPACES  (kept strictly separate on purpose)
--------------------------------------------------------------------------------
* pixel space   : H_px  x W_px          — what the user sees; the `MASK` outputs
                                          of the node live here (unchanged from
                                          the Forge node, so existing mask
                                          consumers / detailers keep working).
* latent space  : H_lat x W_lat = H_px/8 x W_px/8   (VAE factor 8, Flux family).
                                          The masks attached to CONDITIONING
                                          entries live here, because the sampler
                                          resolves masks against the *latent*
                                          (`resolve_areas_and_cond_masks_multidim`
                                          would otherwise bilinearly upscale a
                                          pixel mask and slightly move the seam).
* patch-grid    : H_lat/2 x W_lat/2      — HiDream-internal only (2x2 patchify).
                                          NEVER used here. Architecture E composes
                                          full latent predictions, not patch
                                          tokens, so this space does not appear.

--------------------------------------------------------------------------------
NORMALISATION
--------------------------------------------------------------------------------
`routing_masks` returns masks whose per-pixel sum is exactly 1 (it divides by
their total). The sampler *also* renormalises by the sum of all region weights
(`out_conds /= out_counts` in `_calc_cond_batch`), so a partition of unity in and
a partition of unity out. We still normalise here so the returned masks are
individually meaningful (previews, debugging, detailer reuse).
"""

from __future__ import annotations

import torch

# Same defaults as forge/node.py so a SAYA_COUPLE_CONFIG bundle produced by the
# SDXL Forge MASTER stays meaningful when reused for HiDream.
_ORIENTATIONS = ("horizontal", "vertical")


def _axis_coordinates(axis_size: int) -> torch.Tensor:
    """Pixel/cell centres mapped to [0, 1) along one axis.

    Resolution independent: the same `center` / `transition` produce the same
    geometry whether sampled at latent or pixel size.
    """
    axis_size = max(1, int(axis_size))
    return (torch.arange(axis_size, dtype=torch.float32) + 0.5) / axis_size


def ownership_profile(
    axis_size: int, center: float, transition: float, floor: float
) -> torch.Tensor:
    """1-D person-1 ownership along one axis, values in [floor, 1 - floor].

    Identical formula to forge/node.py:build_character_masks. `center` is the seam
    position in [0, 1]; `transition` is the soft-blend width; `floor` keeps a
    minimum share for the losing person so a prompt is never fully zeroed.
    Returns shape [axis_size].
    """
    coordinate = _axis_coordinates(axis_size)
    start = float(center) - float(transition) / 2.0
    ramp = ((start + float(transition) - coordinate) / float(transition)).clamp(0.0, 1.0)
    return float(floor) + (1.0 - 2.0 * float(floor)) * ramp


def contact_profile(
    axis_size: int, center: float, contact_width: float, strength: float
) -> torch.Tensor:
    """1-D MAIN-only contact band centred on the seam, values in [0, strength].

    Geometry of Forge "COARSE MAIN 2.3": a smoothstep bump that gives the global
    scene prompt authority exactly where the two people physically meet (fused
    hands / hips) without letting it bleed across the whole frame.
    Returns shape [axis_size].
    """
    coordinate = _axis_coordinates(axis_size)
    half = max(float(contact_width) / 2.0, 1.0 / max(1, int(axis_size)))
    bump = (1.0 - (coordinate - float(center)).abs() / half).clamp(0.0, 1.0)
    bump = bump * bump * (3.0 - 2.0 * bump)  # smoothstep
    return bump * float(strength)


def _expand_axis_profile(
    profile: torch.Tensor, orientation: str, batch: int, height: int, width: int
) -> torch.Tensor:
    """Broadcast a 1-D axis profile to a [batch, height, width] mask."""
    if orientation == "vertical":
        return profile.view(1, height, 1).expand(batch, height, width).contiguous()
    return profile.view(1, 1, width).expand(batch, height, width).contiguous()


def _normalise_orientation(orientation: str) -> str:
    return orientation if orientation in _ORIENTATIONS else "horizontal"


def routing_masks(
    batch: int,
    height: int,
    width: int,
    orientation: str,
    center: float,
    transition: float,
    mask_floor: float,
    shared_contact_strength: float,
    contact_width: float,
    include_main_contact: bool,
    swap: bool,
) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor | None]:
    """Per-region masks for the sampler, each shape [batch, height, width].

    Call with LATENT height/width. Returns (person_1, person_2, main_contact):
      * without contact band: person_1 + person_2 == 1 per pixel — the plain
        two-region Forge baseline.
      * with contact band: person_1 + person_2 + main_contact == 1 per pixel;
        inside the band a bounded share is moved to the MAIN-only region so the
        shared pose can dominate without hard-assigning the overlap.
    `main_contact` is None when the band is disabled or degenerate.

    The mask dtype is float32; the sampler casts/repeats it to the latent later.
    """
    orientation = _normalise_orientation(orientation)
    axis_size = width if orientation == "horizontal" else height

    p1_axis = ownership_profile(axis_size, center, transition, mask_floor)
    if swap:
        p1_axis = 1.0 - p1_axis

    p1 = _expand_axis_profile(p1_axis, orientation, batch, height, width)
    p2 = 1.0 - p1

    band_active = (
        include_main_contact
        and float(shared_contact_strength) > 0.0
        and float(contact_width) > 0.0
    )
    if not band_active:
        total = (p1 + p2).clamp_min(1e-8)
        return p1 / total, p2 / total, None

    shared_axis = contact_profile(axis_size, center, contact_width, shared_contact_strength)
    shared = _expand_axis_profile(shared_axis, orientation, batch, height, width)
    private_budget = 1.0 - shared
    private_p1 = private_budget * p1
    private_p2 = private_budget * p2
    total = (shared + private_p1 + private_p2).clamp_min(1e-8)
    return private_p1 / total, private_p2 / total, shared / total


def pixel_masks(
    batch: int,
    height: int,
    width: int,
    orientation: str,
    center: float,
    transition: float,
    mask_floor: float,
    swap: bool,
) -> tuple[torch.Tensor, torch.Tensor]:
    """Person ownership masks at PIXEL resolution, each shape [batch, height, width].

    These are the node's public `MASK` outputs. Same formula as
    forge/node.py:build_character_masks so previews and non-HiDream detailers
    keep matching. They carry NO contact-band share — they are pure identity
    masks, exactly as the Forge node exposes them.
    """
    orientation = _normalise_orientation(orientation)
    axis_size = width if orientation == "horizontal" else height
    p1_axis = ownership_profile(axis_size, center, transition, mask_floor)
    if swap:
        p1_axis = 1.0 - p1_axis
    p1 = _expand_axis_profile(p1_axis, orientation, batch, height, width)
    return p1.clamp(0.0, 1.0), (1.0 - p1).clamp(0.0, 1.0)

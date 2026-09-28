"""Node: per-segment detail pass, delegated to the Impact Pack.

The legacy node this replaces existed only to reorder the Impact Pack's
input sockets so the graph could wire CONDITIONING more conveniently, and
to carry regional crop coordinates inside a globally patched model. The
refinement itself was always Impact Pack's ``DetailerForEach.do_detail``.

This node keeps the first half and drops the second:

* the socket names, types and declaration order are the ones the workflow
  graph already wires (verified against the workflow JSON, where the
  detailer nodes expose IMAGE, SEGS, MODEL, CLIP, VAE, positive, negative,
  the two optional hook sockets, then eighteen widgets in this order);
* every pixel is computed by the installed Impact Pack, lazily imported so
  this pack still registers when Impact is absent and fails at execution
  with an install message.

The positional call below was verified against the installed Impact Pack
(commit 429d015, 2026-04-20): ``do_detail(image, segs, model, clip, vae,
guide_size, guide_size_for_bbox, max_size, seed, steps, cfg, sampler_name,
scheduler, positive, negative, denoise, feather, noise_mask, force_inpaint,
wildcard_opt=None, detailer_hook=None, ...)``.

Crop-aware couple attention (``saya_couple_crop``)
---------------------------------------------------
``model`` arrives already patched by ``SayaAttentionCouplePPM`` (via
``SayaCoupleReconstruct``'s ``model_patched`` output) at the FULL PASS
resolution. Impact Pack crops+upscales each SEGS region internally and
samples it in isolation, with no knowledge of that region's real position
in the canvas — without help, the couple patch's per-region output mask
gets squashed onto the tiny crop's own grid (see
``ppm_vendor/attention_couple/common.py::reshape_mask``), scrambling which
pixels get P1 vs P2. USDU's tiling engine avoids this by attaching
``transformer_options["saya_couple_crop"]`` (crop_region + canvas size) to
a per-tile model clone before sampling (``crop_model_patch.py``); this node
reproduces the SAME metadata contract per SEGS region, via Impact Pack's
own ``DetailerHook.touch_scaled_size``/``pre_ksample`` extension points
(``modules/impact/hooks.py``) — no Impact Pack internals are forked, no
private closure is touched. See ``_CoupleCropHook`` below.

The hook never sees a seg, only its scaled size, so each sampled seg is
matched POSITIONALLY to a prediction of Impact Pack's skip logic. That is
only attempted with an empty wildcard (a wildcard can reorder or drop
SEGS), and the hook cross-checks every scaled size against the prediction:
any mismatch disables the metadata for the rest of the call rather than
attach a wrong region.
"""

from __future__ import annotations

import logging
from typing import Any

LOGGER = logging.getLogger(__name__)

_IMPACT_INSTALL_HINT = (
    "Install ComfyUI-Impact-Pack from "
    "https://github.com/ltdrdata/ComfyUI-Impact-Pack"
)

_FALLBACK_SAMPLERS = (
    "euler", "euler_cfg_pp", "euler_ancestral", "euler_ancestral_cfg_pp",
    "heun", "heunpp2", "dpm_2", "dpm_2_ancestral", "lms", "dpm_fast",
    "dpm_adaptive", "dpmpp_2s_ancestral", "dpmpp_2s_ancestral_cfg_pp",
    "dpmpp_sde", "dpmpp_sde_gpu", "dpmpp_2m", "dpmpp_2m_cfg_pp",
    "dpmpp_2m_sde", "dpmpp_2m_sde_gpu", "dpmpp_3m_sde", "dpmpp_3m_sde_gpu",
    "ddpm", "lcm", "ddim", "uni_pc", "uni_pc_bh2",
)
_FALLBACK_SCHEDULERS = (
    "normal", "karras", "exponential", "sgm_uniform", "simple",
    "ddim_uniform", "beta", "linear_quadratic", "kl_optimal",
)

MAX_DETAIL_SIZE = 16384


def _detail_option_lists() -> tuple[list[str], list[str]]:
    """Live sampler/scheduler names, preferring the Impact Pack's schedulers.

    Impact adds its own entries (AYS variants and friends) on top of
    ComfyUI's, and the saved graph may reference them, so its list wins
    when the pack is loaded.
    """
    try:
        from comfy.samplers import KSampler  # type: ignore

        samplers = list(KSampler.SAMPLERS)
        schedulers = list(KSampler.SCHEDULERS)
    except (ImportError, AttributeError):
        samplers = list(_FALLBACK_SAMPLERS)
        schedulers = list(_FALLBACK_SCHEDULERS)
    try:
        from impact import core as impact_core  # type: ignore

        schedulers = list(impact_core.get_schedulers())
    except (ImportError, AttributeError):
        pass
    return (samplers, schedulers)


def _resolve_impact_detailer() -> Any:
    """Lazily fetch Impact Pack's DetailerForEach class."""
    try:
        from impact.impact_pack import DetailerForEach  # type: ignore

        return DetailerForEach
    except ImportError:
        pass
    try:
        import nodes as comfy_nodes  # type: ignore

        found = comfy_nodes.NODE_CLASS_MAPPINGS.get("DetailerForEach")
        if found is not None:
            return found
    except (ImportError, AttributeError):
        pass
    raise ImportError(
        "Saya Duo Detail (SEGS) requires ComfyUI-Impact-Pack, which is not "
        f"loaded. {_IMPACT_INSTALL_HINT}"
    )


def _resolve_impact_hook_classes() -> tuple[Any, Any]:
    """Lazily fetch Impact Pack's DetailerHook / DetailerHookCombine (public
    extension points, ``modules/impact/hooks.py``)."""
    from impact.hooks import DetailerHook, DetailerHookCombine  # type: ignore

    return DetailerHook, DetailerHookCombine


def _predicted_processed_segs(
    segs: Any, image: Any, guide_size: float, guide_size_for_bbox: bool,
    max_size: float, force_inpaint: bool,
) -> list[tuple[Any, tuple[int, int]]]:
    """``(crop_region, scaled size)`` of each seg ``DetailerForEach.do_detail``
    will sample, in sampling order, for an EMPTY wildcard only. Mirrors, in
    order (Impact Pack 8.28.3):

    1. ``core.segs_scale_match`` (a pure function, called with the SAME
       inputs Impact itself uses, so ``crop_region`` values match exactly);
    2. ``do_detail``'s empty-mask skip;
    3. ``enhance_detail``'s guide_size/upscale skip and scaled size — the
       ``(w, h)`` it then hands to ``touch_scaled_size`` (core.py ~283-326).

    A wildcard can re-sort ([ASC]/[DSC]/[ASC-SIZE]/[DSC-SIZE]/[RND]) or drop
    ([SKIP]/[STOP], per label with [LAB]) segs; none of that is replicated,
    so the caller must not use this with a non-empty wildcard. This
    duplicates Impact's skip logic and may drift on an Impact update — the
    hook's size cross-check then disables the metadata instead of attaching
    a wrong region.
    """
    from impact import core as impact_core  # type: ignore

    scaled = impact_core.segs_scale_match(segs, image.shape)
    expected = []
    for seg in scaled[1]:
        cropped_mask = seg.cropped_mask
        if cropped_mask is not None and bool((cropped_mask == 0).all()):
            continue
        x1, y1, x2, y2 = seg.crop_region
        w, h = x2 - x1, y2 - y1
        bx1, by1, bx2, by2 = seg.bbox
        bbox_w, bbox_h = bx2 - bx1, by2 - by1
        if w <= 0 or h <= 0 or bbox_w <= 0 or bbox_h <= 0:
            continue
        if not force_inpaint and bbox_h >= guide_size and bbox_w >= guide_size:
            continue
        upscale = guide_size / min(bbox_w, bbox_h) if guide_size_for_bbox else guide_size / min(w, h)
        new_w, new_h = int(w * upscale), int(h * upscale)
        if new_w > max_size or new_h > max_size:
            upscale *= max_size / max(new_w, new_h)
            new_w, new_h = int(w * upscale), int(h * upscale)
        if upscale <= 1.0 or new_w == 0 or new_h == 0:
            if not force_inpaint:
                continue
            new_w, new_h = w, h
        expected.append((seg.crop_region, (new_w, new_h)))
    return expected


def _attach_saya_couple_crop(model: Any, crop_region: Any, canvas_size: tuple[int, int]) -> Any:
    """Clone ``model`` and attach ``saya_couple_crop`` for THIS seg's real
    position in the original canvas — the SAME metadata contract USDU's
    tiling engine writes (``crop_model_patch.py`` -> ``full_width``,
    ``full_height``, ``crop_region``; read by
    ``ppm_vendor/attention_couple/common.py::crop_mask_to_tile``). A fresh
    clone per seg: never mutates the shared model, never leaks to the next
    seg or the next ``detail()`` call.
    """
    x1, y1, x2, y2 = (float(v) for v in crop_region)
    canvas_w, canvas_h = canvas_size
    if not (x2 > x1 and y2 > y1 and canvas_w > 0 and canvas_h > 0):
        return model
    patched = model.clone()
    transformer_options = patched.model_options.setdefault("transformer_options", {})
    transformer_options["saya_couple_crop"] = {
        "crop_region": [x1, y1, x2, y2],
        "full_width": int(canvas_w),
        "full_height": int(canvas_h),
        "label": "Detailer",
    }
    return patched


def _build_couple_crop_hook(
    expected: list[tuple[Any, tuple[int, int]]], canvas_size: tuple[int, int], inner_hook: Any,
) -> Any:
    """A ``DetailerHook`` that attaches ``saya_couple_crop`` per seg, chained
    with any caller-supplied ``detailer_hook`` via Impact Pack's own
    ``DetailerHookCombine`` (no delegation boilerplate, no forked hook
    surface). ``_pos``/``_desynced`` are instance state private to THIS
    ``detail()`` invocation, never shared or global.
    """
    DetailerHook, DetailerHookCombine = _resolve_impact_hook_classes()

    class _CoupleCropHook(DetailerHook):
        def __init__(self) -> None:
            super().__init__()
            self._pos = -1
            self._desynced = False

        def touch_scaled_size(self, w: int, h: int) -> tuple[int, int]:
            # Fires once per seg that reaches sampling (core.py:325-326), before
            # its pre_ksample calls. Impact's scaled size must match the
            # prediction at this position, else the positional mapping is lost.
            self._pos += 1
            if not self._desynced:
                predicted = expected[self._pos][1] if self._pos < len(expected) else None
                if predicted != (w, h):
                    self._desynced = True
                    LOGGER.warning(
                        "[Saya Duo Detail] SEGS desync at sampled seg #%d: Impact scaled size %s, "
                        "predicted %s -- saya_couple_crop disabled for the rest of this detailer call",
                        self._pos, (w, h), predicted,
                    )
            return w, h

        def pre_ksample(self, model, seed, steps, cfg, sampler_name, scheduler,
                         positive, negative, upscaled_latent, denoise):
            if not self._desynced and 0 <= self._pos < len(expected):
                model = _attach_saya_couple_crop(model, expected[self._pos][0], canvas_size)
            return model, seed, steps, cfg, sampler_name, scheduler, positive, negative, upscaled_latent, denoise

    crop_hook = _CoupleCropHook()
    if inner_hook is None:
        return crop_hook
    return DetailerHookCombine(crop_hook, inner_hook)


class SayaDuoSegsDetail:
    """Refine every detected segment, delegating entirely to the Impact Pack."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        samplers, schedulers = _detail_option_lists()
        return {
            "required": {
                "image": ("IMAGE",),
                "segs": ("SEGS",),
                "model": ("MODEL",),
                "clip": ("CLIP",),
                "vae": ("VAE",),
                "positive": ("CONDITIONING",),
                "negative": ("CONDITIONING",),
                "guide_size": ("FLOAT", {"default": 512, "min": 64, "max": MAX_DETAIL_SIZE, "step": 8}),
                "guide_size_for": ("BOOLEAN", {"default": True, "label_on": "bbox", "label_off": "crop_region"}),
                "max_size": ("FLOAT", {"default": 1024, "min": 64, "max": MAX_DETAIL_SIZE, "step": 8}),
                "seed": ("INT", {"default": 0, "min": 0, "max": 0xFFFFFFFFFFFFFFFF}),
                "steps": ("INT", {"default": 20, "min": 1, "max": 10000}),
                "cfg": ("FLOAT", {"default": 8.0, "min": 0.0, "max": 100.0}),
                "sampler_name": (samplers,),
                "scheduler": (schedulers,),
                "denoise": ("FLOAT", {"default": 0.5, "min": 0.0001, "max": 1.0, "step": 0.01}),
                "feather": ("INT", {"default": 5, "min": 0, "max": 100, "step": 1}),
                "noise_mask": ("BOOLEAN", {"default": True, "label_on": "enabled", "label_off": "disabled"}),
                "force_inpaint": ("BOOLEAN", {"default": True, "label_on": "enabled", "label_off": "disabled"}),
                "wildcard": ("STRING", {"multiline": True, "dynamicPrompts": False}),
                "cycle": ("INT", {"default": 1, "min": 1, "max": 10, "step": 1}),
            },
            "optional": {
                "detailer_hook": ("DETAILER_HOOK",),
                "inpaint_model": ("BOOLEAN", {"default": False, "label_on": "enabled", "label_off": "disabled"}),
                "noise_mask_feather": ("INT", {"default": 20, "min": 0, "max": 100, "step": 1}),
                "scheduler_func_opt": ("SCHEDULER_FUNC",),
                "tiled_encode": ("BOOLEAN", {"default": False, "label_on": "enabled", "label_off": "disabled"}),
                "tiled_decode": ("BOOLEAN", {"default": False, "label_on": "enabled", "label_off": "disabled"}),
            },
        }

    RETURN_TYPES = ("IMAGE",)
    RETURN_NAMES = ("image",)
    FUNCTION = "detail"
    CATEGORY = "Saya/Duo"
    DESCRIPTION = (
        "Refine each SEGS region with the installed Impact Pack. Socket order "
        "matches the graph the workflow already wires; no detail logic is "
        "forked. Each region's real crop position is handed to the couple "
        "attention patch via saya_couple_crop (DetailerHook), so P1/P2 stay "
        "spatially correct inside each crop instead of being squashed from "
        "the full-frame mask."
    )

    def detail(
        self,
        image: Any,
        segs: Any,
        model: Any,
        clip: Any,
        vae: Any,
        positive: Any,
        negative: Any,
        guide_size: float,
        guide_size_for: bool,
        max_size: float,
        seed: int,
        steps: int,
        cfg: float,
        sampler_name: str,
        scheduler: str,
        denoise: float,
        feather: int,
        noise_mask: bool,
        force_inpaint: bool,
        wildcard: str,
        cycle: int = 1,
        detailer_hook: Any = None,
        inpaint_model: bool = False,
        noise_mask_feather: int = 0,
        scheduler_func_opt: Any = None,
        tiled_encode: bool = False,
        tiled_decode: bool = False,
    ) -> tuple[Any]:
        detailer = _resolve_impact_detailer()
        crop_hook = detailer_hook
        if wildcard.strip():
            LOGGER.warning(
                "[Saya Duo Detail] saya_couple_crop disabled for this detailer call: a non-empty "
                "wildcard can reorder or skip SEGS ([ASC]/[DSC]/[SKIP]...), so crop positions "
                "cannot be matched safely"
            )
        else:
            expected = _predicted_processed_segs(
                segs, image, guide_size, guide_size_for, max_size, force_inpaint,
            )
            if expected:
                canvas_h, canvas_w = int(image.shape[1]), int(image.shape[2])
                crop_hook = _build_couple_crop_hook(expected, (canvas_w, canvas_h), detailer_hook)
        enhanced, *_ = detailer.do_detail(
            image, segs, model.clone(), clip, vae,
            guide_size, guide_size_for, max_size,
            seed, steps, cfg, sampler_name, scheduler,
            positive, negative, denoise, feather, noise_mask,
            force_inpaint, wildcard, crop_hook,
            cycle=cycle,
            inpaint_model=inpaint_model,
            noise_mask_feather=noise_mask_feather,
            scheduler_func_opt=scheduler_func_opt,
            tiled_encode=tiled_encode,
            tiled_decode=tiled_decode,
        )
        return (enhanced,)

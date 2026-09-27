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
  with an install message;
* no geometry is smuggled through the MODEL. A pass that needs duo region
  masks builds them for its own resolution (``src/duo_geometry``).

The positional call below was verified against the installed Impact Pack
(commit 429d015, 2026-04-20): ``do_detail(image, segs, model, clip, vae,
guide_size, guide_size_for_bbox, max_size, seed, steps, cfg, sampler_name,
scheduler, positive, negative, denoise, feather, noise_mask, force_inpaint,
wildcard_opt=None, detailer_hook=None, ...)``.
"""

from __future__ import annotations

from typing import Any

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
        "matches the graph the workflow already wires; no logic is forked and "
        "no geometry is carried inside the MODEL."
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
        enhanced, *_ = detailer.do_detail(
            image, segs, model.clone(), clip, vae,
            guide_size, guide_size_for, max_size,
            seed, steps, cfg, sampler_name, scheduler,
            positive, negative, denoise, feather, noise_mask,
            force_inpaint, wildcard, detailer_hook,
            cycle=cycle,
            inpaint_model=inpaint_model,
            noise_mask_feather=noise_mask_feather,
            scheduler_func_opt=scheduler_func_opt,
            tiled_encode=tiled_encode,
            tiled_decode=tiled_decode,
        )
        return (enhanced,)

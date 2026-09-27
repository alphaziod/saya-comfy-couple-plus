"""Sampler configuration whose COMBO output types match ComfyUI exactly."""

from __future__ import annotations

from typing import Any


def _runtime_sampler_lists() -> tuple[list[str], list[str]]:
    """Read the lists after scheduler extensions (beta45/beta57) are loaded."""
    try:
        from comfy.samplers import KSampler  # type: ignore

        samplers = list(KSampler.SAMPLERS)
        schedulers = list(KSampler.SCHEDULERS)
    except (ImportError, AttributeError):
        # Unit-test fallback only. A real ComfyUI process provides KSampler.
        samplers = ["euler"]
        schedulers = ["simple", "linear_quadratic", "beta57", "beta45"]
    return samplers, schedulers


class _LiveComboTypes(type):
    """Resolve ``RETURN_TYPES`` when ComfyUI reads it, not when Python imports it.

    ComfyUI imports custom nodes in ``os.listdir`` order, which is unsorted, and
    the packs that add beta45/beta57/bong_tangent REBIND
    ``KSampler.SCHEDULERS`` to a new list. Whatever this module captured at
    import time -- a copy or a reference -- is therefore stale whenever this
    pack happens to load first. ComfyUI only reads a node's types after every
    custom node has been imported, so reading them on access is always current.
    ``RETURN_TYPES`` stays a plain tuple, which is all ComfyUI expects.
    """

    @property
    def RETURN_TYPES(cls) -> tuple:
        samplers, schedulers = _runtime_sampler_lists()
        return ("INT", "INT", "FLOAT", samplers, schedulers)


class SayaKSamplerConfig(metaclass=_LiveComboTypes):
    """Expose sampling controls with exact runtime COMBO types.

    rgthree's config node can keep an older scheduler list. When another custom
    node adds beta45/beta57 to ``KSampler.SCHEDULERS``, ComfyUI rejects links
    between the two unequal COMBO types. This node sources both its input and
    output types from the live KSampler lists, read on access, so the types
    remain identical whatever order the custom nodes were loaded in.
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        samplers, schedulers = _runtime_sampler_lists()
        return {
            "required": {
                "steps_total": (
                    "INT",
                    {"default": 4, "min": 1, "max": 10000, "step": 1},
                ),
                "refiner_step": (
                    "INT",
                    {"default": 2, "min": 0, "max": 10000, "step": 1},
                ),
                "cfg": (
                    "FLOAT",
                    {
                        "default": 1.0,
                        "min": 0.0,
                        "max": 100.0,
                        "step": 0.01,
                    },
                ),
                "sampler_name": (samplers,),
                "scheduler": (schedulers,),
            }
        }

    @property
    def RETURN_TYPES(self) -> tuple:
        # ComfyUI also reads the types off the node INSTANCE (execution.py);
        # the metaclass property alone is invisible from there.
        return type(self).RETURN_TYPES

    RETURN_NAMES = (
        "steps",
        "refiner_step",
        "cfg",
        "sampler_name",
        "scheduler",
    )
    FUNCTION = "configure"
    CATEGORY = "Saya/Sampling"

    def configure(
        self,
        steps_total: int,
        refiner_step: int,
        cfg: float,
        sampler_name: str,
        scheduler: str,
    ) -> tuple[int, int, float, str, str]:
        return (
            int(steps_total),
            int(refiner_step),
            float(cfg),
            str(sampler_name),
            str(scheduler),
        )

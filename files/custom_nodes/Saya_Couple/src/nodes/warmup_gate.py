"""SayaWarmupGate — passes the original latent through after a discarded sampling step.

Phase 3's "frame -1": an identical sampler (same pair, same size) runs ONE step whose
result is thrown away; the real sampler then starts from an already-warm GPU. ComfyUI only
executes what leads to an output, so this node wires the discarded step to the real sampler
(guaranteeing the order) without ever touching the latent.
"""

from __future__ import annotations

from typing import Any


class SayaWarmupGate:
    """Return ``latent`` unchanged; ``warmup`` exists only to force its execution first."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "latent": ("LATENT", {"tooltip": "Original latent, returned unchanged."}),
                "warmup": ("LATENT", {"tooltip": "Output of the discarded step: never read, only waited on."}),
            },
        }

    RETURN_TYPES = ("LATENT",)
    RETURN_NAMES = ("latent",)
    FUNCTION = "gate"
    CATEGORY = "saya/hidream"
    DESCRIPTION = "Frame -1: waits for the discarded sampling step, then returns the original latent unmodified."

    def gate(self, latent: dict[str, Any], warmup: dict[str, Any]) -> tuple[dict[str, Any]]:
        return (latent,)

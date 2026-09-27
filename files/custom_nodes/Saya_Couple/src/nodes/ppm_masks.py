"""Saya PPM Masks -- minimal spatial helper for Saya Attention Couple PPM.

This node touches neither the conditioning, the MODEL, nor attention. It
only builds the three MASK outputs consumed by Attention Couple: BASE_MASK
(global MAIN), P1_MASK and P2_MASK.

Controls:
- Orientation: horizontal = P1 left / P2 right, vertical = P1 top / P2 bottom.
- Swap P1 / P2: only swaps the spatial assignment of the two masks.
- MAIN Weight / Person Weight: mask amplitudes before PPM normalization.
- Split: position of the main boundary on the chosen axis.
- Overlap: extension of P1/P2 around the boundary (0 = no overlap).
"""

from __future__ import annotations

import torch


class SayaPPMMasks:
    @classmethod
    def INPUT_TYPES(cls):
        return {
            "required": {
                "latent": ("LATENT",),
                "Orientation": (["horizontal", "vertical"], {
                    "default": "horizontal",
                    "tooltip": "horizontal: P1 left / P2 right. vertical: P1 top / P2 bottom.",
                }),
                "Swap P1 / P2": ("BOOLEAN", {
                    "default": False,
                    "tooltip": "Swaps the spatial assignment of P1 and P2 without rewiring prompts.",
                }),
                "MAIN Weight": ("FLOAT", {
                    "default": 0.7, "min": 0.0, "max": 1.0, "step": 0.01,
                    "tooltip": "Weight of the MAIN mask over the whole frame before PPM normalization.",
                }),
                "Person Weight": ("FLOAT", {
                    "default": 0.3, "min": 0.0, "max": 1.0, "step": 0.01,
                    "tooltip": "Weight of the P1/P2 masks before PPM normalization.",
                }),
                "Split": ("FLOAT", {
                    "default": 0.5, "min": 0.0, "max": 1.0, "step": 0.01, "round": 0.001,
                    "tooltip": "P1/P2 boundary. 0.50 = exact center.",
                }),
                "Overlap": ("FLOAT", {
                    "default": 0.0, "min": 0.0, "max": 0.5, "step": 0.01, "round": 0.001,
                    "tooltip": "Overlap around the Split. 0.00 = no overlap.",
                }),
            },
        }

    RETURN_TYPES = ("MASK", "MASK", "MASK")
    RETURN_NAMES = ("BASE_MASK", "P1_MASK", "P2_MASK")
    FUNCTION = "make_masks"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Masks only. PPM normalizes the weights wherever several masks overlap. "
        "This node never modifies prompts, conditionings or the MODEL."
    )

    @staticmethod
    def _fill_interval(mask: torch.Tensor, orientation: str, start: float, end: float, value: float) -> None:
        start = max(0.0, min(1.0, float(start)))
        end = max(0.0, min(1.0, float(end)))
        if end <= start or value == 0.0:
            return
        h, w = mask.shape
        if orientation == "vertical":
            y0 = max(0, min(h, round(start * h)))
            y1 = max(0, min(h, round(end * h)))
            if y1 > y0:
                mask[y0:y1, :] = value
        else:
            x0 = max(0, min(w, round(start * w)))
            x1 = max(0, min(w, round(end * w)))
            if x1 > x0:
                mask[:, x0:x1] = value

    def make_masks(
        self,
        latent,
        Orientation="horizontal",
        **kwargs,
    ):
        orientation = str(Orientation).lower()
        if orientation not in {"horizontal", "vertical"}:
            orientation = "horizontal"

        swap = bool(kwargs.get("Swap P1 / P2", False))
        main_weight = float(kwargs.get("MAIN Weight", 0.7))
        person_weight = float(kwargs.get("Person Weight", 0.3))
        split = max(0.0, min(1.0, float(kwargs.get("Split", 0.5))))
        overlap = max(0.0, min(0.5, float(kwargs.get("Overlap", 0.0))))

        samples = latent["samples"]
        height = int(samples.shape[2]) * 8
        width = int(samples.shape[3]) * 8

        base = torch.full((height, width), main_weight, dtype=torch.float32)
        p1 = torch.zeros((height, width), dtype=torch.float32)
        p2 = torch.zeros((height, width), dtype=torch.float32)

        # Preserves the historical Overlap semantics: P1 advances by +overlap
        # and P2 retreats by -overlap around Split.
        self._fill_interval(p1, orientation, 0.0, split + overlap, person_weight)
        self._fill_interval(p2, orientation, split - overlap, 1.0, person_weight)

        if swap:
            p1, p2 = p2, p1

        return (base.unsqueeze(0), p1.unsqueeze(0), p2.unsqueeze(0))

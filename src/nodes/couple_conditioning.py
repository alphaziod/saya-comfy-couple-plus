"""Character conditioning composition and model patch orchestration."""

from __future__ import annotations

import os
from typing import Any, Self

try:
    import torch
except Exception:
    torch = None

from .regional_attention.node import RegionalAttentionNode


class CoupleConditioningNode:
    """Build two character regions and patch each connected model consistently."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        """Return the ComfyUI input schema exposed by this node."""
        return {
            "required": {
                "model_main": ("MODEL",),
                "main_positive": ("CONDITIONING",),
                "person_1_positive": ("CONDITIONING",),
                "person_2_positive": ("CONDITIONING",),
                "negative": ("CONDITIONING",),
                "latent": ("LATENT",),
                "use_couple_attention": ("BOOLEAN", {"default": True}),
                "orientation": (["horizontal", "vertical"], {"default": "horizontal"}),
                "center": ("FLOAT", {"default": 0.5, "min": 0.15, "max": 0.85, "step": 0.01}),
                "swap_person_positions": ("BOOLEAN", {"default": False}),
            },
            "optional": {
                "dual_sampling_model": ("MODEL",),
                "support_model_1": ("MODEL",),
                "support_model_2": ("MODEL",),
                "support_model_3": ("MODEL",),
            },
        }

    RETURN_TYPES = (
        "MODEL",
        "MODEL",
        "MODEL",
        "MODEL",
        "MODEL",
        "CONDITIONING",
        "CONDITIONING",
        "CONDITIONING",
        "MASK",
        "MASK",
    )
    RETURN_NAMES = (
        "patched_model_main",
        "patched_dual_sampling_model",
        "patched_support_model_1",
        "patched_support_model_2",
        "patched_support_model_3",
        "positive_final",
        "detailer_positive",
        "negative",
        "mask_person_1",
        "mask_person_2",
    )
    FUNCTION = "run"
    CATEGORY = "saya/rescue"

    @staticmethod
    def copy_conditioning(conditioning: Any) -> Any:
        """Clone conditioning entries while preserving tensor ownership and metadata."""
        if not conditioning:
            return []
        return [[entry[0], dict(entry[1])] for entry in conditioning]

    @staticmethod
    def describe_conditioning_error(name: str, conditioning: Any) -> Any:
        """Return a human-readable conditioning validation error, or None when valid."""
        if conditioning is None or conditioning == []:
            return None
        if not isinstance(conditioning, list):
            return f"{name} is not a CONDITIONING list"
        for index, entry in enumerate(conditioning):
            if not isinstance(entry, (list, tuple)) or len(entry) != 2:
                return f"{name}[{index}] is not a [tensor, metadata] entry"
            if not isinstance(entry[0], torch.Tensor) or entry[0].ndim != 3:
                return f"{name}[{index}] context is not a rank-3 tensor"
            if not isinstance(entry[1], dict):
                return f"{name}[{index}] metadata is not a dictionary"
        return None

    @classmethod
    def build_regional_conditioning(cls: type[Self], conditioning: Any, mask: Any) -> Any:
        """Copy conditioning entries and attach one regional mask."""
        result = cls.copy_conditioning(conditioning)
        for entry in result:
            entry[1]["mask"] = mask
            entry[1]["mask_strength"] = 1.0
            entry[1]["set_area_to_bounds"] = False
        return result

    @classmethod
    def build_couple_region(
        cls: type[Self], main_positive: Any, person_positive: Any, mask: Any
    ) -> Any:
        """Combine global and character conditioning for one logical region."""
        if len(person_positive) != 1 or len(main_positive) > 1:
            raise ValueError("a couple region requires one person entry and at most one Base entry")
        if main_positive:
            from nodes import ConditioningConcat

            region = ConditioningConcat().concat(main_positive, person_positive)[0]
        else:
            region = cls.copy_conditioning(person_positive)
        return cls.build_regional_conditioning(region, mask)

    @staticmethod
    def log_debug_message(message: str) -> Any:
        """Write a debug message when couple-node diagnostics are enabled."""
        if os.environ.get("SAYA_COUPLE_DEBUG", "0") == "1":
            print(f"[Saya Comfy Couple] {message}")

    @staticmethod
    def read_latent_dimensions(latent: Any) -> Any:
        """Validate the latent tensor and return its image-space dimensions."""
        if not isinstance(latent, dict) or "samples" not in latent:
            raise ValueError("Saya Comfy Couple: latent must contain latent['samples']")
        samples = latent["samples"]
        if torch is None or not isinstance(samples, torch.Tensor):
            raise ValueError("Saya Comfy Couple: latent['samples'] must be a torch.Tensor")
        if samples.ndim != 4:
            raise ValueError(
                "Saya Comfy Couple: latent['samples'] must have shape [batch, channels, height, width]"
            )
        batch, _channels, latent_height, latent_width = samples.shape
        if batch < 1 or latent_height < 1 or latent_width < 1:
            raise ValueError(
                "Saya Comfy Couple: latent['samples'] has an empty batch or spatial dimension"
            )
        return (samples, batch, latent_height * 8, latent_width * 8)

    @classmethod
    def build_character_masks(
        cls: type[Self],
        latent: Any,
        use_couple_attention: bool,
        orientation: str,
        center: float,
        swap_person_positions: bool,
    ) -> Any:
        """Build complementary ownership masks for both characters."""
        samples, batch, height, width = cls.read_latent_dimensions(latent)
        dtype = samples.dtype if samples.is_floating_point() else torch.float32
        device = samples.device
        if not use_couple_attention:
            return (
                torch.ones((batch, height, width), device=device, dtype=dtype),
                torch.zeros((batch, height, width), device=device, dtype=dtype),
            )
        axis_size = width if orientation == "horizontal" else height
        coordinate = (torch.arange(axis_size, device=device, dtype=torch.float32) + 0.5) / axis_size
        transition = 0.1
        start = float(center) - transition / 2.0
        ownership = ((start + transition - coordinate) / transition).clamp(0.0, 1.0)
        person_1_axis = 0.1 + 0.8 * ownership
        if swap_person_positions:
            person_1_axis = 1.0 - person_1_axis
        if orientation == "horizontal":
            person_1 = person_1_axis.view(1, 1, width).expand(batch, height, width)
        else:
            person_1 = person_1_axis.view(1, height, 1).expand(batch, height, width)
        person_1 = person_1.to(dtype=dtype).clamp(0.0, 1.0)
        person_2 = (1.0 - person_1).clamp(0.0, 1.0)
        return (person_1, person_2)

    @staticmethod
    def regional_attention_node_class() -> Any:
        """Resolve the regional-attention node lazily to avoid import cycles."""
        return RegionalAttentionNode

    @classmethod
    def patch_model_with_regional_attention(
        cls: type[Self],
        model: Any,
        positive: Any,
        negative: Any,
        branch: str,
        detailer_positive: Any = None,
        public_positive: Any = None,
    ) -> Any:
        """Clone and patch one connected model with regional attention."""
        if model is None:
            return (None, None, None)
        try:
            attention_couple = cls.regional_attention_node_class()()
            cls.log_debug_message(
                f"branch={branch} patch=install regions={len(positive)} native_token_lengths={[x[0].shape[1] for x in positive]} padding=none"
            )
            return attention_couple.attention_couple(
                model,
                positive,
                negative,
                "Attention",
                detailer_positive=detailer_positive,
                public_positive=public_positive,
            )
        except Exception as exc:
            raise RuntimeError(
                f"Saya Comfy Couple: failed to patch connected {branch} model: {exc}"
            ) from exc

    def run(
        self: Self,
        model_main: Any,
        main_positive: Any,
        person_1_positive: Any,
        person_2_positive: Any,
        negative: Any,
        latent: Any,
        use_couple_attention: bool = True,
        orientation: str = "horizontal",
        center: float = 0.5,
        swap_person_positions: bool = False,
        dual_sampling_model: Any = None,
        support_model_1: Any = None,
        support_model_2: Any = None,
        support_model_3: Any = None,
    ) -> Any:
        """Execute the node and return values in declared ComfyUI output order."""
        mask_p1, mask_p2 = self.build_character_masks(
            latent, use_couple_attention, orientation, center, swap_person_positions
        )
        structure_error = next(
            (
                error
                for error in (
                    self.describe_conditioning_error("Base", main_positive),
                    self.describe_conditioning_error("Person 1", person_1_positive),
                    self.describe_conditioning_error("Person 2", person_2_positive),
                    self.describe_conditioning_error("Negative", negative),
                )
                if error
            ),
            None,
        )
        if structure_error:
            raise ValueError(
                f"Saya Comfy Couple: invalid conditioning structure: {structure_error}"
            )
        main_public = self.copy_conditioning(main_positive)
        p1_public = self.copy_conditioning(person_1_positive)
        p2_public = self.copy_conditioning(person_2_positive)
        negative_public = self.copy_conditioning(negative)
        branches = (
            (model_main, "main"),
            (dual_sampling_model, "dual-sampling"),
            (support_model_1, "support 1"),
            (support_model_2, "support 2"),
            (support_model_3, "support 3"),
        )
        counts = (len(main_public), len(p1_public), len(p2_public), len(negative_public))
        fallback_reason = None
        if not use_couple_attention:
            mode = "SOLO"
            if main_public and p1_public:
                from nodes import ConditioningConcat

                positive_final = ConditioningConcat().concat(main_public, p1_public)[0]
            else:
                positive_final = self.copy_conditioning(main_public or p1_public)
            detailer_positive = self.copy_conditioning(p1_public)
        elif not p1_public:
            mode, fallback_reason = ("FALLBACK", "Person 1 conditioning is empty")
        elif not p2_public:
            mode, fallback_reason = ("FALLBACK", "Person 2 conditioning is empty")
        elif not negative_public:
            mode, fallback_reason = ("FALLBACK", "Negative conditioning is empty")
        elif (
            len(main_public) > 1
            or len(p1_public) != 1
            or len(p2_public) != 1
            or (len(negative_public) != 1)
        ):
            mode, fallback_reason = (
                "FALLBACK",
                "multi-entry conditioning cannot form exactly two stable logical regions",
            )
        else:
            mode = "COUPLE"
        if mode == "FALLBACK":
            positive_final = (
                main_public + self.copy_conditioning(p1_public) + self.copy_conditioning(p2_public)
            )
            detailer_positive = self.copy_conditioning(p1_public) + self.copy_conditioning(
                p2_public
            )
        self.log_debug_message(
            f"mode={mode} entries(base,p1,p2,neg)={counts} fallback={fallback_reason or 'none'}"
        )
        self.log_debug_message(
            f"masks p1(min={mask_p1.min().item():.3f},max={mask_p1.max().item():.3f},mean={mask_p1.float().mean().item():.3f}) p2(min={mask_p2.min().item():.3f},max={mask_p2.max().item():.3f},mean={mask_p2.float().mean().item():.3f}) sum(min={(mask_p1 + mask_p2).min().item():.3f},max={(mask_p1 + mask_p2).max().item():.3f},mean={(mask_p1 + mask_p2).float().mean().item():.3f})"
        )
        if mode == "COUPLE":
            region_p1 = self.build_couple_region(main_public, p1_public, mask_p1)
            region_p2 = self.build_couple_region(main_public, p2_public, mask_p2)
            couple_positive = region_p1 + region_p2
            detailer_positive = self.build_regional_conditioning(
                p1_public, mask_p1
            ) + self.build_regional_conditioning(p2_public, mask_p2)
            public_positive = (
                self.copy_conditioning(main_public)
                if main_public
                else self.copy_conditioning(p1_public)
            )
            patched = [
                self.patch_model_with_regional_attention(
                    model,
                    couple_positive,
                    negative_public,
                    branch,
                    detailer_positive=detailer_positive,
                    public_positive=public_positive,
                )
                for model, branch in branches
            ]
            models = tuple((item[0] for item in patched))
            positive_final = self.copy_conditioning(patched[0][1])
            negative_public = self.copy_conditioning(patched[0][2])
            pooled_source = "Base" if main_public else "Person"
            region_summary = [
                (
                    tuple(entry[0].shape),
                    (
                        tuple(entry[1]["pooled_output"].shape)
                        if isinstance(entry[1].get("pooled_output"), torch.Tensor)
                        else None
                    ),
                )
                for entry in couple_positive
            ]
            self.log_debug_message(
                f"logical_regions=2 region(context,pooled)={region_summary} pooled_source={pooled_source} patch=installed"
            )
            if not main_public:
                self.log_debug_message("carrier=P1 reason=Base_absent pooled_global_is_asymmetric")
        else:
            models = tuple((model for model, _branch in branches))
            self.log_debug_message("logical_regions=0 patch=bypassed models=unchanged")
        return models + (positive_final, detailer_positive, negative_public, mask_p1, mask_p2)

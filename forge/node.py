"""Workflow-compatible Saya Comfy Couple node using the Forge Couple engine.

This is deliberately a small Forge-based engine, not another B->L experiment. It keeps
Contact 2.1 exactly at detail resolutions (MAIN+P1 / MAIN+P2 plus MAIN-only in the soft
contact band), then restores a modest MAIN-only share only in coarse attn2 resolutions.
That gives scene/background conditioning a direct low-resolution path without globally
weakening private high-resolution binding. Self-attention remains fully native. The
MASTER node additionally accepts an optional four-conditioning "Naturalize" bank and
returns a second, independently-patched Forge branch for a late img2img cleanup pass.
"""

from __future__ import annotations

import os
from typing import Any, Self

import torch

from .attention_couple import AttentionCouple
from ..src.services.conditioning import copy_conditioning, describe_conditioning_error
from ..src.services.couple_runtime import store_master_payload
from ..src.nodes.text_and_model_routing import resolve_hidream_conditioning

COUPLE_CONFIG_TYPE = "SAYA_COUPLE_CONFIG"
CONFIG_VERSION = 10
_DEBUG_ENV_VAR = "SAYA_COUPLE_DEBUG"
_LOG_PREFIX = "[Saya Forge Couple]"
_ERR_PREFIX = "Saya Forge Couple"

# COARSE MAIN 2.3: deliberately internal so the workflow gets no new sockets.
# At detail resolutions routing is numerically Contact 2.1.
# CONTACT SAFE 2.4: keep character ownership strict away from the seam, but
# give the global MAIN prompt more authority where both characters physically meet.
# This targets fused hands/arms/hips at P1/P2 contact without global prompt bleed.
_SHARED_CONTACT_STRENGTH = 0.90
_CONTACT_WIDTH = 0.24
_COARSE_MAIN_STRENGTH = 0.22
_COARSE_MAX_TOKENS = 1024


def _clamped_float(raw: dict[str, Any], key: str, default: float, low: float, high: float) -> float:
    try:
        value = float(raw.get(key, default))
    except (TypeError, ValueError):
        value = default
    return min(high, max(low, value))



def normalize_couple_config(config: Any = None) -> dict[str, Any]:
    """Normalize only the settings that the Forge baseline actually uses."""
    raw = config if isinstance(config, dict) else {}
    person_2_enabled = bool(raw.get("person_2_enabled", True))
    requested_attention = bool(
        raw.get("requested_use_couple_attention", raw.get("use_couple_attention", True))
    )
    orientation = str(raw.get("orientation", "horizontal"))
    if orientation not in {"horizontal", "vertical"}:
        orientation = "horizontal"

    return {
        "version": CONFIG_VERSION,
        "person_2_enabled": person_2_enabled,
        "use_couple_attention": requested_attention and person_2_enabled,
        "requested_use_couple_attention": requested_attention,
        "orientation": orientation,
        "center": _clamped_float(raw, "center", 0.5, 0.15, 0.85),
        "transition": _clamped_float(raw, "transition", 0.03, 0.01, 0.20),
        "mask_floor": _clamped_float(raw, "mask_floor", 0.0, 0.0, 0.20),
        "swap_person_positions": bool(raw.get("swap_person_positions", False)),
        "shared_contact_strength": _SHARED_CONTACT_STRENGTH,
        "contact_width": _CONTACT_WIDTH,
        "coarse_main_strength": _COARSE_MAIN_STRENGTH,
        "coarse_max_tokens": _COARSE_MAX_TOKENS,
    }


class SayaComfyCoupleForge:
    """Workflow-compatible MASTER surface backed by Forge Couple attn2 routing."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        # Keep only controls that are active in the Forge baseline.
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
                "transition": ("FLOAT", {"default": 0.03, "min": 0.01, "max": 0.20, "step": 0.01}),
                "mask_floor": ("FLOAT", {"default": 0.0, "min": 0.0, "max": 0.20, "step": 0.01}),
                "swap_person_positions": ("BOOLEAN", {"default": False}),
            },
            "optional": {
                "dual_sampling_model": ("MODEL",),
                "support_model_1": ("MODEL",),
                "support_model_2": ("MODEL",),
                "support_model_3": ("MODEL",),
                # Dedicated late-cleanup bank. These are deliberately optional so
                # every existing workflow remains valid. When all four are wired,
                # the MASTER builds a second Forge-patched main-model branch using
                # the exact same couple config/masks, but these softer conditionings.
                "naturalize_main_positive": ("CONDITIONING",),
                "naturalize_person_1_positive": ("CONDITIONING",),
                "naturalize_person_2_positive": ("CONDITIONING",),
                "naturalize_negative": ("CONDITIONING",),
                # Native HiDream conditionings are produced by the same four
                # external prompt encoders. The MASTER receives them only so it
                # can hand them to Phase 03 through Saya's invisible backend.
                "hidream_main_positive": ("CONDITIONING",),
                "hidream_person_1_positive": ("CONDITIONING",),
                "hidream_person_2_positive": ("CONDITIONING",),
                "hidream_negative": ("CONDITIONING",),
            },
        }

    RETURN_TYPES = (
        "MODEL", "MODEL", "MODEL", "MODEL", "MODEL",
        "CONDITIONING", "CONDITIONING", "CONDITIONING",
        "MASK", "MASK", COUPLE_CONFIG_TYPE,
        # Appended only: legacy output slot numbers stay untouched.
        "MODEL", "CONDITIONING", "CONDITIONING",
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
        "couple_config",
        "naturalize_patched_model_main",
        "naturalize_positive_final",
        "naturalize_negative",
    )
    FUNCTION = "run"
    CATEGORY = "saya/rescue"

    # Shared with the legacy couple node; see src/services/conditioning.py.
    copy_conditioning = staticmethod(copy_conditioning)
    describe_conditioning_error = staticmethod(describe_conditioning_error)

    @classmethod
    def build_regional_conditioning(cls: type[Self], conditioning: Any, mask: Any) -> Any:
        result = cls.copy_conditioning(conditioning)
        for entry in result:
            entry[1]["mask"] = mask
            entry[1]["mask_strength"] = 1.0
            entry[1]["set_area_to_bounds"] = False
        return result

    @classmethod
    def build_couple_context(cls: type[Self], main_positive: Any, person_positive: Any) -> Any:
        """Build MAIN + Px exactly through ComfyUI's native ConditioningConcat."""
        if len(person_positive) != 1 or len(main_positive) > 1:
            raise ValueError(
                "Saya Forge Couple: a region requires one person entry and at most one Base entry"
            )
        if main_positive:
            from nodes import ConditioningConcat

            return ConditioningConcat().concat(main_positive, person_positive)[0]
        return cls.copy_conditioning(person_positive)

    @staticmethod
    def log_debug_message(message: str) -> None:
        if os.environ.get(_DEBUG_ENV_VAR, "0") == "1":
            print(f"{_LOG_PREFIX} {message}")

    @staticmethod
    def read_latent_dimensions(latent: Any) -> tuple[torch.Tensor, int, int, int]:
        if not isinstance(latent, dict) or "samples" not in latent:
            raise ValueError(f"{_ERR_PREFIX}: latent must contain latent['samples']")
        samples = latent["samples"]
        if not isinstance(samples, torch.Tensor):
            raise ValueError(f"{_ERR_PREFIX}: latent['samples'] must be a torch.Tensor")
        if samples.ndim != 4:
            raise ValueError(
                f"{_ERR_PREFIX}: latent['samples'] must have shape [batch, channels, height, width]"
            )
        batch, _channels, latent_height, latent_width = samples.shape
        if batch < 1 or latent_height < 1 or latent_width < 1:
            raise ValueError(f"{_ERR_PREFIX}: latent contains an empty dimension")
        return samples, batch, latent_height * 8, latent_width * 8

    @classmethod
    def build_character_masks(
        cls: type[Self],
        latent: Any,
        use_couple_attention: bool,
        orientation: str,
        center: float,
        transition: float,
        mask_floor: float,
        swap_person_positions: bool,
    ) -> tuple[torch.Tensor, torch.Tensor]:
        """Keep Saya's existing complementary soft geometry; no dynamic routing."""
        samples, batch, height, width = cls.read_latent_dimensions(latent)
        dtype = samples.dtype if samples.is_floating_point() else torch.float32
        device = samples.device
        if not use_couple_attention:
            return (
                torch.ones((batch, height, width), device=device, dtype=dtype),
                torch.zeros((batch, height, width), device=device, dtype=dtype),
            )

        orientation = orientation if orientation in {"horizontal", "vertical"} else "horizontal"
        axis_size = width if orientation == "horizontal" else height
        coordinate = (torch.arange(axis_size, device=device, dtype=torch.float32) + 0.5) / axis_size
        transition = min(0.20, max(0.01, float(transition)))
        mask_floor = min(0.20, max(0.0, float(mask_floor)))
        start = float(center) - transition / 2.0
        ownership = ((start + transition - coordinate) / transition).clamp(0.0, 1.0)
        person_1_axis = mask_floor + (1.0 - 2.0 * mask_floor) * ownership
        if swap_person_positions:
            person_1_axis = 1.0 - person_1_axis

        if orientation == "horizontal":
            person_1 = person_1_axis.view(1, 1, width).expand(batch, height, width)
        else:
            person_1 = person_1_axis.view(1, height, 1).expand(batch, height, width)
        person_1 = person_1.to(dtype=dtype).clamp(0.0, 1.0)
        person_2 = (1.0 - person_1).clamp(0.0, 1.0)
        return person_1, person_2

    @classmethod
    def build_forge_routing_masks(
        cls: type[Self],
        latent: Any,
        mask_p1: torch.Tensor,
        mask_p2: torch.Tensor,
        orientation: str,
        center: float,
        shared_contact_strength: float,
        contact_width: float,
        *,
        global_enabled: bool,
    ) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
        """Build MAIN-only contact weight + two MAIN+Px private weights.

        Critical property: OUTSIDE the contact band the MAIN-only branch has exactly
        zero weight and P1/P2 are exactly the normal Forge baseline masks. Therefore
        background/scene conditioning is not diluted by a permanently-active third
        branch. Inside contact, a bounded share is transferred to MAIN-only so pose
        and shared interaction can dominate without hard-assigning the overlap.
        """
        samples, batch, height, width = cls.read_latent_dimensions(latent)
        dtype = mask_p1.dtype
        device = mask_p1.device

        strength = min(0.95, max(0.0, float(shared_contact_strength))) if global_enabled else 0.0
        cw = min(0.50, max(0.0, float(contact_width)))
        center = min(0.85, max(0.15, float(center)))
        orientation = orientation if orientation in {"horizontal", "vertical"} else "horizontal"

        if not global_enabled or strength <= 0.0 or cw <= 0.0:
            total = mask_p1 + mask_p2
            return torch.zeros_like(mask_p1), mask_p1 / total, mask_p2 / total

        axis_size = width if orientation == "horizontal" else height
        coordinate = (torch.arange(axis_size, device=device, dtype=torch.float32) + 0.5) / axis_size
        half = max(cw / 2.0, 1.0 / max(axis_size, 1))
        t = (1.0 - (coordinate - center).abs() / half).clamp(0.0, 1.0)
        t = t * t * (3.0 - 2.0 * t)
        if orientation == "horizontal":
            contact = t.view(1, 1, width).expand(batch, height, width)
        else:
            contact = t.view(1, height, 1).expand(batch, height, width)
        contact = contact.to(dtype=dtype)

        # MAIN-only is contact-only. Outside contact: shared=0, private_budget=1,
        # making the routing numerically identical to the original two-region baseline.
        shared = (contact * strength).clamp(0.0, 0.95)
        private_budget = 1.0 - shared
        private_p1 = private_budget * mask_p1
        private_p2 = private_budget * mask_p2

        total = shared + private_p1 + private_p2
        if torch.any(total <= 0):
            raise RuntimeError("Saya Forge Couple: invalid contact-only routing mask")
        return shared / total, private_p1 / total, private_p2 / total

    @classmethod
    def patch_model_with_forge(
        cls: type[Self],
        model: Any,
        private_p1: Any,
        private_p2: Any,
        base_mask: torch.Tensor,
        mask_p1: torch.Tensor,
        mask_p2: torch.Tensor,
        branch: str,
        coarse_main_strength: float,
        coarse_max_tokens: int,
    ) -> Any:
        if model is None:
            return None
        try:
            # Forge uses one spatial mask shared across the image batch and repeats
            # it in the output patch. Saya's generated geometry is identical for all
            # batch members, so use the first copy here.
            kwargs = {
                "cond_1": private_p1,
                "mask_1": mask_p1[:1],
                "cond_2": private_p2,
                "mask_2": mask_p2[:1],
            }
            cls.log_debug_message(
                f"branch={branch} engine=forge branches=MAIN+P1+P2 "
                f"tokens=({private_p1[0][0].shape[1]},{private_p2[0][0].shape[1]})"
            )
            return AttentionCouple.patch_unet(
                model,
                base_mask,
                kwargs,
                coarse_main_strength=coarse_main_strength,
                coarse_max_tokens=coarse_max_tokens,
            )
        except Exception as exc:
            raise RuntimeError(f"{_ERR_PREFIX}: failed to patch {branch}: {exc}") from exc

    def _patch_branches(
        self,
        branches: tuple[tuple[Any, str], ...],
        *,
        private_p1: Any,
        private_p2: Any,
        base_mask: torch.Tensor,
        mask_p1: torch.Tensor,
        mask_p2: torch.Tensor,
        coarse_main_strength: float,
        coarse_max_tokens: int,
    ) -> tuple[Any, ...]:
        patched: list[Any] = []
        by_identity: dict[int, Any] = {}
        for model, branch in branches:
            if model is None:
                patched.append(None)
                continue
            identity = id(model)
            if identity in by_identity:
                self.log_debug_message(f"branch={branch} patch=reuse identical_model")
                patched.append(by_identity[identity])
                continue
            result = self.patch_model_with_forge(
                model,
                private_p1,
                private_p2,
                base_mask,
                mask_p1,
                mask_p2,
                branch,
                coarse_main_strength,
                coarse_max_tokens,
            )
            by_identity[identity] = result
            patched.append(result)
        return tuple(patched)

    def _run_with_config(
        self: Self,
        model_main: Any,
        main_positive: Any,
        person_1_positive: Any,
        person_2_positive: Any,
        negative: Any,
        latent: Any,
        config: Any,
        dual_sampling_model: Any = None,
        support_model_1: Any = None,
        support_model_2: Any = None,
        support_model_3: Any = None,
    ) -> Any:
        config = normalize_couple_config(config)
        person_2_enabled = bool(config["person_2_enabled"])
        use_couple_attention = bool(config["use_couple_attention"])

        mask_p1, mask_p2 = self.build_character_masks(
            latent,
            use_couple_attention,
            str(config["orientation"]),
            float(config["center"]),
            float(config["transition"]),
            float(config["mask_floor"]),
            bool(config["swap_person_positions"]),
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
            raise ValueError(f"{_ERR_PREFIX}: invalid conditioning structure: {structure_error}")

        main_public = self.copy_conditioning(main_positive)
        p1_public = self.copy_conditioning(person_1_positive)
        p2_public = self.copy_conditioning(person_2_positive) if person_2_enabled else []
        negative_public = self.copy_conditioning(negative)
        branches = (
            (model_main, "main"),
            (dual_sampling_model, "dual-sampling"),
            (support_model_1, "support 1"),
            (support_model_2, "support 2"),
            (support_model_3, "support 3"),
        )

        # Keep v21.7L's public fallback behavior. Only the COUPLE engine changes.
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
            mode, fallback_reason = "FALLBACK", "Person 1 conditioning is empty"
        elif not p2_public:
            mode, fallback_reason = "FALLBACK", "Person 2 conditioning is empty"
        elif not negative_public:
            mode, fallback_reason = "FALLBACK", "Negative conditioning is empty"
        elif len(main_public) > 1 or len(p1_public) != 1 or len(p2_public) != 1 or len(negative_public) != 1:
            mode, fallback_reason = (
                "FALLBACK",
                "multi-entry conditioning cannot form exactly two stable logical regions",
            )
        else:
            mode = "COUPLE"

        if mode == "FALLBACK":
            positive_final = main_public + self.copy_conditioning(p1_public) + self.copy_conditioning(p2_public)
            detailer_positive = self.copy_conditioning(p1_public) + self.copy_conditioning(p2_public)

        if mode == "COUPLE":
            # Preserve the baseline's scene/background strength: each regional branch
            # carries MAIN + its private prompt. MAIN-only exists solely in contact.
            p1_context = self.build_couple_context(main_public, p1_public)
            p2_context = self.build_couple_context(main_public, p2_public)
            positive_final = self.copy_conditioning(main_public or p1_public)
            base_mask, route_p1, route_p2 = self.build_forge_routing_masks(
                latent,
                mask_p1,
                mask_p2,
                str(config["orientation"]),
                float(config["center"]),
                float(config["shared_contact_strength"]),
                float(config["contact_width"]),
                global_enabled=bool(main_public),
            )

            models = self._patch_branches(
                branches,
                private_p1=p1_context,
                private_p2=p2_context,
                base_mask=base_mask,
                mask_p1=route_p1,
                mask_p2=route_p2,
                coarse_main_strength=(float(config["coarse_main_strength"]) if main_public else 0.0),
                coarse_max_tokens=int(config["coarse_max_tokens"]),
            )

            # Detailers/IPAdapter keep the original character masks. The new shared
            # contact weighting exists only inside Forge attention routing.
            detailer_positive = self.build_regional_conditioning(
                p1_public, mask_p1
            ) + self.build_regional_conditioning(p2_public, mask_p2)

            self.log_debug_message(
                "mode=COUPLE engine=forge-coarse-main23 attn1=native "
                f"shared_contact={config['shared_contact_strength']:.3f} "
                f"contact_width={config['contact_width']:.3f} "
                f"coarse_main={config['coarse_main_strength']:.3f} "
                f"coarse_max_tokens={config['coarse_max_tokens']} detail_routing=contact21"
            )
        else:
            models = tuple(model for model, _branch in branches)
            self.log_debug_message(
                f"mode={mode} engine=bypass fallback={fallback_reason or 'none'}"
            )

        return models + (
            positive_final,
            detailer_positive,
            negative_public,
            mask_p1,
            mask_p2,
            dict(config),
        )

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
        transition: float = 0.03,
        mask_floor: float = 0.0,
        swap_person_positions: bool = False,
        dual_sampling_model: Any = None,
        support_model_1: Any = None,
        support_model_2: Any = None,
        support_model_3: Any = None,
        naturalize_main_positive: Any = None,
        naturalize_person_1_positive: Any = None,
        naturalize_person_2_positive: Any = None,
        naturalize_negative: Any = None,
        hidream_main_positive: Any = None,
        hidream_person_1_positive: Any = None,
        hidream_person_2_positive: Any = None,
        hidream_negative: Any = None,
    ) -> Any:
        # The four SayaDualCLIPTextEncode nodes defer their HiDream branch and
        # hand us inert request tokens. ComfyUI cannot run this MASTER until all
        # four have executed, so resolving here -- before any SDXL UNet work --
        # flushes every pending HiDream prompt in ONE quad-CLIP load instead of
        # a per-node SDXL<->HiDream ping-pong. Plain (non-request) values pass
        # straight through.
        hidream_main_positive = resolve_hidream_conditioning(hidream_main_positive)
        hidream_person_1_positive = resolve_hidream_conditioning(hidream_person_1_positive)
        hidream_person_2_positive = resolve_hidream_conditioning(hidream_person_2_positive)
        hidream_negative = resolve_hidream_conditioning(hidream_negative)

        config = normalize_couple_config(
            {
                "person_2_enabled": bool(person_2_positive),
                "use_couple_attention": use_couple_attention,
                "orientation": orientation,
                "center": center,
                "transition": transition,
                "mask_floor": mask_floor,
                "swap_person_positions": swap_person_positions,
            }
        )
        normal = self._run_with_config(
            model_main,
            main_positive,
            person_1_positive,
            person_2_positive,
            negative,
            latent,
            config,
            dual_sampling_model,
            support_model_1,
            support_model_2,
            support_model_3,
        )

        naturalize_ready = all(
            bool(value)
            for value in (
                naturalize_main_positive,
                naturalize_person_1_positive,
                naturalize_person_2_positive,
                naturalize_negative,
            )
        )
        if naturalize_ready:
            # Build an independent main-model Forge patch using the exact same
            # geometric config/masks. Do NOT patch dual/support branches here: the
            # late naturalize sampler only needs the main model and this keeps the
            # extra branch cheap and isolated from the normal pipeline.
            naturalize = self._run_with_config(
                model_main,
                naturalize_main_positive,
                naturalize_person_1_positive,
                naturalize_person_2_positive,
                naturalize_negative,
                latent,
                config,
                None,
                None,
                None,
                None,
            )
            naturalize_model = naturalize[0]
            naturalize_positive = naturalize[5]
            naturalize_negative_out = naturalize[7]
            self.log_debug_message("naturalize=enabled independent_forge_main_branch=true")
        else:
            # Backward-compatible fallback for old workflows. The appended outputs
            # mirror the normal branch when the four dedicated sockets are unwired.
            naturalize_model = normal[0]
            naturalize_positive = normal[5]
            naturalize_negative_out = normal[7]
            self.log_debug_message("naturalize=disabled fallback=normal_outputs")

        special_routing_requested = naturalize_ready or any(
            value is not None and value != []
            for value in (
                hidream_main_positive,
                hidream_person_1_positive,
                hidream_person_2_positive,
                hidream_negative,
            )
        )
        if special_routing_requested:
            store_master_payload(
                hidream_main=hidream_main_positive,
                hidream_person_1=hidream_person_1_positive,
                hidream_person_2=hidream_person_2_positive,
                hidream_negative=hidream_negative,
                naturalize_model=naturalize_model,
                naturalize_positive=naturalize_positive,
                naturalize_negative=naturalize_negative_out,
                naturalize_ready=naturalize_ready,
            )

        return normal + (
            naturalize_model,
            naturalize_positive,
            naturalize_negative_out,
        )


class SayaComfyCoupleForgeCopy(SayaComfyCoupleForge):
    """COPY surface: no visible tuning settings; it reuses the MASTER config."""

    # Keep COPY byte-contract compatible with every existing workflow phase.
    # Only the visible MASTER gets the 3 appended Naturalize outputs.
    RETURN_TYPES = (
        "MODEL", "MODEL", "MODEL", "MODEL", "MODEL",
        "CONDITIONING", "CONDITIONING", "CONDITIONING",
        "MASK", "MASK", COUPLE_CONFIG_TYPE,
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
        "couple_config",
    )

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        return {
            "required": {
                "model_main": ("MODEL",),
                "main_positive": ("CONDITIONING",),
                "person_1_positive": ("CONDITIONING",),
                "person_2_positive": ("CONDITIONING",),
                "negative": ("CONDITIONING",),
                "latent": ("LATENT",),
                "couple_config": (COUPLE_CONFIG_TYPE,),
            },
            "optional": {
                "dual_sampling_model": ("MODEL",),
                "support_model_1": ("MODEL",),
                "support_model_2": ("MODEL",),
                "support_model_3": ("MODEL",),
            },
        }

    FUNCTION = "run_copy"

    def run_copy(
        self: Self,
        model_main: Any,
        main_positive: Any,
        person_1_positive: Any,
        person_2_positive: Any,
        negative: Any,
        latent: Any,
        couple_config: Any,
        dual_sampling_model: Any = None,
        support_model_1: Any = None,
        support_model_2: Any = None,
        support_model_3: Any = None,
    ) -> Any:
        return self._run_with_config(
            model_main,
            main_positive,
            person_1_positive,
            person_2_positive,
            negative,
            latent,
            couple_config,
            dual_sampling_model,
            support_model_1,
            support_model_2,
            support_model_3,
        )

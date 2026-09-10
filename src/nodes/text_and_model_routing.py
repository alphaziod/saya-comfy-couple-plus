"""Text encoding and hires model-routing ComfyUI nodes."""

from __future__ import annotations

from typing import Any, Self

from ..services.models import build_model_choice_list, load_vae_or_fallback


class DualClipTextEncoderNode:
    """Encode prompt text with an explicit disabled output state."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        """Return the ComfyUI input schema exposed by this node."""
        return {
            "required": {
                "clip": ("CLIP",),
                "text": ("STRING", {"multiline": True, "default": ""}),
                "send_data": ("BOOLEAN", {"default": True}),
            }
        }

    RETURN_TYPES = ("CONDITIONING",)
    RETURN_NAMES = ("conditioning",)
    FUNCTION = "encode"
    CATEGORY = "saya/rescue"

    def encode(self: Self, clip: Any, text: str, send_data: bool = True) -> Any:
        """Encode the node input and return values in ComfyUI output order."""
        if not send_data:
            return ([],)
        tokens = clip.tokenize(text)
        return (clip.encode_from_tokens_scheduled(tokens),)


class HiresModelRouterNode:
    """Route shared conditioning, models, and VAEs through hires stages."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        """Return the ComfyUI input schema exposed by this node."""
        vaes = build_model_choice_list(
            "vae",
            [
                "AAA%20Anime%20VAE%20SDXL%20v2.safetensors",
                "Dark%20VAE%20SDXL%20Weak.safetensors",
                "crystalVAESDXL_vaeV3.safetensors",
            ],
        )
        source = ["main", "dual_sampling", "support1", "usdu1", "usdu2"]
        vae_choice = [
            "none",
            "main",
            "dual_sampling",
            "support1",
            "usdu1",
            "usdu2",
            "custom_vae_1",
            "custom_vae_2",
            "custom_vae_3",
        ]
        req = {
            "positive": ("CONDITIONING",),
            "negative": ("CONDITIONING",),
            "main_model": ("MODEL",),
            "main_vae": ("VAE",),
            "dual_sampling_model": ("MODEL",),
            "dual_sampling_vae": ("VAE",),
            "support1_model": ("MODEL",),
            "support1_vae": ("VAE",),
            "usdu1_model": ("MODEL",),
            "usdu1_vae": ("VAE",),
            "usdu2_model": ("MODEL",),
            "usdu2_vae": ("VAE",),
            "━━ CUSTOM VAE ━━": ("STRING", {"default": "━━ CUSTOM VAE ━━"}),
            "custom_vae_1": (vaes,),
            "custom_vae_2": (vaes,),
            "custom_vae_3": (vaes,),
        }
        for name in ["base", "mid", "final", "last"]:
            req[f"━━ {name.upper()} HIRES ━━"] = (
                "STRING",
                {"default": f"━━ {name.upper()} HIRES ━━"},
            )
            req[f"{name}_source"] = (source, {"default": "main"})
            req[f"{name}_vae"] = (vae_choice, {"default": "none"})
        return {"required": req}

    RETURN_TYPES = (
        "CONDITIONING",
        "CONDITIONING",
        "MODEL",
        "VAE",
        "MODEL",
        "VAE",
        "MODEL",
        "VAE",
        "MODEL",
        "VAE",
    )
    RETURN_NAMES = (
        "positive",
        "negative",
        "base_model",
        "base_vae",
        "mid_model",
        "mid_vae",
        "final_model",
        "final_vae",
        "last_model",
        "last_vae",
    )
    FUNCTION = "route"
    CATEGORY = "saya/rescue"

    def route(self: Self, **kw: Any) -> Any:
        """Route node inputs to the selected output path."""
        models = {
            "main": kw["main_model"],
            "dual_sampling": kw["dual_sampling_model"],
            "support1": kw["support1_model"],
            "usdu1": kw["usdu1_model"],
            "usdu2": kw["usdu2_model"],
        }
        vaes = {
            "main": kw["main_vae"],
            "dual_sampling": kw["dual_sampling_vae"],
            "support1": kw["support1_vae"],
            "usdu1": kw["usdu1_vae"],
            "usdu2": kw["usdu2_vae"],
        }
        vaes["custom_vae_1"] = load_vae_or_fallback(kw.get("custom_vae_1"), kw["main_vae"])
        vaes["custom_vae_2"] = load_vae_or_fallback(kw.get("custom_vae_2"), kw["main_vae"])
        vaes["custom_vae_3"] = load_vae_or_fallback(kw.get("custom_vae_3"), kw["main_vae"])
        out = [kw["positive"], kw["negative"]]
        for name in ["base", "mid", "final", "last"]:
            src = kw.get(f"{name}_source", "main")
            vc = kw.get(f"{name}_vae", "none")
            out.append(models.get(src, kw["main_model"]))
            out.append(vaes.get(src if vc == "none" else vc, kw["main_vae"]))
        return tuple(out)

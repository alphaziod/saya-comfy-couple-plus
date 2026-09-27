"""HiDream-only diffusion LoRA, with independent lightweight trigger controls."""

import logging

import comfy.model_base
import comfy.sd
import comfy.utils
import folder_paths

LOGGER = logging.getLogger(__name__)


class SayaHiDreamLoraSettings:
    """Turn LoRA widgets into one config dict, plus the trigger only when active."""

    @classmethod
    def INPUT_TYPES(cls):
        return {"required": {
            "enabled": ("BOOLEAN", {"default": False}),
            "lora_name": (["none", *folder_paths.get_filename_list("loras")], {"tooltip": "Ignored while enabled is off or strength_model is 0."}),
            "strength_model": ("FLOAT", {"default": 1.0, "min": -20.0, "max": 20.0, "step": 0.05, "tooltip": "0 disables the LoRA, like enabled=off."}),
            "trigger_text": ("STRING", {"default": "", "multiline": True, "tooltip": "Folded into the HiDream positives only while the LoRA is active; dropped otherwise."}),
        }}

    RETURN_TYPES = ("SAYA_HIDREAM_LORA", "STRING")
    RETURN_NAMES = ("lora_config", "hidream_trigger")
    FUNCTION = "configure"
    CATEGORY = "saya/hidream"

    def configure(self, enabled, lora_name, strength_model, trigger_text):
        active = bool(enabled and lora_name != "none" and strength_model != 0)
        return ({"enabled": active, "lora_name": lora_name,
                 "strength_model": strength_model},
                trigger_text.strip() if active else "")


class SayaHiDreamLoraLoader:
    """Apply the configured LoRA to a HiDream MODEL via ComfyUI's own cloning loader.

    Passes the MODEL straight through, untouched, when the config says the
    LoRA is disabled/zero-strength/unset.
    """

    @classmethod
    def INPUT_TYPES(cls):
        return {"required": {
            "model": ("MODEL",),
            "lora_config": ("SAYA_HIDREAM_LORA",),
        }}

    RETURN_TYPES = ("MODEL",)
    RETURN_NAMES = ("hidream_model",)
    FUNCTION = "load_lora"
    CATEGORY = "saya/hidream"

    def load_lora(self, model, lora_config):
        if not isinstance(model.model, comfy.model_base.HiDream):
            raise ValueError("Saya HiDream LoRA: connect a HiDream MODEL, not an SDXL/Naturalize model.")
        enabled = lora_config["enabled"]
        name = lora_config["lora_name"]
        strength = lora_config["strength_model"]
        LOGGER.info("[SAYA HIDREAM LORA] enabled=%s name=%r strength=%s", enabled, name, strength)
        if not enabled or strength == 0 or name == "none":
            return (model,)
        path = folder_paths.get_full_path_or_raise("loras", name)
        lora = comfy.utils.load_torch_file(path, safe_load=True)
        # Native API clones the patcher; GGUFModelPatcher.clone preserves its
        # quantized-weight patch implementation. No CLIP is passed or patched.
        patched, _ = comfy.sd.load_lora_for_models(model, None, lora, strength, 0.0)
        before = sum(len(p) for p in model.patches.values())
        after = sum(len(p) for p in patched.patches.values())
        if after <= before:
            raise ValueError(f"Saya HiDream LoRA: no compatible MODEL weights in {name!r}.")
        LOGGER.info("[SAYA HIDREAM LORA] patched_weights=%d", after - before)
        return (patched,)

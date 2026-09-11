"""HiDream-only diffusion LoRA, with independent lightweight trigger controls."""

import comfy.model_base
import comfy.sd
import comfy.utils
import folder_paths


class SayaHiDreamLoraSettings:
    @classmethod
    def INPUT_TYPES(cls):
        return {"required": {
            "enabled": ("BOOLEAN", {"default": False}),
            "lora_name": (["none", *folder_paths.get_filename_list("loras")],),
            "strength_model": ("FLOAT", {"default": 1.0, "min": -20.0, "max": 20.0, "step": 0.05}),
            "trigger_text": ("STRING", {"default": "", "multiline": True}),
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
        print(f"[SAYA HIDREAM LORA] enabled={enabled} name={name!r} strength={strength}", flush=True)
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
        print(f"[SAYA HIDREAM LORA] patched_weights={after - before}", flush=True)
        return (patched,)

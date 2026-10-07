"""Saya LoRA Family Filter: the LoRA stack of Model 1 applied to Model 2, keeping only the LoRAs its family can take.

An Illustrious model takes some SDXL LoRAs, the reverse is to be avoided. The family of each LoRA is the
``base_model`` LoRA Manager wrote in ``<lora>.metadata.json``; the family of the Model 2 checkpoint is its own
``base_model``, or, when that is missing / "Unknown", read from its folder ("SDXL 1.0/anime/...").

Rules (Model 2 family -> LoRAs that pass):
* SDXL        -> SDXL only.
* Illustrious -> Illustrious and SDXL.
* other / unknown family -> every LoRA (for now).
A LoRA whose family is unknown always passes, with a line in the report.
Only the UNet of Model 2 is patched (model strength): the CLIP is Model 1's and is never touched here.
"""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

SDXL, ILLUSTRIOUS = "sdxl", "illustrious"
ALLOWED = {SDXL: {SDXL}, ILLUSTRIOUS: {ILLUSTRIOUS, SDXL}}
LORA_EXTS = (".safetensors", ".ckpt", ".pt", ".bin")
# Folder / base_model spellings -> family (first match wins, so "illustrious" beats the "sdxl" of "waiIllustriousSDXL").
FAMILY_PATTERNS = (("illustrious", ILLUSTRIOUS), (r"noob", "noobai"), (r"pony", "pony"), (r"sd\s*1\.?5", "sd15"),
                   (r"flux", "flux"), (r"hidream", "hidream"), (r"sdxl", SDXL))
LORA_TAG = re.compile(r"<lora:([^:>]+):([-\d.]+)(?::([-\d.]+))?>")


def family_of(text: str | None) -> str | None:
    if not text or text.strip().lower() in ("", "unknown"):
        return None
    low = text.lower()
    for pattern, family in FAMILY_PATTERNS:
        if re.search(pattern, low):
            return family
    return low.strip()


def _metadata_family(model_path: Path) -> str | None:
    meta = model_path.with_name(model_path.stem + ".metadata.json")
    try:
        return family_of(json.loads(meta.read_text(encoding="utf-8")).get("base_model"))
    except (OSError, ValueError, AttributeError):
        return None


def checkpoint_family(ckpt_name: str, ckpt_path: Path | None) -> tuple[str | None, str]:
    """(family, where it was read): metadata first, then the folders of the checkpoint name."""
    if ckpt_path is not None:
        family = _metadata_family(ckpt_path)
        if family:
            return family, "metadata"
    folders = str(Path(ckpt_name.replace("\\", "/")).parent)
    family = family_of(folders) if folders not in ("", ".") else None
    return (family, "folder") if family else (None, "unknown")


def parse_loras(*texts: str) -> list[tuple[str, float]]:
    """<lora:name:model[:clip]> tags of LoRA Manager's loaded_loras outputs (or typed by hand) -> (name, model strength)."""
    return [(m.group(1).strip(), float(m.group(2))) for text in texts if text for m in LORA_TAG.finditer(text)]


def find_lora(name: str, roots: list[Path]) -> Path | None:
    """LoRA Manager names are the file stem (sometimes with folders)."""
    name = name.replace("\\", "/")
    for ext in LORA_EXTS:
        if name.lower().endswith(ext):
            name = name[: -len(ext)]
    stem = name.rsplit("/", 1)[-1]
    fallback = None
    for root in roots:
        for ext in LORA_EXTS:
            direct = root / (name + ext)
            if direct.is_file():
                return direct
        for path in root.rglob(stem + ".*"):
            if path.suffix.lower() in LORA_EXTS and path.stem == stem:
                fallback = fallback or path
    return fallback


def decide(ckpt_family: str | None, lora_family: str | None) -> bool:
    if lora_family is None or ckpt_family not in ALLOWED:
        return True
    return lora_family in ALLOWED[ckpt_family]


def _apply_lora(model: Any, path: Path, strength: float) -> Any:
    import comfy.sd
    import comfy.utils
    lora = comfy.utils.load_torch_file(str(path), safe_load=True)
    return comfy.sd.load_lora_for_models(model, None, lora, strength, 0.0)[0]


def _lora_roots() -> list[Path]:
    import folder_paths
    return [Path(p) for p in folder_paths.get_folder_paths("loras")]


def _ckpt_path(ckpt_name: str) -> Path | None:
    import folder_paths
    path = folder_paths.get_full_path("checkpoints", ckpt_name)
    return Path(path) if path else None


def _ckpt_choices() -> list[str]:
    try:
        import folder_paths
        return folder_paths.get_filename_list("checkpoints")
    except Exception:
        return []


class SayaLoraFamilyFilter:
    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "model": ("MODEL", {"tooltip": "Model 2, as loaded (before any LoRA)."}),
                "ckpt_name": (_ckpt_choices(), {"tooltip": "The Model 2 checkpoint: its family decides which LoRAs pass."}),
                "enabled": ("BOOLEAN", {"default": True, "tooltip": "OFF = Model 2 returned untouched."}),
            },
            "optional": {
                "ckpt_name_in": ("STRING", {"forceInput": True, "tooltip": "Model 2 checkpoint name as text (e.g. Model 2 Identifier): wins over ckpt_name."}),
                "loaded_loras_1": ("STRING", {"forceInput": True, "tooltip": "loaded_loras output of a LoRA Manager loader of Model 1."}),
                "loaded_loras_2": ("STRING", {"forceInput": True}),
                "loaded_loras_3": ("STRING", {"forceInput": True}),
                "loaded_loras_4": ("STRING", {"forceInput": True}),
                "loaded_loras_5": ("STRING", {"forceInput": True}),
                "extra_loras": ("STRING", {"default": "", "multiline": True, "tooltip": "More LoRAs, <lora:name:strength>."}),
            },
        }

    RETURN_TYPES = ("MODEL", "STRING")
    RETURN_NAMES = ("MODEL", "report")
    FUNCTION = "filter"
    CATEGORY = "saya/couple"
    DESCRIPTION = ("Model 1's LoRAs on Model 2, filtered by family (LoRA Manager base_model): SDXL model -> SDXL LoRAs only, "
                   "Illustrious model -> Illustrious + SDXL, other family -> all. UNet only.")

    def filter(self, model: Any, ckpt_name: str, enabled: bool = True, ckpt_name_in: str = "", loaded_loras_1: str = "", loaded_loras_2: str = "",
               loaded_loras_3: str = "", loaded_loras_4: str = "", loaded_loras_5: str = "", extra_loras: str = "",
               _roots: list[Path] | None = None, _ckpt: Path | None = None, _apply=None) -> tuple[Any, str]:
        if not enabled:
            return model, "LoRA family filter OFF: Model 2 untouched"
        ckpt_name = (ckpt_name_in or "").strip() or ckpt_name
        roots = _roots if _roots is not None else _lora_roots()
        ckpt_family, source = checkpoint_family(ckpt_name, _ckpt if _ckpt is not None else _ckpt_path(ckpt_name))
        rule = "/".join(sorted(ALLOWED[ckpt_family])) if ckpt_family in ALLOWED else "all (family not filtered yet)"
        lines = [f"Model 2 {ckpt_name}: family {ckpt_family or 'unknown'} ({source}) -> LoRAs {rule}"]
        apply = _apply or _apply_lora
        seen = set()
        for name, strength in parse_loras(loaded_loras_1, loaded_loras_2, loaded_loras_3, loaded_loras_4, loaded_loras_5, extra_loras):
            if name in seen:
                continue
            seen.add(name)
            path = find_lora(name, roots)
            if path is None:
                lines.append(f"  SKIP {name}: file not found")
                continue
            lora_family = _metadata_family(path)
            if not decide(ckpt_family, lora_family):
                lines.append(f"  BLOCK {name} [{lora_family}]")
                continue
            if strength == 0:
                lines.append(f"  SKIP {name}: strength 0")
                continue
            model = apply(model, path, strength)
            note = "" if lora_family else " (family unknown: passed)"
            lines.append(f"  PASS {name} [{lora_family or '?'}] {strength}{note}")
        if len(lines) == 1:
            lines.append("  no LoRA given")
        return model, "\n".join(lines)

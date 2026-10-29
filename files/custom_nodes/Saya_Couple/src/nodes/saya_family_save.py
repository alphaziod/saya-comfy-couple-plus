"""SayaFamilySave — family-aware PNG saver (ComfyUI OUTPUT node).

Saves images with the STANDARD ComfyUI metadata — tEXt ``prompt`` (API graph)
plus every ``extra_pnginfo`` key (=> the ``workflow`` UI graph), byte-for-byte
the native SaveImage contract — so Saya Studio's PNG reader
(``crates/studio-core/src/image_restore.rs``: tEXt/zTXt/iTXt by keyword,
``prompt`` / ``workflow`` / ``parameters``) keeps working unchanged. On top of
that, and ADDITIVELY ONLY (an unknown chunk key is just listed by ``inspect``):
a tEXt chunk ``saya_family`` carrying the canonical identity JSON, plus the
same JSON written as a sidecar file ``<png stem>.saya_family.json`` beside the
image (the pack counterpart of generez's ``<stem>.workflow.json`` habit).

Identity contract (Saya, 2026-10-25):

* ``perso_id`` is the COMPLETE character stem (``ch--0001-nelia-thorne``),
  never the bare number: numbering restarts at every generez batch, the stem
  does not. The widget stays free text — legacy ``ch`` stems are kept as-is.
* The file stem stays CANONICAL: this node never renumbers and never aliases
  ``ch`` -> ``hum`` (that is ``registre_ids.py``'s job, and this node is never
  part of the generez flow — generez keeps its own writer/embedding).
* Files land in ``<output>/saya[/<lot>]/<famille>/<stem>_<counter:05>_.png``
  through ComfyUI's own ``folder_paths.get_save_image_path``: standard
  counter, never overwrites an existing file, path jail-checked inside the
  output directory. ``filename_prefix`` set = native override (subfolder and
  ``%width%``-style vars), the family fields stay in the metadata/sidecar.

``saya_family`` JSON fields: famille, prefix_id, perso_id (stem), numero, nom,
lot, stem, modele (``noobai`` / ``illustrious`` short tag derived from the
checkpoint filename, else its slug), fichier, seed (executed value from the
API graph, null when the graph only carries a link), checkpoints, loras,
saya_couple_version (read from the saved workflow's ``extra``, else null),
date (ISO). ``sort_keys`` + ``ensure_ascii`` keep the chunk reproducible and
latin-1-safe (tEXt, not iTXt).
"""

from __future__ import annotations

import json
import logging
import os
import re
import unicodedata
from datetime import datetime
from pathlib import Path
from typing import Any

import numpy as np
import torch
from PIL import Image, PngImagePlugin

LOGGER = logging.getLogger(__name__)

#: Families of generez ``moteur/registre_ids.py`` (read 2026-10-25). Legacy
#: ``ch`` is deliberately NOT offered: since the 2026-10-20 rule it only
#: survives inside historical stems, and new positive saves pick ``hum``.
FAMILLES = ("hum", "bny", "kmn", "slm", "lcs", "cat", "dem", "elf", "cute", "edgy", "hor", "ard", "div", "sop", "lra")

#: Keyword of the additive tEXt chunk; also the sidecar suffix stem.
FAMILY_CHUNK_KEY = "saya_family"

#: ``<png stem>.saya_family.json`` — JSON sidecar written beside the image.
FAMILY_SIDECAR_SUFFIX = ".saya_family.json"

#: Parse a canonical stem: famille, number, name, optional model short tag
#: (``hum--0015-nelia-thorne``, ``ch-0009-meche-sur-l-oeil-illustrious``, …).
STEM_RE = re.compile(r"^(?P<famille>[a-z]+?)--?(?P<numero>\d+)-(?P<nom>.+?)(?:-(?P<modele>noobai|illustrious))?$")

#: Output root override for tests (``None`` = ComfyUI's output directory) —
#: same pattern as ``conditioning_cache.CACHE_ROOT``.
OUTPUT_ROOT: Path | None = None


class SayaFamilySaveError(ValueError):
    """Hard contract failure of SayaFamilySave (the pack never falls back silently)."""


def _slug(text: str) -> str:
    """Folder/widget sanitisation, same recipe as ``registre_ids.slug``."""
    folded = unicodedata.normalize("NFKD", text or "")
    folded = "".join(ch for ch in folded if not unicodedata.combining(ch)).casefold()
    return re.sub(r"[^a-z0-9]+", "-", folded).strip("-")


def _sanitize_stem(stem: str) -> str:
    """Filesystem-safety ONLY: the stem stays canonical (case, ``--``, digits kept)."""
    return re.sub(r"[^A-Za-z0-9._-]+", "-", (stem or "").strip()).strip("-.")


def _resolve_famille(famille: str, famille_custom: str) -> str:
    """The combo value, or the free-text family when ``custom`` (hard error if empty)."""
    if famille == "custom":
        slug = _slug(famille_custom)
        if not slug:
            raise SayaFamilySaveError(
                "SayaFamilySave: famille 'custom' requires a non-empty famille_custom "
                "(it becomes the folder name and the prefix_id).")
        return slug
    slug = _slug(famille)
    if slug not in FAMILLES:
        raise SayaFamilySaveError(
            f"SayaFamilySave: unknown famille {famille!r} (known: {', '.join(FAMILLES)})")
    return slug


def _detect(prompt: Any) -> dict[str, Any]:
    """Literal values in the API graph — the same ones the app's ``inspect`` reads.

    Links (``[node_id, slot]``) are skipped on purpose: inventing a value the
    graph does not carry would be a lie (seed stays ``null`` instead).
    """
    checkpoints: list[str] = []
    loras: list[dict[str, Any]] = []
    seed: int | None = None
    if isinstance(prompt, dict):
        for node in prompt.values():
            if not isinstance(node, dict):
                continue
            inputs = node.get("inputs")
            if not isinstance(inputs, dict):
                continue
            ckpt = inputs.get("ckpt_name")
            if isinstance(ckpt, str) and ckpt and ckpt not in checkpoints:
                checkpoints.append(ckpt)
            class_type = str(node.get("class_type", ""))
            if "lora" in class_type.lower():
                name = inputs.get("lora_name") or inputs.get("name")
                if isinstance(name, str) and name:
                    base = os.path.splitext(os.path.basename(name))[0]
                    entry: dict[str, Any] = {"name": base}
                    for key in ("strength_model", "strength_clip"):
                        value = inputs.get(key)
                        if isinstance(value, (int, float)) and not isinstance(value, bool):
                            entry[key] = value
                    if all(existing.get("name") != base for existing in loras):
                        loras.append(entry)
            if seed is None:
                for key in ("seed", "noise_seed"):
                    value = inputs.get(key)
                    if isinstance(value, int) and not isinstance(value, bool):
                        seed = value
                        break
    return {"checkpoints": checkpoints, "loras": loras, "seed": seed}


def _modele_court(checkpoint: str) -> str:
    """Generez-style model short tag from the checkpoint filename (explicit fallback).

    The comparison compacts the name first ("NOOB AI", "NoobAI", "noob-ai" all
    count as noobai — the Saya checkpoint is literally
    ``MKLAN POISONED NOOB AI FUTA_00001_.safetensors``).
    """
    compact = re.sub(r"[^a-z0-9]+", "", (checkpoint or "").lower())
    if "noobai" in compact:
        return "noobai"
    if "illustrious" in compact:
        return "illustrious"
    return _slug(os.path.splitext(os.path.basename(checkpoint or ""))[0])


def _output_dir() -> str:
    if OUTPUT_ROOT is not None:
        return str(OUTPUT_ROOT)
    import folder_paths  # type: ignore

    return folder_paths.get_output_directory()


class SayaFamilySave:
    """Save PNG(s) under ``saya/<lot>/famille/`` with additive family metadata."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "images": ("IMAGE", {"tooltip": "The images to save."}),
                "famille": (FAMILLES + ("custom",), {
                    "default": "hum",
                    "tooltip": "generez family (registre_ids.py) — decides the subfolder and prefix_id. "
                               "'custom' reveals the free-text famille_custom.",
                }),
            },
            "optional": {
                "famille_custom": ("STRING", {"default": "", "tooltip": "Free-text family used when famille = 'custom' (slugified; required then)."}),
                "perso_id": ("STRING", {"default": "", "tooltip": "COMPLETE character stem, e.g. ch--0001-nelia-thorne (never the bare number: numbering restarts every batch). Canonical — never renamed or renumbered."}),
                "lot": ("STRING", {"default": "", "tooltip": "Optional batch label; becomes a subfolder segment: saya/<lot>/<famille>/."}),
                "filename_prefix": ("STRING", {"default": "", "tooltip": "Native SaveImage override (subfolders and %width%-style vars allowed). Empty = the family layout."}),
            },
            "hidden": {"prompt": "PROMPT", "extra_pnginfo": "EXTRA_PNGINFO"},
        }

    RETURN_TYPES = ("STRING",)
    RETURN_NAMES = ("paths",)
    FUNCTION = "save"
    OUTPUT_NODE = True
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Saya Family Save: PNG in saya/<lot>/<famille>/ with the standard ComfyUI "
        "prompt/workflow chunks PLUS an additive 'saya_family' tEXt chunk and its "
        "<stem>.saya_family.json sidecar (famille, stem, seed, checkpoint, loras)."
    )

    def __init__(self) -> None:
        self.prefix_append = ""
        self.compress_level = 4

    def save(
        self,
        images: torch.Tensor,
        famille: str = "hum",
        famille_custom: str = "",
        perso_id: str = "",
        lot: str = "",
        filename_prefix: str = "",
        prompt: Any = None,
        extra_pnginfo: Any = None,
    ) -> dict[str, Any]:
        family = _resolve_famille(famille, famille_custom)
        lot_slug = _slug(lot)
        stem = _sanitize_stem(perso_id)

        detected = _detect(prompt)
        modele = _modele_court(detected["checkpoints"][0]) if detected["checkpoints"] else ""
        if stem and modele:
            # The trailing noobai/illustrious token is the MODEL tag (generez
            # filename convention), never part of the perso name: the checkpoint
            # that actually ran wins, and the tag is never doubled. Without a
            # detected checkpoint the stem stays untouched (canonical).
            stem = re.sub(r"-(?:noobai|illustrious)$", "", stem)
            stem = f"{stem}-{modele}"

        if filename_prefix:
            prefix = filename_prefix  # native override, verbatim
        else:
            segments = ["saya"] + ([lot_slug] if lot_slug else []) + [family]
            prefix = "/".join([*segments, stem or family])

        parsed = STEM_RE.match(stem)
        version = None
        if isinstance(extra_pnginfo, dict) and isinstance(extra_pnginfo.get("workflow"), dict):
            version = extra_pnginfo["workflow"].get("extra", {}).get("saya_couple_version")
        payload: dict[str, Any] = {
            "famille": family,
            "prefix_id": parsed.group("famille") if parsed else family,
            "perso_id": _sanitize_stem(perso_id) or None,
            "numero": int(parsed.group("numero")) if parsed else None,
            "nom": parsed.group("nom") if parsed else None,
            "lot": lot_slug or None,
            "stem": stem,
            "modele": modele or None,
            "seed": detected["seed"],
            "checkpoints": detected["checkpoints"],
            "loras": detected["loras"],
            "saya_couple_version": version,
            "date": datetime.now().isoformat(timespec="seconds"),
        }

        prefix += self.prefix_append
        import folder_paths  # type: ignore

        full_output_folder, filename, counter, subfolder, _ = folder_paths.get_save_image_path(
            prefix, _output_dir(), int(images.shape[1]), int(images.shape[0]))

        results: list[dict[str, str]] = []
        paths: list[str] = []
        for batch_number, image in enumerate(images):
            array = 255.0 * image.cpu().numpy()
            img = Image.fromarray(np.clip(array, 0, 255).astype(np.uint8))

            metadata = PngImagePlugin.PngInfo()
            if prompt is not None:
                metadata.add_text("prompt", json.dumps(prompt))
            if extra_pnginfo is not None:
                for key, value in extra_pnginfo.items():
                    metadata.add_text(key, json.dumps(value))

            file = f"{filename.replace('%batch_num%', str(batch_number))}_{counter:05}_.png"
            payload["fichier"] = file
            chunk = json.dumps(payload, ensure_ascii=True, sort_keys=True)
            metadata.add_text(FAMILY_CHUNK_KEY, chunk)

            file_path = os.path.join(full_output_folder, file)
            img.save(file_path, pnginfo=metadata, compress_level=self.compress_level)
            sidecar = Path(file_path).with_suffix(FAMILY_SIDECAR_SUFFIX)
            sidecar.write_text(chunk, encoding="utf-8")

            results.append({"filename": file, "subfolder": subfolder, "type": "output"})
            paths.append(file_path)
            counter += 1

        LOGGER.info("[SAYA FAMILY] %s -> %s", payload["stem"], paths[0] if paths else "?")
        return {"ui": {"images": results}, "result": ("\n".join(paths),)}

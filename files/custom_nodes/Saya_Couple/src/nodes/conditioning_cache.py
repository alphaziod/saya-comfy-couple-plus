"""Disk cache for conditionings ("cache-first" encoding) + text encoder release.

A conditioning only depends on three things: the exact TEXT, the CLIP's
FILES, and the LoRAs applied to that CLIP. The cache key contains all three,
so changing a prompt or a LoRA only invalidates the entries it actually
affects.

* directory: ``<output>/conditionings/<family>`` (``hidream`` or ``sdxl``),
  one ``.pt`` file per conditioning;
* an unreadable file counts as absent (re-encoded then rewritten); writes are
  atomic;
* ``release_clip`` moves the encoder off the GPU right after encoding
  (HiDream needs all the VRAM).
"""

from __future__ import annotations

import gc
import hashlib
import logging
import os
import pickle
import tempfile
from pathlib import Path
from typing import Any

import torch

LOGGER = logging.getLogger(__name__)

#: Cache root; ``None`` means ``<output>/conditionings`` (overridable by tests).
CACHE_ROOT: Path | None = None


def _directory(family: str) -> Path:
    if CACHE_ROOT is not None:
        return CACHE_ROOT / family
    import folder_paths  # type: ignore

    return Path(folder_paths.get_output_directory()) / "conditionings" / family


def cache_key(*parts: str) -> str:
    """Fingerprint of the parts (CLIP identity, LoRA, text): any difference changes the key."""
    digest = hashlib.sha256()
    for part in parts:
        digest.update(part.encode("utf-8"))
        digest.update(b"\x00")
    return digest.hexdigest()


def load(family: str, key: str) -> Any | None:
    """Cached conditioning, or None (absent or unreadable)."""
    path = _directory(family) / f"{key}.pt"
    if not path.is_file():
        return None
    try:
        return torch.load(path, map_location="cpu", weights_only=True)
    except (OSError, RuntimeError, EOFError, ValueError, pickle.UnpicklingError) as error:
        LOGGER.warning("[SAYA CACHE] %s unreadable (%s): re-encoding", path.name, str(error).splitlines()[0])
        return None


def save(family: str, key: str, conditioning: Any) -> None:
    """Atomic write (temporary file then replace)."""
    directory = _directory(family)
    directory.mkdir(parents=True, exist_ok=True)
    handle, temporary = tempfile.mkstemp(dir=directory, suffix=".tmp")
    os.close(handle)
    try:
        torch.save(conditioning, temporary)
        os.replace(temporary, directory / f"{key}.pt")
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def clip_lora_fingerprint(clip: Any) -> str:
    """Fingerprint of the LoRA patches actually applied to the CLIP: empty when there are none.

    Based on tensor content, not filenames: it changes as soon as a LoRA or
    its strength changes.
    """
    digest = hashlib.sha256()
    patches = getattr(getattr(clip, "patcher", None), "patches", None) or {}
    for name in sorted(patches, key=str):
        for entry in patches[name]:
            digest.update(str(name).encode("utf-8"))
            strength, payload = entry[0], entry[1]
            digest.update(repr(strength).encode("utf-8"))
            for tensor in _tensors(payload):
                digest.update(str(tuple(tensor.shape)).encode("utf-8"))
                digest.update(tensor.detach().flatten()[:4096].float().cpu().numpy().tobytes())
    return digest.hexdigest() if patches else ""


def _tensors(payload: Any):
    if isinstance(payload, torch.Tensor):
        yield payload
    elif isinstance(payload, (list, tuple)):
        for item in payload:
            yield from _tensors(item)


def release_clip(clip: Any) -> None:
    """Move the text encoder off the GPU and out of the loaded-models list (a memory optimization that must never fail the pass).

    Unload, remove from ``current_loaded_models`` (otherwise ComfyUI keeps
    the reference and the RAM), then garbage-collect and empty the GPU
    cache: the same sequence used to unload the reference-implementation
    encoder.
    """
    try:
        import comfy.model_management as model_management  # type: ignore

        patcher = clip.patcher
        model_management.unload_model_and_clones(patcher)
        for loaded in list(model_management.current_loaded_models):
            if getattr(loaded, "model", None) is patcher:
                loaded.model_unload()
                model_management.current_loaded_models.remove(loaded)
        gc.collect()
        model_management.cleanup_models_gc()
        model_management.soft_empty_cache()
    except Exception as error:
        LOGGER.warning("[SAYA CACHE] could not release the encoder: %s", error)

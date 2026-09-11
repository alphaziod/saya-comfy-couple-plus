"""Persistent local cache for the four *encoded* HiDream text conditionings.

Why this exists
---------------
The HiDream quad CLIP / T5+Llama text encoder (`HiDreamTEModel_`) is ~13.8 GB.
Loading it just to re-encode four prompt strings that have not changed is the
single most expensive avoidable cost in the pipeline.

This service caches the RESULT of the encode (four `CONDITIONING` objects), not
the encoder weights. On a later run with identical prompts and identical CLIP
files the Phase-03 HiDream node reads the four conditionings straight off disk
and never loads the quad CLIP at all.

What is cached
--------------
Four ComfyUI `CONDITIONING` values (Base / Person 1 / Person 2 / Negative), each
a list of ``[context_tensor, metadata_dict]`` entries where the metadata carries
``pooled_output`` and ``conditioning_llama3`` tensors. Everything is moved to CPU
before saving. Files are plain ``torch.save`` payloads under the ComfyUI user
directory; they survive restarts. Old entries are pruned (newest ``_MAX_ENTRIES``
kept).

Cache key
---------
``sha256`` of a canonical JSON of:
  * the four prompt strings (Base / P1 / P2 / Negative), verbatim;
  * a fingerprint of every HiDream CLIP file actually used
    ``(basename, size_bytes, mtime_int)``  -- so swapping / updating a CLIP file
    invalidates the cache;
  * ``encode_cfg`` -- only values that change the *text* encode itself
    (currently just an encoder-recipe tag). Sampler steps / denoise / seed /
    latent size are deliberately NOT in the key: they do not affect the text
    conditioning.
"""

from __future__ import annotations

import hashlib
import json
import os
import time
from threading import RLock
from typing import Any

try:
    import torch
except Exception:  # pragma: no cover - torch always present in ComfyUI
    torch = None  # type: ignore

_LOCK = RLock()
_MAX_ENTRIES = 24
_LOG_PREFIX = "[SAYA HIDREAM CACHE]"


def _log(message: str) -> None:
    print(f"{_LOG_PREFIX} {message}", flush=True)


def _cache_dir() -> str:
    """Persistent, writable directory. Prefer the ComfyUI user dir."""
    base = None
    try:
        import folder_paths

        base = folder_paths.get_user_directory()
    except Exception:
        base = None
    if not base:
        base = os.path.join(os.path.dirname(__file__), "..", "..", "_cache")
    path = os.path.abspath(os.path.join(base, "saya_hidream_conditioning_cache"))
    os.makedirs(path, exist_ok=True)
    return path


def _resolve_clip_path(name: str) -> str | None:
    try:
        import folder_paths

        return folder_paths.get_full_path("text_encoders", name)
    except Exception:
        return None


def fingerprint_clip_files(clip_filenames: list[str]) -> list[list[Any]]:
    """(basename, size, mtime_int) for each CLIP file; robust to missing files."""
    out: list[list[Any]] = []
    for name in clip_filenames:
        name = str(name or "")
        path = _resolve_clip_path(name)
        if path and os.path.isfile(path):
            st = os.stat(path)
            out.append([os.path.basename(path), int(st.st_size), int(st.st_mtime)])
        else:
            out.append([os.path.basename(name), -1, -1])
    return out


def make_key(
    base_text: str,
    person_1_text: str,
    person_2_text: str,
    negative_text: str,
    clip_filenames: list[str],
    encode_cfg: dict[str, Any] | None = None,
) -> str:
    payload = {
        "v": 1,
        "texts": [
            str(base_text or ""),
            str(person_1_text or ""),
            str(person_2_text or ""),
            str(negative_text or ""),
        ],
        "clip": fingerprint_clip_files(list(clip_filenames)),
        "cfg": encode_cfg or {},
    }
    blob = json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(blob.encode("utf-8")).hexdigest()


# --------------------------------------------------------------------------- #
# CPU (de)serialisation
# --------------------------------------------------------------------------- #
def _to_cpu_value(value: Any) -> Any:
    if torch is not None and torch.is_tensor(value):
        return value.detach().to("cpu").contiguous()
    return value


def _conditioning_to_cpu(conditioning: Any) -> list:
    out = []
    for entry in conditioning or []:
        if isinstance(entry, (list, tuple)) and len(entry) == 2 and isinstance(entry[1], dict):
            context, meta = entry
            out.append(
                [
                    _to_cpu_value(context),
                    {k: _to_cpu_value(v) for k, v in meta.items()},
                ]
            )
        else:  # unexpected shape -- keep verbatim rather than lose data
            out.append(entry)
    return out


# --------------------------------------------------------------------------- #
# Public load / save
# --------------------------------------------------------------------------- #
def load(key: str) -> list | None:
    """Return [base, p1, p2, neg] conditionings from disk, or None on miss/error."""
    path = os.path.join(_cache_dir(), f"{key}.pt")
    with _LOCK:
        if not os.path.isfile(path):
            return None
        try:
            data = torch.load(path, map_location="cpu", weights_only=False)  # type: ignore[union-attr]
        except Exception as exc:
            _log(f"WARN could not read {os.path.basename(path)}: {exc}")
            return None
    conds = data.get("conditionings") if isinstance(data, dict) else None
    if not isinstance(conds, list) or len(conds) != 4:
        _log("WARN cache file malformed; ignoring")
        return None
    try:
        os.utime(path, None)  # touch for LRU pruning
    except Exception:
        pass
    return conds


def save(key: str, conditionings: list) -> None:
    """Persist [base, p1, p2, neg] conditionings (moved to CPU) atomically."""
    if len(conditionings) != 4:
        _log(f"WARN refusing to save {len(conditionings)} conditionings (expected 4)")
        return
    directory = _cache_dir()
    path = os.path.join(directory, f"{key}.pt")
    tmp = f"{path}.{os.getpid()}.tmp"
    payload = {
        "version": 1,
        "saved_at": time.time(),
        "conditionings": [_conditioning_to_cpu(c) for c in conditionings],
    }
    with _LOCK:
        try:
            torch.save(payload, tmp)  # type: ignore[union-attr]
            os.replace(tmp, path)
        except Exception as exc:
            _log(f"WARN could not write cache: {exc}")
            if os.path.isfile(tmp):
                try:
                    os.remove(tmp)
                except Exception:
                    pass
            return
        _prune(directory)


def _prune(directory: str) -> None:
    try:
        entries = [
            os.path.join(directory, f) for f in os.listdir(directory) if f.endswith(".pt")
        ]
        if len(entries) <= _MAX_ENTRIES:
            return
        entries.sort(key=lambda p: os.path.getmtime(p))
        for old in entries[: len(entries) - _MAX_ENTRIES]:
            try:
                os.remove(old)
            except Exception:
                pass
    except Exception:
        pass

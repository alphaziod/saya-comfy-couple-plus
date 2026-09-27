"""SayaCoupleImprintResolve — transport / ancestry of the v2 imprint ONLY.

Walks the ``source_file`` chain from a checkpoint (Phase 2 or a Phase 3
input) up to the Phase 1 checkpoint that carries the canonical imprint, then
returns it validated. No encoding, no sampling, no masking: errors are HARD
(never a silent degradation) and always name the offending path.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from ..services.imprint_integrity import (
    IMPRINT_METADATA_KEY,
    SayaImprintIntegrityError,
    verify_or_raise,
)
from .couple_imprint_v2 import (
    SayaCoupleImprintError,
    canonical_imprint_json,
    parse_imprint_json,
)

#: PNG key of the manifest carried by EVERY phase.
MANIFEST_METADATA_KEY = "saya_phase_manifest"

#: Guard against pathological chains.
MAX_CHAIN_LENGTH = 16


class SayaCoupleImprintResolveError(ValueError):
    """Hard failure of the ancestry walk (message = path + reason)."""


@dataclass(frozen=True)
class ResolveResult:
    imprint: dict[str, Any]
    imprint_json: str
    chain: list[str]
    root: str


def _fail(path: Any, reason: str) -> SayaCoupleImprintResolveError:
    return SayaCoupleImprintResolveError(f"imprint resolve: {path!r}: {reason}")


def _json_object(text: Any, path: Any, label: str) -> dict[str, Any]:
    try:
        value = json.loads(text)
    except (TypeError, ValueError) as error:
        raise _fail(path, f"invalid manifest: {label} is unreadable ({error})") from error
    if not isinstance(value, dict):
        raise _fail(path, f"invalid manifest: {label} is not a JSON object")
    return value


def _read_png_chunks(path: Path) -> tuple[Any, Any]:
    try:
        from PIL import Image  # type: ignore
    except ImportError as error:  # pragma: no cover
        raise _fail(path, "PIL unavailable to read the PNG metadata") from error
    try:
        with Image.open(path) as handle:
            info = dict(handle.info or {})
    except (OSError, ValueError) as error:
        raise _fail(path, f"unreadable PNG ({error})") from error
    return info.get(MANIFEST_METADATA_KEY), info.get(IMPRINT_METADATA_KEY)


def _sidecar_manifest(path: Path, embedded: dict[str, Any]) -> dict[str, Any]:
    sidecar = path.with_suffix(".json")
    if not sidecar.is_file():
        raise _fail(path, f"invalid manifest: sidecar {str(sidecar)!r} missing")
    try:
        text = sidecar.read_text(encoding="utf-8")
    except OSError as error:
        raise _fail(path, f"invalid manifest: sidecar unreadable ({error})") from error
    manifest = _json_object(text, path, "sidecar")
    transaction = manifest.get("transaction_uuid")
    if not transaction or not isinstance(transaction, str) or embedded.get("transaction_uuid") != transaction:
        raise _fail(path, "invalid manifest: sidecar/PNG transaction_uuid mismatch")
    return manifest


def resolve_imprint(checkpoint_path: str) -> ResolveResult:
    """Walk ``source_file`` up to the checkpoint carrying a valid v2 imprint."""
    chain: list[str] = []
    visited: set[Path] = set()
    current: Any = checkpoint_path
    while True:
        if not current or not isinstance(current, str):
            if chain:
                raise _fail(chain[-1], "source_file absent (empty or non-text path)")
            raise _fail(current, "empty path")
        path = Path(current)
        if not path.is_file():
            raise _fail(current, "source file missing")
        real = path.resolve()
        if real in visited:
            raise _fail(current, "cycle in source chain: " + " -> ".join(chain + [str(real)]))
        if len(chain) >= MAX_CHAIN_LENGTH:
            raise _fail(current, f"source chain too long (> {MAX_CHAIN_LENGTH})")
        visited.add(real)
        chain.append(str(real))

        manifest_json, imprint_json = _read_png_chunks(path)
        if manifest_json is None:
            raise _fail(current, f"invalid manifest: PNG chunk {MANIFEST_METADATA_KEY!r} missing")
        embedded = _json_object(manifest_json, current, "PNG manifest")
        manifest = _sidecar_manifest(path, embedded)

        if imprint_json is not None:
            try:
                imprint = parse_imprint_json(imprint_json)
            except SayaCoupleImprintError as error:
                raise _fail(current, f"invalid/unsupported imprint ({error})") from error
            try:
                verify_or_raise(manifest, imprint)
            except SayaImprintIntegrityError as error:
                raise _fail(current, f"invalid imprint: integrity ({error})") from error
            return ResolveResult(imprint, canonical_imprint_json(imprint), chain, str(real))

        source = manifest.get("source_file")
        if manifest.get("phase") == 1:
            raise _fail(current, "no imprint found up to chain root: " + " -> ".join(chain))
        if not source or not isinstance(source, str):
            raise _fail(current, "source_file absent (manifest has no valid source_file)")
        if not Path(source).is_file():
            raise _fail(current, f"source_file invalid/missing: {source!r}")
        current = source


class SayaCoupleImprintResolve:
    """Walk the source_file chain up to the canonical v2 imprint (transport only)."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "checkpoint_path": ("STRING", {
                    "default": "", "multiline": False, "forceInput": True,
                    "tooltip": "Path of a VALID checkpoint (Phase 2 or a Phase 3 "
                               "input). The source_file chain is walked up to the "
                               "Phase 1 checkpoint carrying 'saya_couple_imprint'.",
                }),
            },
        }

    RETURN_TYPES = ("SAYA_IMPRINT", "STRING")
    RETURN_NAMES = ("imprint", "imprint_json")
    FUNCTION = "resolve"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Walks the checkpoint's source_file chain up to the Phase 1 checkpoint "
        "that carries the v2 imprint and returns it validated (transport only; "
        "explicit error on a broken chain, a cycle, or an integrity failure)."
    )

    def resolve(self, checkpoint_path: str = "") -> tuple[dict[str, Any], str]:
        result = resolve_imprint(checkpoint_path)
        return result.imprint, canonical_imprint_json(result.imprint)

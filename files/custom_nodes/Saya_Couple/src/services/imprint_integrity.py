"""Saya imprint integrity — an integrity hash/pointer added to the manifest.

Adds a hash/pointer of the imprint to the manifest — DETECTS a divergence
between manifest and PNG, WITHOUT duplicating the content and WITHOUT
becoming a second imprint channel (the manifest still has its own fixed
``build_manifest`` fields; this module adds the field at save time only,
never inside ``build_manifest`` itself).

CONFLICT RULE:

    The Couple imprint is the CANONICAL source for reconstructing the
    Couple. The manifest transports/references the artifact and verifies
    integrity/provenance, and may confirm expected identities. On a
    contradiction between manifest and imprint over a Couple contract
    value: HARD ERROR — neither one wins, no silent fallback, no automatic
    correction. The hash/pointer DETECTS the divergence; it is NOT the
    precedence rule.

This module is DETECTION only: ``verify_imprint_integrity`` returns a
verdict; the caller MUST turn a divergence into a HARD ERROR (the
consuming node documents this obligation in its docstring).
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
from typing import Any

from ..nodes.couple_imprint_v2 import canonical_imprint_json

#: PNG key carrying the imprint.
IMPRINT_METADATA_KEY = "saya_couple_imprint"

#: Field added to the manifest (the manifest's own fixed fields stay untouched).
MANIFEST_FIELD = "imprint_integrity"

ALGORITHM = "sha256+canonical-json"


def imprint_digest(imprint: dict[str, Any] | str) -> str:
    """sha256 fingerprint of the CANONICAL JSON (key order does not matter).

    Accepts a dict (canonicalized here) or a JSON STRING (re-canonicalized
    after parsing — two serializations of the same content give the same
    digest).
    """
    if isinstance(imprint, str):
        try:
            data = json.loads(imprint)
        except json.JSONDecodeError as error:
            raise ValueError(f"imprint_digest: invalid JSON: {error}") from None
    elif isinstance(imprint, dict):
        data = imprint
    else:
        raise ValueError(
            f"imprint_digest: expected a dict or JSON str, got {type(imprint).__name__}"
        )
    canonical = canonical_imprint_json(data)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def build_integrity_pointer(imprint: dict[str, Any] | str) -> dict[str, str]:
    """Canonical integrity pointer (deterministic, without the imprint content)."""
    return {
        "algorithm": ALGORITHM,
        "digest": imprint_digest(imprint),
        "png_key": IMPRINT_METADATA_KEY,
        "rule": (
            "detection-only — a manifest/imprint divergence is a HARD ERROR; "
            "the imprint stays the canonical source"
        ),
    }


def attach_integrity_pointer(manifest: dict[str, Any], imprint: dict[str, Any] | str) -> dict[str, Any]:
    """Return a COPY of the manifest plus the integrity pointer.

    The original manifest is never mutated; ``build_manifest``'s own fields
    are not touched — the field only exists on manifests that explicitly
    carry the pointer.
    """
    if not isinstance(manifest, dict):
        raise ValueError(f"manifest: expected a dict, got {type(manifest).__name__}")
    if MANIFEST_FIELD in manifest:
        raise ValueError(
            f"manifest.{MANIFEST_FIELD}: pointer already present — refusing to "
            "overwrite (detection-only, no silent correction)"
        )
    out = dict(manifest)
    out[MANIFEST_FIELD] = build_integrity_pointer(imprint)
    return out


class SayaImprintIntegrityError(ValueError):
    """Imprint integrity divergence/absence — the contractual HARD ERROR.

    A manifest/imprint contradiction is a HARD ERROR: neither one wins, no
    silent fallback, no automatic correction.
    """


def verify_or_raise(manifest: dict[str, Any], imprint: dict[str, Any] | str) -> str:
    """``verify_imprint_integrity`` that ENFORCES the contract: ok=False -> raise.

    Use this API from any consumer that cannot proceed without a verified
    imprint (reconstruction, sampling) — ``verify`` alone only DETECTS.
    """
    ok, detail = verify_imprint_integrity(manifest, imprint)
    if not ok:
        raise SayaImprintIntegrityError(detail)
    return detail


def verify_imprint_integrity(
    manifest: dict[str, Any],
    imprint: dict[str, Any] | str,
) -> tuple[bool, str]:
    """Compare the manifest's pointer against the given imprint (e.g. from the PNG).

    Returns ``(ok, detail)``. ``ok=False`` means divergence OR a missing
    pointer — in BOTH cases the caller MUST raise a HARD ERROR (never pass
    silently; a missing pointer means "cannot verify").
    """
    if not isinstance(manifest, dict):
        return False, f"manifest: expected a dict, got {type(manifest).__name__}"
    pointer = manifest.get(MANIFEST_FIELD)
    if pointer is None:
        return False, (
            f"imprint integrity: pointer missing from manifest "
            f"({MANIFEST_FIELD}) — cannot verify (HARD ERROR expected)"
        )
    if not isinstance(pointer, dict) or "digest" not in pointer:
        return False, f"imprint integrity: malformed pointer {pointer!r}"
    expected = build_integrity_pointer(imprint)
    if pointer.get("digest") != expected["digest"]:
        return False, (
            "imprint integrity: DIVERGENCE between manifest and imprint — "
            f"manifest={pointer.get('digest')!r} vs imprint={expected['digest']!r} "
            "(HARD ERROR expected; neither one wins)"
        )
    return True, f"imprint integrity: OK ({expected['digest'][:16]}...)"


# ---------------------------------------------------------------------------
# pack_discriminant (provenance block) — deterministic
# ---------------------------------------------------------------------------

def pack_discriminant(pack_root: str | os.PathLike[str]) -> str:
    """Deterministic sha256 of registry.py + the pack's src/ tree.

    Determinism contract (the same code state must always give the same
    hash):
    * files = ``registry.py`` + every file under ``src/``;
    * excluded: ``__pycache__``, ``*.pyc``, dotfiles;
    * paths RELATIVE to the root, '/' separators, sorted lexicographically;
    * stream = for each file: ``len(path):path`` then ``len(bytes):bytes``
      (length prefixes remove concatenation ambiguity);
    * RAW content (no line-ending normalization), symlinks followed once
      through their resolved content.
    """
    root = Path(pack_root)
    files: list[Path] = []
    registry = root / "registry.py"
    if registry.is_file():
        files.append(registry)
    src = root / "src"
    if src.is_dir():
        for current, dirnames, filenames in os.walk(src):
            dirnames[:] = sorted(d for d in dirnames if d != "__pycache__")
            for filename in filenames:
                if filename.endswith((".pyc", ".pyo")) or filename.startswith("."):
                    continue
                files.append(Path(current) / filename)
    if not files:
        raise ValueError(f"pack_discriminant: no files under {root} (registry.py/src missing)")
    files.sort(key=lambda p: p.relative_to(root).as_posix())
    digest = hashlib.sha256()
    for path in files:
        rel = path.relative_to(root).as_posix()
        data = path.read_bytes()
        digest.update(f"{len(rel)}:{rel}".encode("utf-8"))
        digest.update(f"{len(data)}:".encode("ascii"))
        digest.update(data)
    return f"sha256:{digest.hexdigest()}"


def read_png_info(path):
    """Text chunks of a PNG (``saya_phase_manifest``, ``saya_couple_imprint``...) as a plain dict. The one PNG
    reader of the pack (P-E, 2026-10-05); callers map OSError / ValueError to their own error types."""
    from PIL import Image

    with Image.open(path) as handle:
        return dict(handle.info or {})

"""Small conditioning helpers shared by the couple node surfaces.

Both the Forge couple node (`forge/node.py`) and the legacy couple node
(`src/nodes/couple_conditioning.py`) need the exact same two operations:

* ``copy_conditioning`` — a shallow copy that clones the per-entry metadata dict
  (so a caller can add a ``mask`` key) while keeping the context tensor shared by
  reference. Conditioning tensors are read-only by contract, so this never
  duplicates tensor storage.
* ``describe_conditioning_error`` — a structural validation that returns a
  human-readable reason string, or ``None`` when the value is a well-formed
  ComfyUI CONDITIONING (a list of ``[rank-3 tensor, dict]`` pairs). An empty list
  / ``None`` is considered "not supplied", not an error.

They were byte-identical copies in both node files; this is the single source.
"""

from __future__ import annotations

from typing import Any

try:  # torch is always present inside ComfyUI; the guard keeps imports cheap in tests
    import torch
except Exception:  # pragma: no cover
    torch = None  # type: ignore[assignment]


def copy_conditioning(conditioning: Any) -> list:
    """Return a shallow copy whose metadata dicts are safe to mutate."""
    if not conditioning:
        return []
    return [[entry[0], dict(entry[1])] for entry in conditioning]


def describe_conditioning_error(name: str, conditioning: Any) -> str | None:
    """Return why ``conditioning`` is not a valid CONDITIONING, or ``None``."""
    if conditioning is None or conditioning == []:
        return None
    if not isinstance(conditioning, list):
        return f"{name} is not a CONDITIONING list"
    for index, entry in enumerate(conditioning):
        if not isinstance(entry, (list, tuple)) or len(entry) != 2:
            return f"{name}[{index}] is not a [tensor, metadata] entry"
        if torch is not None and (
            not isinstance(entry[0], torch.Tensor) or entry[0].ndim != 3
        ):
            return f"{name}[{index}] context is not a rank-3 tensor"
        if not isinstance(entry[1], dict):
            return f"{name}[{index}] metadata is not a dictionary"
    return None

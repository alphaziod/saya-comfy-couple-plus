"""Invisible in-process handoff for Saya Couple automatic image phases.

The automatic image workflow runs its phases as separate ComfyUI queue
submissions inside the same server process.  Phase 1 writes the latest
Couple payload here; Phase 3 and Phase 6 read it later.  There are no
workflow transport nodes and no CLIP/HiDream model objects are retained.

This service intentionally models one active automatic Saya image sequence,
matching the existing phase controller.  A new MASTER evaluation replaces
the previous payload, and the final phase clears it.
"""

from __future__ import annotations

from threading import RLock
from typing import Any

_LOCK = RLock()
_STATE: dict[str, Any] = {}


def _copy_conditioning(value: Any) -> Any:
    if not value:
        return []
    if not isinstance(value, list):
        return value
    copied = []
    for entry in value:
        if isinstance(entry, (list, tuple)) and len(entry) == 2 and isinstance(entry[1], dict):
            copied.append([entry[0], dict(entry[1])])
        else:
            copied.append(entry)
    return copied


def store_master_payload(
    *,
    hidream_main: Any = None,
    hidream_person_1: Any = None,
    hidream_person_2: Any = None,
    hidream_negative: Any = None,
    naturalize_model: Any = None,
    naturalize_positive: Any = None,
    naturalize_negative: Any = None,
    naturalize_ready: bool = False,
) -> None:
    """Replace the payload for the currently active automatic image sequence.

    Only what a later phase actually reads is kept: the Naturalize
    model/positive/negative for Phase 6, and (for a workflow that still encodes
    HiDream in Phase 1) the four native HiDream conditionings for Phase 3.
    """
    hidream_values = (hidream_main, hidream_person_1, hidream_person_2, hidream_negative)
    hidream_ready = all(value is not None and value != [] for value in hidream_values)
    with _LOCK:
        _STATE.clear()
        if hidream_ready:
            _STATE["hidream"] = (
                _copy_conditioning(hidream_main),
                _copy_conditioning(hidream_person_1),
                _copy_conditioning(hidream_person_2),
                _copy_conditioning(hidream_negative),
            )
        if naturalize_ready:
            _STATE["naturalize"] = (
                naturalize_model,
                _copy_conditioning(naturalize_positive),
                _copy_conditioning(naturalize_negative),
            )


def get_hidream_payload() -> tuple[Any, Any, Any, Any]:
    """Return the four pre-encoded native HiDream conditionings, if any.

    Since the HiDream lifecycle rework the encode is DEFERRED to Phase 03
    (SayaComfyCoupleHiDreamCopy encodes cache-first there), so Phase 01 usually
    stores nothing here. Return four empty CONDITIONING lists in that case and
    let the Phase-03 node materialise them; only return a real payload when a
    workflow still encodes HiDream in Phase 01.
    """
    with _LOCK:
        value = _STATE.get("hidream")
        if not value:
            return ([], [], [], [])
        return value


def get_naturalize_payload() -> tuple[Any, Any, Any]:
    """Return the MASTER Naturalize model/positive/negative outputs."""
    with _LOCK:
        value = _STATE.get("naturalize")
        if not value:
            raise RuntimeError(
                "Saya Naturalize routing: no cached MASTER Naturalize payload. "
                "Run the normal Saya Phase 1 first and keep the server running."
            )
        return value


def clear_master_payload() -> None:
    """Drop all cross-phase references after the automatic sequence finishes."""
    with _LOCK:
        _STATE.clear()


def debug_state_keys() -> tuple[str, ...]:
    """Small test helper; returns keys only and never exposes payload contents."""
    with _LOCK:
        return tuple(sorted(_STATE))

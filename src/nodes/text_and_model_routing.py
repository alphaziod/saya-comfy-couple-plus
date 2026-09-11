"""Text encoding and hires model-routing ComfyUI nodes."""

from __future__ import annotations

import time
from threading import RLock
from typing import Any, Self

from ..services.models import build_model_choice_list, load_vae_or_fallback

_LOG_PREFIX = "[SAYA ENCODE]"


def _log(message: str) -> None:
    """One short line per encode boundary — kept intentionally minimal."""
    print(f"{_LOG_PREFIX} {message}", flush=True)


def _encode_with_clip(clip: Any, text: str) -> Any:
    """Tokenise + scheduled-encode a prompt with whichever CLIP family is given."""
    tokens = clip.tokenize(text)
    return clip.encode_from_tokens_scheduled(tokens)


# --------------------------------------------------------------------------- #
# HiDream text-encode batching
# --------------------------------------------------------------------------- #
# WHY THIS EXISTS
# --------------
# Each SayaDualCLIPTextEncode used to run, inside ONE node execution:
#     SDXL(text)  ->  HiDream(text)  ->  SDXL(naturalize_text)
# The HiDream quad-CLIP text encoder (`HiDreamTEModel_`, ~13.8 GB) and the SDXL
# CLIP (`SDXLClipModel`, ~1.6 GB) do not co-reside in a 16 GB card, so every
# HiDream call evicted the SDXL CLIP and every following SDXL call evicted the
# HiDream TE. With the four prompt nodes (Base / Person 1 / Person 2 / Negative)
# executing one after another this produced ~7 full model transitions on the
# very first pass (measured wall-clock: ~50 s -> ~102 s).
#
# THE FIX (no graph change, no new nodes, identical 6-in / 3-out interface)
# ----------------------------------------------------------------------
# SDXL and Naturalize still encode eagerly (they share the SDXL CLIP, which
# stays resident, so they are cheap and cause no transition). The HiDream encode
# is DEFERRED: `encode()` registers `(hidream_clip, text)` and returns an inert
# request token. The Couple MASTER (`SayaComfyCoupleForge.run()`) -- which
# ComfyUI cannot execute until all four prompt nodes have produced their
# outputs -- calls `resolve_hidream_conditioning()` on its four HiDream inputs.
# The first of those calls flushes the WHOLE pending batch in one shot: the quad
# CLIP is loaded once, all four prompts are encoded back to back, the quad CLIP
# is released once. Net first-pass model swaps: 2 (SDXL CLIP -> HiDream TE ->
# SDXL UNet) instead of ~7.
#
# WHY A PLAIN TOKEN, NOT A `list` SUBCLASS
# --------------------------------------
# A first attempt returned a lazy `list` subclass that encoded itself on first
# read. It failed: ComfyUI 0.34's RAM-pressure output cache
# (`comfy_execution/caching.py: RAMPressureCache.ram_release ->
# scan_list_for_ram_usage`) walks every cached node output and, for anything
# that `isinstance(x, (list, tuple, Mapping))`, recurses with `for output in
# outputs` -- which triggered the lazy immediately after each node, one prompt
# at a time (log: `HIDREAM START (1 prompt(s))` x4, ping-pong intact).
# `_HiDreamEncodeRequest` is therefore NOT a list: the scanner falls through to
# its `_comfy_cache_tensors()` hook (returns `[]`) and never iterates it. It is
# only ever materialised by the explicit `resolve_hidream_conditioning()` call
# in the MASTER, so the batch always contains all four prompts.
#
# ASSUMPTIONS (documented on purpose)
# ----------------------------------
#  * ComfyUI runs one prompt graph at a time in-process; the four prompt nodes
#    and the MASTER that consumes them are in the same run, MASTER last.
#  * The consumer of slot 1 (`hidream_conditioning`) is the Couple MASTER, which
#    calls `resolve_hidream_conditioning()` on each. A token that is never
#    resolved encodes nothing (correct -- unused output); stale tokens older
#    than `_STALE_SECONDS` are dropped on the next registration so the pending
#    list cannot grow unbounded. As a last-resort safety net the token also
#    resolves itself (single encode) if some other code iterates it directly.

_HIDREAM_LOCK = RLock()
_HIDREAM_PENDING: list["_HiDreamEncodeRequest"] = []
_STALE_SECONDS = 600.0
_HIDREAM_DEFERRED_NOTED = False


def _note_hidream_deferred() -> None:
    """One line per process: HiDream is not touched until Phase 03."""
    global _HIDREAM_DEFERRED_NOTED
    if not _HIDREAM_DEFERRED_NOTED:
        _HIDREAM_DEFERRED_NOTED = True
        print("[SAYA HIDREAM] DEFERRED UNTIL PHASE 03 (no quad CLIP load in Phase 01)", flush=True)


def _flush_hidream_batch() -> None:
    """Encode every still-pending HiDream prompt in a single quad-CLIP window."""
    with _HIDREAM_LOCK:
        pending = [item for item in _HIDREAM_PENDING if not item._resolved]
        _HIDREAM_PENDING.clear()
        if not pending:
            return
        _log(f"HIDREAM START ({len(pending)} prompt(s), one quad-CLIP load)")
        started = time.time()
        for item in pending:
            try:
                item._fill(_encode_with_clip(item._clip, item._text))
            except Exception as exc:  # never silently emit an empty conditioning
                item._fill_error(exc)
        _log(f"HIDREAM END ({time.time() - started:.1f}s)")
        for item in pending:
            if item._error is not None:
                raise item._error


class _HiDreamEncodeRequest:
    """Inert placeholder for one deferred HiDream prompt encode.

    Deliberately NOT a ``list``/``tuple``/``Mapping``/``Tensor`` so ComfyUI's
    output-cache RAM scanner does not walk it (it hits ``_comfy_cache_tensors``
    instead). Materialised by ``resolve_hidream_conditioning`` (normal path) or,
    as a fallback only, by direct iteration.
    """

    __slots__ = ("_clip", "_text", "_stamp", "_resolved", "_result", "_error")

    def __init__(self, clip: Any, text: str) -> None:
        self._clip = clip
        self._text = text
        self._stamp = time.monotonic()
        self._resolved = False
        self._result: list | None = None
        self._error: Exception | None = None

    # -- population (called by the batch flush) -------------------------- #
    def _fill(self, conditioning: Any) -> None:
        self._result = list(conditioning or [])
        self._resolved = True

    def _fill_error(self, exc: Exception) -> None:
        self._error = exc
        self._resolved = True

    # -- materialisation ------------------------------------------------- #
    def resolved_value(self) -> list:
        """Flush the whole pending batch if needed, then return this entry."""
        if not self._resolved:
            _flush_hidream_batch()
        if self._error is not None:
            raise self._error
        return self._result if self._result is not None else []

    # ComfyUI 0.34 RAM-pressure cache hook: "this object owns no cached tensors",
    # which stops `scan_list_for_ram_usage` from iterating it.
    def _comfy_cache_tensors(self) -> list:
        return []

    # Safety net: if some other code path iterates / truth-tests the token
    # directly, degrade gracefully to a (still batched) resolve instead of
    # crashing. Not the normal path.
    def __iter__(self):
        return iter(self.resolved_value())

    def __len__(self) -> int:
        return len(self.resolved_value())

    def __bool__(self) -> bool:
        return len(self.resolved_value()) > 0

    def __eq__(self, other) -> bool:
        if isinstance(other, _HiDreamEncodeRequest):
            return other is self
        return self.resolved_value() == other

    def __ne__(self, other) -> bool:
        return not self.__eq__(other)

    __hash__ = None

    def __repr__(self) -> str:  # never resolve just to print a debug line
        return f"<SayaHiDreamEncodeRequest resolved={self._resolved}>"


def resolve_hidream_conditioning(value: Any) -> Any:
    """Return a real CONDITIONING for a deferred HiDream request.

    Called by the Couple MASTER on each of its four `hidream_*` inputs. The first
    call flushes every pending request in one quad-CLIP residency window; the
    rest read their already-filled result. Passes non-request values through
    unchanged (``[]`` or an already-encoded conditioning).
    """
    if isinstance(value, _HiDreamEncodeRequest):
        return value.resolved_value()
    return value


class DualClipTextEncoderNode:
    """Encode prompts for SDXL, HiDream, and Naturalize independently.

    Output slot 0 is the historical SDXL/primary conditioning and is kept
    append-only for workflow compatibility.

    Each branch encodes independently, never converting one family into
    another:
      * ``text`` + ``clip`` -> slot 0 (SDXL conditioning), encoded eagerly;
      * ``naturalize_text`` + ``naturalize_clip`` -> slot 2: Naturalize
        conditioning, encoded eagerly (same SDXL CLIP, no model transition),
        empty (``[]``) when that CLIP is not connected;
      * ``text`` + ``hidream_clip`` -> slot 1: the same original text encoded
        with a native HiDream QuadrupleCLIPLoader. DEFERRED -- slot 1 returns an
        inert `_HiDreamEncodeRequest`; the Couple MASTER calls
        `resolve_hidream_conditioning()` which encodes all four prompt nodes'
        HiDream branches in one quad-CLIP load (see module docstring above).
        Empty (``[]``) when that CLIP is not connected.

    ``send_data=False`` disables every encode and returns three empty lists.
    """

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        """Return the ComfyUI input schema exposed by this node."""
        return {
            "required": {
                "clip": ("CLIP",),
                "text": ("STRING", {"multiline": True, "default": ""}),
                "send_data": ("BOOLEAN", {"default": True}),
            },
            "optional": {
                "hidream_clip": ("CLIP",),
                "naturalize_clip": ("CLIP",),
                "naturalize_text": ("STRING", {"multiline": True, "default": ""}),
            },
        }

    RETURN_TYPES = ("CONDITIONING", "CONDITIONING", "CONDITIONING")
    RETURN_NAMES = ("conditioning", "hidream_conditioning", "naturalize_conditioning")
    FUNCTION = "encode"
    CATEGORY = "saya/rescue"

    @staticmethod
    def _encode_with_clip(clip: Any, text: str) -> Any:
        return _encode_with_clip(clip, text)

    def encode(
        self: Self,
        clip: Any,
        text: str,
        send_data: bool = True,
        hidream_clip: Any | None = None,
        naturalize_clip: Any | None = None,
        naturalize_text: str = "",
    ) -> Any:
        """Encode SDXL + Naturalize now; defer HiDream to the shared batch."""
        if not send_data:
            return ([], [], [])

        _log("SDXL START")
        started = time.time()
        conditioning = _encode_with_clip(clip, text)
        _log(f"SDXL END ({time.time() - started:.1f}s)")

        if naturalize_clip is None:
            naturalize_conditioning = []
        else:
            _log("NATURALIZE START")
            started = time.time()
            naturalize_conditioning = _encode_with_clip(naturalize_clip, naturalize_text)
            _log(f"NATURALIZE END ({time.time() - started:.1f}s)")

        if hidream_clip is None:
            # Normal path for the phased HiDream workflow: the quad CLIP is NOT
            # wired here, so nothing HiDream is loaded or encoded in Phase 01.
            # Phase 03 (SayaComfyCoupleHiDreamCopy) encodes the 4 prompts on
            # demand, cache-first. Slot 1 stays an empty CONDITIONING.
            hidream_conditioning: Any = []
            _note_hidream_deferred()
        else:
            hidream_conditioning = _HiDreamEncodeRequest(hidream_clip, text)
            with _HIDREAM_LOCK:
                now = time.monotonic()
                _HIDREAM_PENDING[:] = [
                    item
                    for item in _HIDREAM_PENDING
                    if not item._resolved and (now - item._stamp) < _STALE_SECONDS
                ]
                _HIDREAM_PENDING.append(hidream_conditioning)
            _log("HIDREAM DEFERRED (batched, resolved by the Couple MASTER)")

        return (conditioning, hidream_conditioning, naturalize_conditioning)


class HiresModelRouterNode:
    """Route shared conditioning, models, and VAEs through hires stages."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        """Return the ComfyUI input schema exposed by this node."""
        vaes = build_model_choice_list("vae")
        source = ["main", "dual_sampling", "support1", "usdu1", "usdu2"]
        vae_choice = [
            "none",
            "main",
            "dual_sampling",
            "support1",
            "usdu1",
            "usdu2",
            "custom_vae_1",
            "custom_vae_2",
            "custom_vae_3",
        ]
        req = {
            "positive": ("CONDITIONING",),
            "negative": ("CONDITIONING",),
            "main_model": ("MODEL",),
            "main_vae": ("VAE",),
            "dual_sampling_model": ("MODEL",),
            "dual_sampling_vae": ("VAE",),
            "support1_model": ("MODEL",),
            "support1_vae": ("VAE",),
            "usdu1_model": ("MODEL",),
            "usdu1_vae": ("VAE",),
            "usdu2_model": ("MODEL",),
            "usdu2_vae": ("VAE",),
            "━━ CUSTOM VAE ━━": ("STRING", {"default": "━━ CUSTOM VAE ━━"}),
            "custom_vae_1": (vaes,),
            "custom_vae_2": (vaes,),
            "custom_vae_3": (vaes,),
        }
        for name in ["base", "mid", "final", "last"]:
            req[f"━━ {name.upper()} HIRES ━━"] = (
                "STRING",
                {"default": f"━━ {name.upper()} HIRES ━━"},
            )
            req[f"{name}_source"] = (source, {"default": "main"})
            req[f"{name}_vae"] = (vae_choice, {"default": "none"})
        return {"required": req}

    RETURN_TYPES = (
        "CONDITIONING",
        "CONDITIONING",
        "MODEL",
        "VAE",
        "MODEL",
        "VAE",
        "MODEL",
        "VAE",
        "MODEL",
        "VAE",
    )
    RETURN_NAMES = (
        "positive",
        "negative",
        "base_model",
        "base_vae",
        "mid_model",
        "mid_vae",
        "final_model",
        "final_vae",
        "last_model",
        "last_vae",
    )
    FUNCTION = "route"
    CATEGORY = "saya/rescue"

    def route(self: Self, **kw: Any) -> Any:
        """Route node inputs to the selected output path."""
        models = {
            "main": kw["main_model"],
            "dual_sampling": kw["dual_sampling_model"],
            "support1": kw["support1_model"],
            "usdu1": kw["usdu1_model"],
            "usdu2": kw["usdu2_model"],
        }
        vaes = {
            "main": kw["main_vae"],
            "dual_sampling": kw["dual_sampling_vae"],
            "support1": kw["support1_vae"],
            "usdu1": kw["usdu1_vae"],
            "usdu2": kw["usdu2_vae"],
        }
        vaes["custom_vae_1"] = load_vae_or_fallback(kw.get("custom_vae_1"), kw["main_vae"])
        vaes["custom_vae_2"] = load_vae_or_fallback(kw.get("custom_vae_2"), kw["main_vae"])
        vaes["custom_vae_3"] = load_vae_or_fallback(kw.get("custom_vae_3"), kw["main_vae"])
        out = [kw["positive"], kw["negative"]]
        for name in ["base", "mid", "final", "last"]:
            src = kw.get(f"{name}_source", "main")
            vc = kw.get(f"{name}_vae", "none")
            out.append(models.get(src, kw["main_model"]))
            out.append(vaes.get(src if vc == "none" else vc, kw["main_vae"]))
        return tuple(out)

"""HiDream regional conditioning — Architecture E (native, no MODEL wrapper).

Nodes registered from this module:
    SayaComfyCoupleHiDream        (MASTER: geometry widgets + config output)
    SayaComfyCoupleHiDreamCopy    (COPY: consumes a config; used in Phase 03)

================================================================================
WHAT THIS NODE DOES  (and, just as important, what it does NOT do)
================================================================================
It takes native HiDream CONDITIONING (each prompt encoded with the HiDream quad
CLIP, so it carries T5 context + pooled_output + conditioning_llama3) and produces
*more* native HiDream CONDITIONING entries, one per region, each with a spatial
`mask`. You wire the result into `KSampler.positive`. The stock ComfyUI sampler
(`comfy/samplers.py:_calc_cond_batch`) then runs one HiDream forward per region
and composites the predictions by the masks — regional routing done entirely
through data the sampler already understands (scheduling, EasyCache, memory
batching, detailers), with no attention patch.

It does NOT:
  * wrap, clone or patch the MODEL (there is no MODEL socket at all);
  * install any WrappersMP.DIFFUSION_MODEL wrapper;
  * keep a "regional bank" or any large tensor alive past the node call;
  * monkey-patch ComfyUI or reuse the Forge attn2 engine (a no-op on HiDream's
    joint attention).

================================================================================
PHASE-03 DEFERRED, CACHE-FIRST ENCODE  (SayaComfyCoupleHiDreamCopy only)
================================================================================
In the phased image workflow, `SayaDualCLIPTextEncode` does NOT encode HiDream in
Phase 01 (the ~13.8 GB quad CLIP never loads there). So `run_copy` may receive
four EMPTY CONDITIONING inputs. When that happens it reads the four original
prompt strings from the Phase-01 bundle (`prompt_bundle_json`) and calls
`materialize_hidream_conditionings`:
  * cache HIT  -> read the four conditionings from
                 <ComfyUI>/user/saya_hidream_conditioning_cache/ ; no CLIP load;
  * cache MISS -> load the quad CLIP once (wired `clip`, else self-load), encode
                 the four prompts back to back, persist them, then unload the TE
                 (only when this node self-loaded it).
A workflow that still provides pre-encoded conditionings skips all of this
(`_has_entries` on the four inputs) and behaves exactly as before.

================================================================================
CONDITIONING CONTRACT
================================================================================
A ComfyUI CONDITIONING is a list of entries `[context_tensor, metadata_dict]`.
There can be MORE THAN ONE entry (prompt scheduling, ConditioningCombine, hooks).
This node never assumes `len(conditioning) == 1`: for every source entry of a
person it emits one regional entry, carrying that entry's own metadata forward.

For each regional entry we PRESERVE every metadata key the sampler understands
(`start_percent`/`end_percent`, `hooks`, `control`, `strength`, `area`, `gligen`,
`pooled_output`, `conditioning_llama3`, ...) and only ADD / OVERRIDE:
  * `pooled_output`, `conditioning_llama3`  — replaced when the context mode
    concatenates or re-encodes MAIN with the person (person_only leaves them);
  * `mask`             — the region ownership mask, LATENT resolution [B,H,W];
  * `mask_strength`    — 1.0 (the geometry already encodes softness);
  * `set_area_to_bounds` — False (we want the whole frame evaluated and weighted
    by the mask, not cropped to its bounding box).

Inputs are never mutated: every copy is `[entry[0], dict(entry[1])]`.

================================================================================
COORDINATE SPACES
================================================================================
* regional masks on CONDITIONING  -> LATENT space  (H_px/8, W_px/8)
* public `MASK` outputs + detailer masks -> PIXEL space (unchanged from Forge)
* HiDream patch grid (H_lat/2)     -> never used here
See regional_attention/hidream_masks.py for the full note.

================================================================================
PROMPT / CONTEXT MODES
================================================================================
person_only
    Each region carries ONLY the person's own T5/pooled/llama3. The scene prompt
    is not represented inside the regions. Turn on `add_global_main_entry` (or
    `include_main_contact`) if you still want MAIN to influence the frame.

main_plus_person_concat
    Token-concatenate the independently-encoded MAIN and person sequences
    (T5 on dim 1, llama3 on dim 2, pooled = MAIN). Shape-safe for any token
    lengths (the DiT indexes per layer then projects on the last dim). This is
    ZCODE's original strategy. Cheap (no extra encode) but the encoders never
    contextualised the two prompts together — quality is a RUNTIME question.
    Requires MAIN to be a single entry.

main_plus_person_reencoded
    Genuinely re-encode a merged prompt "<main_text>. <person_text>" through the
    HiDream quad CLIP. This needs the raw text and a CLIP, so the node exposes
    `clip` + `*_text` sockets; it is a hard error to select this mode without
    them (we never fake a re-encode with an embedding concat). Closest to the
    training distribution; costs two extra encodes.
"""

from __future__ import annotations

import gc
import json
import re
from typing import Any

import torch

import comfy.utils

from .regional_attention.hidream_masks import pixel_masks, routing_masks
from ..services import hidream_cache
from ..services.conditioning import copy_conditioning as _shared_copy_conditioning

COUPLE_CONFIG_TYPE = "SAYA_COUPLE_CONFIG"

# Default HiDream quad-CLIP file names (match the workflow's QuadrupleCLIPLoader).
# Used only when the Phase-03 node has to encode on a cache MISS and no CLIP is
# wired into its `clip` socket.
_HIDREAM_TE_DEFAULTS = (
    "clip_l_hidream.safetensors",
    "clip_g_hidream.safetensors",
    "t5xxl_fp8_e4m3fn.safetensors",
    "llama_3.1_8b_instruct_fp8_scaled.safetensors",
)
# Bumped only if the raw per-prompt encode itself changes (not regional logic).
_ENCODE_RECIPE_TAG = "quad_clip_v1"
_HIDREAM_LOG = "[SAYA HIDREAM]"


def inject_hidream_trigger(text: str, trigger: str) -> str:
    """Prepend a configurable trigger, avoiding simple token/phrase duplicates."""
    trigger = str(trigger or "").strip(" ,\n\r\t")
    if not trigger or re.match(r"^\s*" + re.escape(trigger) + r"(?:\s*,|\s*$)", text):
        return text
    # Move a standalone comma-delimited occurrence to the top, if present.
    parts = text.split(",")
    if any(part.strip() == trigger for part in parts):
        text = ",".join(part for part in parts if part.strip() != trigger).strip(" ,\n\r\t")
    return f"{trigger}, {text}" if text else trigger


def _prompts_from_bundle(bundle_json: str) -> tuple[str, str, str, str]:
    """(base, p1, p2, negative) from the Phase-01 prompt bundle JSON string."""
    try:
        parsed = json.loads(str(bundle_json or ""))
        if isinstance(parsed, dict):
            return (
                str(parsed.get("base_prompt", "")),
                str(parsed.get("person_1_prompt", "")),
                str(parsed.get("person_2_prompt", "")),
                str(parsed.get("negative_prompt", "")),
            )
    except (json.JSONDecodeError, TypeError):
        pass
    return "", "", "", ""


def _has_entries(conditioning: Any) -> bool:
    """True only for a real, non-empty CONDITIONING list."""
    return (
        isinstance(conditioning, list)
        and len(conditioning) > 0
        and isinstance(conditioning[0], (list, tuple))
        and len(conditioning[0]) == 2
    )


def _load_quad_clip(names: tuple[str, str, str, str]) -> Any:
    """Load the HiDream quad CLIP exactly like the stock QuadrupleCLIPLoader."""
    import comfy.sd
    import folder_paths

    paths = [folder_paths.get_full_path_or_raise("text_encoders", n) for n in names]
    return comfy.sd.load_clip(
        ckpt_paths=paths,
        embedding_directory=folder_paths.get_folder_paths("embeddings"),
    )


def _unload_self_loaded_clip(clip_obj: Any) -> None:
    """Drop a CLIP we loaded ourselves so its ~13.8 GB TE does not linger."""
    import comfy.model_management as mm

    patcher = getattr(clip_obj, "patcher", None)
    removed = False
    try:
        for loaded in list(mm.current_loaded_models):
            if patcher is not None and getattr(loaded, "model", None) is patcher:
                loaded.model_unload()
                try:
                    mm.current_loaded_models.remove(loaded)
                except ValueError:
                    pass
                removed = True
    except Exception as exc:  # never let cleanup break the run
        print(f"{_HIDREAM_LOG} TE unload warning: {exc}", flush=True)
    del clip_obj
    gc.collect()
    try:
        mm.cleanup_models_gc()
        mm.soft_empty_cache()
    except Exception:
        pass
    print(
        f"{_HIDREAM_LOG} TE UNLOAD COMPLETE" + ("" if removed else " (was not GPU-resident)"),
        flush=True,
    )


def materialize_hidream_conditionings(
    base_text: str,
    person_1_text: str,
    person_2_text: str,
    negative_text: str,
    *,
    clip: Any = None,
    te_files: tuple[str, str, str, str] = _HIDREAM_TE_DEFAULTS,
) -> tuple[list, list, list, list]:  # noqa: D401 - see module docstring
    """Return the 4 native HiDream conditionings, from the local cache if possible.

    Cache HIT  -> no quad CLIP is loaded at all.
    Cache MISS -> load the quad CLIP ONCE (wired `clip`, else self-load), encode
                  the 4 prompts back to back, persist them, then unload the TE
                  (only if we self-loaded it) before Phase 03 loads the GGUF.
    """
    texts = (str(base_text or ""), str(person_1_text or ""),
             str(person_2_text or ""), str(negative_text or ""))
    if not any(t.strip() for t in texts):
        raise RuntimeError(
            "Saya HiDream Couple: Phase 03 reached with no pre-encoded HiDream "
            "conditioning AND no prompt text to encode. Wire the 4 original "
            "prompt strings into this node."
        )

    key = hidream_cache.make_key(
        *texts, list(te_files), {"recipe": _ENCODE_RECIPE_TAG}
    )
    cached = hidream_cache.load(key)
    if cached is not None:
        print("[SAYA HIDREAM CACHE] HIT", flush=True)
        return tuple(cached)  # type: ignore[return-value]

    print("[SAYA HIDREAM CACHE] MISS", flush=True)
    self_loaded = clip is None
    clip_obj = clip if clip is not None else _load_quad_clip(te_files)

    print(f"{_HIDREAM_LOG} BATCH ENCODE START (4 prompt(s))", flush=True)
    try:
        conds = [
            clip_obj.encode_from_tokens_scheduled(clip_obj.tokenize(text))
            for text in texts
        ]
    finally:
        print(f"{_HIDREAM_LOG} BATCH ENCODE END", flush=True)

    try:
        hidream_cache.save(key, conds)
        print("[SAYA HIDREAM CACHE] SAVE", flush=True)
    except Exception as exc:
        print(f"[SAYA HIDREAM CACHE] WARN save failed: {exc}", flush=True)

    if self_loaded:
        _unload_self_loaded_clip(clip_obj)

    return conds[0], conds[1], conds[2], conds[3]

CONTEXT_MODES = ("main_plus_person_concat", "person_only", "main_plus_person_reencoded")
COPY_CONTEXT_MODES = ("inherit", *CONTEXT_MODES)

# In `main_plus_person_reencoded` the merged-prompt re-encode OWNS these keys; any
# copy carried from the original person conditioning must NOT overwrite them.
#  * pooled_output / conditioning_llama3 : the re-encode produced the merged text
#  * the scheduling keys                 : any schedule here belongs to the merged
#                                          text (from `[a:b:0.5]`-style syntax),
#                                          not to the original person prompt
_REENCODE_OWNED_KEYS = frozenset(
    {
        "pooled_output",
        "conditioning_llama3",
        "cross_attn",
        "model_conds",
        "mask",
        "mask_strength",
        "set_area_to_bounds",
        "start_percent",
        "end_percent",
        "timestep_start",
        "timestep_end",
    }
)
# A person-level schedule cannot be transferred onto a *different* (merged) prompt
# without guessing, so `main_plus_person_reencoded` rejects it outright.
_PERSON_SCHEDULE_KEYS = ("start_percent", "end_percent", "timestep_start", "timestep_end")

_DEFAULTS: dict[str, Any] = {
    "orientation": "horizontal",
    "center": 0.5,
    "transition": 0.03,
    "mask_floor": 0.0,
    "shared_contact_strength": 0.90,
    "contact_width": 0.24,
    "context_mode": "main_plus_person_concat",
}


# --------------------------------------------------------------------------- #
# Config
# --------------------------------------------------------------------------- #
def _clamped(raw: dict[str, Any], key: str, default: float, low: float, high: float) -> float:
    try:
        value = float(raw.get(key, default))
    except (TypeError, ValueError):
        value = default
    return min(high, max(low, value))


def normalize_config(config: Any = None) -> dict[str, Any]:
    """Normalise a SAYA_COUPLE_CONFIG bundle for the HiDream E surface.

    Reuses the Forge config vocabulary (so a bundle produced by the SDXL MASTER
    stays meaningful when a HiDream COPY inherits it); E-only keys are explicit.
    `use_couple_attention` is the *effective* flag: requested AND person-2 present.
    """
    raw = dict(config) if isinstance(config, dict) else {}
    person_2_enabled = bool(raw.get("person_2_enabled", True))
    requested = bool(raw.get("requested_use_couple_attention", raw.get("use_couple_attention", True)))

    orientation = str(raw.get("orientation", _DEFAULTS["orientation"]))
    if orientation not in ("horizontal", "vertical"):
        orientation = "horizontal"

    context_mode = str(raw.get("context_mode", _DEFAULTS["context_mode"]))
    if context_mode not in CONTEXT_MODES:
        context_mode = _DEFAULTS["context_mode"]

    return {
        "person_2_enabled": person_2_enabled,
        "requested_use_couple_attention": requested,
        "use_couple_attention": requested and person_2_enabled,
        "orientation": orientation,
        "center": _clamped(raw, "center", _DEFAULTS["center"], 0.15, 0.85),
        "transition": _clamped(raw, "transition", _DEFAULTS["transition"], 0.01, 0.20),
        "mask_floor": _clamped(raw, "mask_floor", _DEFAULTS["mask_floor"], 0.0, 0.20),
        "swap_person_positions": bool(raw.get("swap_person_positions", False)),
        "shared_contact_strength": _clamped(
            raw, "shared_contact_strength", _DEFAULTS["shared_contact_strength"], 0.0, 0.95
        ),
        "contact_width": _clamped(raw, "contact_width", _DEFAULTS["contact_width"], 0.0, 0.50),
        "include_main_contact": bool(raw.get("include_main_contact", False)),
        "add_global_main_entry": bool(raw.get("add_global_main_entry", False)),
        "context_mode": context_mode,
        "engine": "hidream-e",
    }


# --------------------------------------------------------------------------- #
# Shared implementation
# --------------------------------------------------------------------------- #
class _HiDreamCoupleBase:
    """Logic shared by the MASTER and COPY node surfaces.

    Nothing here holds state between calls: every method takes its inputs and
    returns fresh objects. Two MASTER nodes, two COPY nodes, or the same node in
    two queued prompts never share a mutable object.
    """

    # -- validation ------------------------------------------------------- #
    @staticmethod
    def _entry_list(name: str, conditioning: Any) -> list[tuple[torch.Tensor, dict]]:
        """Validate a CONDITIONING and return its entries as (context, meta).

        Family guard (mirrors regional_attention/anima.py): a native HiDream
        conditioning must be `[[rank-3 T5 tensor, dict], ...]` where each dict
        has tensor `pooled_output` and `conditioning_llama3`. We fail loudly
        instead of silently degrading — a silent no-op is the project's #1 risk.
        Multiple entries are allowed (scheduling / combine / hooks).
        """
        if not isinstance(conditioning, list) or len(conditioning) == 0:
            raise RuntimeError(
                f"Saya HiDream Couple: {name} is empty or not a CONDITIONING list "
                "(re-encode the prompt with the HiDream quad CLIP)"
            )
        out: list[tuple[torch.Tensor, dict]] = []
        for index, entry in enumerate(conditioning):
            if not isinstance(entry, (list, tuple)) or len(entry) != 2:
                raise RuntimeError(f"Saya HiDream Couple: {name}[{index}] is not [tensor, metadata]")
            context, meta = entry
            if not isinstance(context, torch.Tensor) or context.ndim != 3:
                raise RuntimeError(
                    f"Saya HiDream Couple: {name}[{index}] context must be a rank-3 T5 tensor [B,T,4096]"
                )
            if not isinstance(meta, dict):
                raise RuntimeError(f"Saya HiDream Couple: {name}[{index}] metadata must be a dict")
            if not isinstance(meta.get("pooled_output"), torch.Tensor):
                raise RuntimeError(
                    f"Saya HiDream Couple: {name}[{index}] has no pooled_output tensor — this is "
                    "not a native HiDream conditioning (family guard, cf. ANIMA adapter)"
                )
            if not isinstance(meta.get("conditioning_llama3"), torch.Tensor):
                raise RuntimeError(
                    f"Saya HiDream Couple: {name}[{index}] has no conditioning_llama3 tensor — this "
                    "is not a native HiDream conditioning (family guard, cf. ANIMA adapter)"
                )
            out.append((context, meta))
        return out

    # Shared with the Forge couple node; see src/services/conditioning.py.
    _copy_conditioning = staticmethod(_shared_copy_conditioning)

    @staticmethod
    def _read_latent(latent: Any) -> tuple[int, int, int]:
        """Return (batch, latent_height, latent_width) from a LATENT dict.

        The regional masks must match the latent the KSampler actually denoises,
        so the node reads the same latent that feeds the sampler.
        """
        if not isinstance(latent, dict) or not isinstance(latent.get("samples"), torch.Tensor):
            raise RuntimeError("Saya HiDream Couple: latent must contain latent['samples'] tensor")
        samples = latent["samples"]
        if samples.ndim != 4:
            raise RuntimeError(
                "Saya HiDream Couple: latent['samples'] must be [batch, channels, height, width]"
            )
        batch, _channels, lat_h, lat_w = samples.shape
        if min(batch, lat_h, lat_w) < 1:
            raise RuntimeError("Saya HiDream Couple: latent has an empty dimension")
        return int(batch), int(lat_h), int(lat_w)

    # -- context construction ------------------------------------------- #
    @staticmethod
    def _concat_main_person(
        main_entry: tuple[torch.Tensor, dict],
        person_context: torch.Tensor,
        person_meta: dict,
        batch: int,
    ) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
        """main_plus_person_concat: token-concatenate MAIN and person sequences.

        T5 context   : cat on dim 1  -> [B, T_main + T_person, 4096]
        llama3       : cat on dim 2  -> [B, 32, L_main + L_person, 4096]
        pooled_output: MAIN's (a model-dimension vector, not tokens; concatenating
                       two pooled vectors would be meaningless).

        Each tensor is repeated/truncated to the latent batch BEFORE the concat
        (mirrors comfy.conds.CONDRegular.process_cond), so a MAIN encoded at
        batch 1 and a person at batch 2 stay valid. Token lengths may differ;
        the HiDream DiT never hardcodes 128 (model.py:761-780 indexes per layer
        then projects on the last dim).

        Shape validity is NOT semantic validity: the two prompts were encoded
        separately. Quality is a RUNTIME comparison (see the report §13).
        """
        main_context, main_meta = main_entry
        context = torch.cat(
            (
                comfy.utils.repeat_to_batch_size(main_context, batch),
                comfy.utils.repeat_to_batch_size(person_context, batch),
            ),
            dim=1,
        )
        llama3 = torch.cat(
            (
                comfy.utils.repeat_to_batch_size(main_meta["conditioning_llama3"], batch),
                comfy.utils.repeat_to_batch_size(person_meta["conditioning_llama3"], batch),
            ),
            dim=2,
        )
        pooled = comfy.utils.repeat_to_batch_size(main_meta["pooled_output"], batch)
        return context, pooled, llama3

    @classmethod
    def _reencode_merged_prompt(
        cls, clip: Any, main_text: str, person_text: str
    ) -> list[tuple[torch.Tensor, dict]]:
        """main_plus_person_reencoded: really run the quad CLIP on a merged prompt.

        Returns native HiDream entries (may itself be multi-entry if scheduled
        prompt syntax is used). No embedding concat — this is a true encode.
        """
        if clip is None:
            raise RuntimeError(
                "Saya HiDream Couple: context_mode 'main_plus_person_reencoded' needs the `clip` "
                "socket (the HiDream quad CLIP) wired in"
            )
        main_text = (main_text or "").strip()
        person_text = (person_text or "").strip()
        if not person_text:
            raise RuntimeError(
                "Saya HiDream Couple: context_mode 'main_plus_person_reencoded' needs a non-empty "
                "person text for every active person"
            )
        merged = f"{main_text}. {person_text}" if main_text else person_text
        conditioning = clip.encode_from_tokens_scheduled(clip.tokenize(merged))
        return cls._entry_list("re-encoded merged prompt", conditioning)

    # -- regional entries --------------------------------------------------- #
    @staticmethod
    def _carry_forward_metadata(person_meta: dict) -> dict:
        """Region-intent metadata to move from the ORIGINAL person conditioning
        onto the entries of a re-encoded merged prompt.

        Everything the person authored that expresses *where / how strongly this
        person's prompt applies* — `hooks` (e.g. a regional LoRA), `control`,
        `strength`, `area`, `gligen`, and any custom key — is preserved. The keys
        in `_REENCODE_OWNED_KEYS` are dropped because the re-encoded merged prompt
        legitimately owns them (see the constant). Person schedules are handled
        (rejected) upstream, so they never reach here.
        """
        return {k: v for k, v in person_meta.items() if k not in _REENCODE_OWNED_KEYS}

    @classmethod
    def _reencoded_region_source(
        cls,
        region_label: str,
        person_entries: list[tuple[torch.Tensor, dict]],
        clip: Any,
        main_text: str,
        person_text: str,
    ) -> tuple[list[tuple[torch.Tensor, dict]], dict]:
        """Return (re-encoded entries, metadata carried from the person prompt).

        Metadata contract for `main_plus_person_reencoded`
        -------------------------------------------------
        The re-encoded merged prompt provides context / pooled_output /
        conditioning_llama3 (and any scheduling its own `[a:b:t]` syntax created).
        The ORIGINAL person conditioning provides region intent (hooks, control,
        strength, area, gligen, custom keys) — merged onto every re-encoded entry.

        Two combinations are rejected instead of guessed:
          * the person conditioning has >1 entry — there is no non-arbitrary way
            to associate N person entries with the M re-encoded entries;
          * the person's single entry carries a schedule — it was authored for
            the person prompt, not for the different merged prompt.
        In both cases use `person_only` or `main_plus_person_concat`.
        """
        if len(person_entries) > 1:
            raise RuntimeError(
                f"Saya HiDream Couple: {region_label} has {len(person_entries)} conditioning "
                "entries; 'main_plus_person_reencoded' cannot map them onto the re-encoded "
                "merged prompt without an arbitrary association. Use 'person_only' or "
                "'main_plus_person_concat', or reduce the person prompt to a single entry."
            )
        person_meta = person_entries[0][1] if person_entries else {}
        scheduled = [key for key in _PERSON_SCHEDULE_KEYS if key in person_meta]
        if scheduled:
            raise RuntimeError(
                f"Saya HiDream Couple: {region_label} person conditioning carries a schedule "
                f"({', '.join(scheduled)}); it was authored for the person prompt, not for the "
                "re-encoded merged prompt. Use 'person_only' or 'main_plus_person_concat'."
            )
        source = cls._reencode_merged_prompt(clip, main_text, person_text)
        return source, cls._carry_forward_metadata(person_meta)

    @classmethod
    def _regional_entries_for(
        cls,
        region_label: str,
        person_entries: list[tuple[torch.Tensor, dict]],
        region_mask_latent: torch.Tensor,
        main_entry: tuple[torch.Tensor, dict] | None,
        mode: str,
        batch: int,
        clip: Any,
        main_text: str,
        person_text: str,
    ) -> list[list]:
        """Build the native CONDITIONING entries for one region (P1 or P2).

        One output entry per source entry, so prompt scheduling / combine on a
        person prompt survives. `region_mask_latent` is [B, H_lat, W_lat].
        In `main_plus_person_reencoded` mode `source` is the re-encoded merged
        prompt and `carried` is the region-intent metadata pulled from the
        original person conditioning (see `_reencoded_region_source`).
        """
        carried: dict = {}
        if mode == "main_plus_person_reencoded":
            source, carried = cls._reencoded_region_source(
                region_label, person_entries, clip, main_text, person_text
            )
            use_person_pool_llama = True  # the re-encode already merged MAIN
        else:
            source = person_entries
            use_person_pool_llama = mode == "person_only" or main_entry is None

        entries: list[list] = []
        for context, meta in source:
            if use_person_pool_llama:
                new_context = context
                new_pooled = meta["pooled_output"]
                new_llama3 = meta["conditioning_llama3"]
            else:  # main_plus_person_concat with a real MAIN entry
                new_context, new_pooled, new_llama3 = cls._concat_main_person(
                    main_entry, context, meta, batch
                )
            # Start from the source entry's own metadata, then layer the carried
            # region intent on top (empty except in re-encoded mode).
            new_meta = dict(meta)
            new_meta.update(carried)
            new_meta["pooled_output"] = new_pooled
            new_meta["conditioning_llama3"] = new_llama3
            new_meta["mask"] = region_mask_latent
            new_meta["mask_strength"] = 1.0
            new_meta["set_area_to_bounds"] = False
            if "area" in new_meta:
                # A person prompt should not already be regionalised; keep the
                # area but let the caller know the two will interact.
                print(
                    f"[Saya HiDream Couple E] note: {region_label} entry already has an 'area'; "
                    "the region mask is applied on top of it"
                )
            entries.append([new_context, new_meta])
        return entries

    @staticmethod
    def _global_main_entries(
        main_entries: list[tuple[torch.Tensor, dict]]
    ) -> list[list]:
        """MAIN as an UNMASKED entry: contributes to every pixel.

        With `add_global_main_entry` the sampler averages `D_main` in everywhere
        and the masked person predictions in their regions (normalised by the
        weight sum). Use it to keep the scene prompt present in `person_only`
        mode. Off by default because the *_concat / *_reencoded modes already
        carry MAIN inside every region.
        """
        out: list[list] = []
        for context, meta in main_entries:
            new_meta = dict(meta)
            new_meta.pop("mask", None)
            new_meta.pop("mask_strength", None)
            out.append([context, new_meta])
        return out

    @classmethod
    def _contact_entries(
        cls,
        main_entries: list[tuple[torch.Tensor, dict]],
        contact_mask_latent: torch.Tensor,
    ) -> list[list]:
        """MAIN restricted to the soft contact band (Forge COARSE MAIN 2.3).

        One masked entry PER MAIN entry (MAIN may be multi-entry in `person_only`
        / `main_plus_person_reencoded`). `routing_masks` already reduced P1/P2 by
        the band weight `shared` exactly ONCE, so to keep the per-pixel partition
        at 1 the band mask is split evenly: each of the N entries gets
        `contact / N`. The sampler then averages the N MAIN prompts inside the
        band (its native behaviour for several conds over one region) while
        `out_counts` still sums to `shared` there, not `N * shared`.

        Each MAIN entry keeps its own metadata (a schedule on one MAIN entry
        simply drops it from the band for that step — correct: an inactive prompt
        should not contribute; the band is then momentarily under-weighted by
        `contact / N` at that pixel, which is acceptable).
        """
        count = max(1, len(main_entries))
        share = contact_mask_latent if count == 1 else contact_mask_latent / float(count)
        out: list[list] = []
        for context, meta in main_entries:
            new_meta = dict(meta)
            new_meta["mask"] = share
            new_meta["mask_strength"] = 1.0
            new_meta["set_area_to_bounds"] = False
            out.append([context, new_meta])
        return out

    # -- detailers -------------------------------------------------------- #
    @classmethod
    def _masked_detailer_positive(
        cls,
        person_1: Any,
        person_2: Any,
        mask_p1_pixel: torch.Tensor,
        mask_p2_pixel: torch.Tensor,
        couple_active: bool,
        solo: bool = True,
    ) -> list[list]:
        """Per-person conditioning for detailers — NOT for the main KSampler.

        Detailers run their own sampler on the UN-wrapped model and crop in
        pixel space (`crop_condition_mask`), so these masks are PIXEL resolution.
        Pairing is explicit `(conditioning, mask)`: P1 entries always get
        `mask_p1`, P2 entries always get `mask_p2`, regardless of list length or
        identity. When the couple engine is not active, Forge parity
        (forge/node.py:435,452): SOLO keeps the original defensive `person_1 or
        person_2` copy (no mask metadata); FALLBACK concatenates P1+P2 so a
        missing person/negative never silently drops the other person's detailer
        conditioning.
        """
        if not couple_active:
            if solo:
                return cls._copy_conditioning(person_1 or person_2)
            return cls._copy_conditioning(person_1) + cls._copy_conditioning(person_2)
        pairs = [(person_1, mask_p1_pixel)]
        if person_2:
            pairs.append((person_2, mask_p2_pixel))
        out: list[list] = []
        for conditioning, mask in pairs:
            for entry in cls._copy_conditioning(conditioning):
                meta = dict(entry[1])
                meta["mask"] = mask
                meta["mask_strength"] = 1.0
                meta["set_area_to_bounds"] = False
                out.append([entry[0], meta])
        return out

    # -- public pixel masks -------------------------------------------------- #
    @classmethod
    def _public_pixel_masks(
        cls, batch: int, lat_h: int, lat_w: int, config: dict, couple_active: bool
    ) -> tuple[torch.Tensor, torch.Tensor]:
        px_h, px_w = lat_h * 8, lat_w * 8
        if not couple_active:
            return (
                torch.ones((batch, px_h, px_w)),
                torch.zeros((batch, px_h, px_w)),
            )
        return pixel_masks(
            batch, px_h, px_w,
            str(config["orientation"]),
            float(config["center"]),
            float(config["transition"]),
            float(config["mask_floor"]),
            bool(config["swap_person_positions"]),
        )

    # -- the shared body --------------------------------------------------- #
    @classmethod
    def _run(
        cls,
        main_positive: Any,
        person_1_positive: Any,
        person_2_positive: Any,
        negative: Any,
        latent: Any,
        config: dict,
        clip: Any = None,
        main_text: str = "",
        person_1_text: str = "",
        person_2_text: str = "",
    ) -> tuple[list, list, list, torch.Tensor, torch.Tensor, dict]:
        """Return (positive, detailer_positive, negative, mask_p1, mask_p2, config).

        `positive` is what you wire into `KSampler.positive`. In COUPLE mode it is
        the list of regional entries; in SOLO/FALLBACK it degrades exactly like
        the Forge node so existing automatic phases keep working.
        """
        batch, lat_h, lat_w = cls._read_latent(latent)
        negative_public = cls._copy_conditioning(negative)

        p1_entries = cls._entry_list("Person 1", person_1_positive) if person_1_positive else []
        p2_entries = cls._entry_list("Person 2", person_2_positive) if person_2_positive else []
        main_entries = cls._entry_list("Base", main_positive) if main_positive else []

        person_2_present = bool(p2_entries) and bool(config["person_2_enabled"])
        couple_active = bool(config["use_couple_attention"]) and bool(p1_entries) and person_2_present and bool(negative_public)

        # ---- SOLO / FALLBACK (Forge parity) --------------------------------
        if not couple_active:
            solo = not bool(config["requested_use_couple_attention"])
            main_public = cls._copy_conditioning(main_positive)
            p1_public = cls._copy_conditioning(person_1_positive)
            p2_public = cls._copy_conditioning(person_2_positive) if person_2_present else []
            if solo:
                # SOLO: concat MAIN + P1 (or whichever exists).
                if main_public and p1_public:
                    from nodes import ConditioningConcat

                    positive = ConditioningConcat().concat(main_public, p1_public)[0]
                else:
                    positive = cls._copy_conditioning(main_public or p1_public)
            else:
                # FALLBACK: a person/negative is missing — keep every prompt as a
                # plain list so nothing is silently dropped.
                positive = main_public + p1_public + p2_public
            detailer_positive = cls._masked_detailer_positive(
                person_1_positive, person_2_positive, None, None, couple_active=False, solo=solo
            )
            mask_p1, mask_p2 = cls._public_pixel_masks(batch, lat_h, lat_w, config, couple_active=False)
            print("[Saya HiDream Couple E] engine=off (SOLO/FALLBACK): no regional conditioning built")
            return positive, detailer_positive, negative_public, mask_p1, mask_p2, dict(config)

        # ---- COUPLE ------------------------------------------------------
        mode = str(config["context_mode"])
        main_entry = main_entries[0] if main_entries else None
        if mode == "main_plus_person_concat" and len(main_entries) > 1:
            raise RuntimeError(
                "Saya HiDream Couple: 'main_plus_person_concat' needs MAIN to be a single "
                "conditioning entry (found %d). Use 'person_only' or 'main_plus_person_reencoded' "
                "for a scheduled/combined MAIN." % len(main_entries)
            )
        if mode == "main_plus_person_concat" and main_entry is None:
            print(
                "[Saya HiDream Couple E] warning: no Base conditioning — regions fall back to "
                "person-only context; the scene prompt is not represented"
            )

        include_contact = bool(config["include_main_contact"]) and bool(main_entries)
        route_p1, route_p2, contact = routing_masks(
            batch, lat_h, lat_w,
            str(config["orientation"]),
            float(config["center"]),
            float(config["transition"]),
            float(config["mask_floor"]),
            float(config["shared_contact_strength"]),
            float(config["contact_width"]),
            include_contact,
            bool(config["swap_person_positions"]),
        )

        positive: list[list] = []
        positive += cls._regional_entries_for(
            "P1", p1_entries, route_p1, main_entry, mode, batch, clip, main_text, person_1_text
        )
        positive += cls._regional_entries_for(
            "P2", p2_entries, route_p2, main_entry, mode, batch, clip, main_text, person_2_text
        )
        if contact is not None and main_entries:
            positive += cls._contact_entries(main_entries, contact)
        if bool(config["add_global_main_entry"]) and main_entries:
            positive += cls._global_main_entries(main_entries)

        mask_p1, mask_p2 = cls._public_pixel_masks(batch, lat_h, lat_w, config, couple_active=True)
        detailer_positive = cls._masked_detailer_positive(
            person_1_positive, person_2_positive, mask_p1, mask_p2, couple_active=True
        )

        print(
            f"[Saya HiDream Couple E] engine=native regions={len(positive)} "
            f"context_mode={mode} contact={'on' if contact is not None else 'off'} "
            f"global_main={'on' if bool(config['add_global_main_entry']) else 'off'}"
        )
        return positive, detailer_positive, negative_public, mask_p1, mask_p2, dict(config)


# --------------------------------------------------------------------------- #
# Node surfaces
# --------------------------------------------------------------------------- #
_COMMON_REQUIRED = {
    "main_positive": ("CONDITIONING",),
    "person_1_positive": ("CONDITIONING",),
    "person_2_positive": ("CONDITIONING",),
    "negative": ("CONDITIONING",),
    "latent": ("LATENT",),
}

_REENCODE_OPTIONAL = {
    # Only consumed when context_mode == "main_plus_person_reencoded".
    "clip": ("CLIP",),
    "main_text": ("STRING", {"multiline": True, "default": ""}),
    "person_1_text": ("STRING", {"multiline": True, "default": ""}),
    "person_2_text": ("STRING", {"multiline": True, "default": ""}),
}


class SayaComfyCoupleHiDream(_HiDreamCoupleBase):
    """MASTER surface: build regional HiDream conditioning + emit the config bundle.

    No MODEL socket: architecture E does not touch the model. Wire the HiDream
    MODEL straight through `ModelSamplingSD3` into the KSampler; wire this node's
    `positive` output into `KSampler.positive`.
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, Any]:
        return {
            "required": {
                **_COMMON_REQUIRED,
                "use_couple_attention": ("BOOLEAN", {"default": True}),
                "context_mode": (list(CONTEXT_MODES), {"default": _DEFAULTS["context_mode"]}),
                "orientation": (["horizontal", "vertical"], {"default": "horizontal"}),
                "center": ("FLOAT", {"default": 0.5, "min": 0.15, "max": 0.85, "step": 0.01}),
                "transition": ("FLOAT", {"default": 0.03, "min": 0.01, "max": 0.20, "step": 0.01}),
                "mask_floor": ("FLOAT", {"default": 0.0, "min": 0.0, "max": 0.20, "step": 0.01}),
                "swap_person_positions": ("BOOLEAN", {"default": False}),
            },
            "optional": {
                "couple_config": (COUPLE_CONFIG_TYPE,),
                "include_main_contact": ("BOOLEAN", {"default": False}),
                "add_global_main_entry": ("BOOLEAN", {"default": False}),
                "shared_contact_strength": ("FLOAT", {"default": 0.90, "min": 0.0, "max": 0.95, "step": 0.01}),
                "contact_width": ("FLOAT", {"default": 0.24, "min": 0.0, "max": 0.50, "step": 0.01}),
                **_REENCODE_OPTIONAL,
            },
        }

    RETURN_TYPES = ("CONDITIONING", "CONDITIONING", "CONDITIONING", "MASK", "MASK", COUPLE_CONFIG_TYPE)
    RETURN_NAMES = (
        "positive",           # -> KSampler.positive  (regional; SOLO/FALLBACK degrade)
        "detailer_positive",  # -> detailers only, NEVER this KSampler
        "negative",           # -> KSampler.negative  (NEG untouched)
        "mask_person_1",      # pixel resolution, Forge-compatible
        "mask_person_2",
        "couple_config",
    )
    FUNCTION = "run"
    CATEGORY = "saya/hidream"

    def run(
        self,
        main_positive: Any,
        person_1_positive: Any,
        person_2_positive: Any,
        negative: Any,
        latent: Any,
        use_couple_attention: bool = True,
        context_mode: str = _DEFAULTS["context_mode"],
        orientation: str = "horizontal",
        center: float = 0.5,
        transition: float = 0.03,
        mask_floor: float = 0.0,
        swap_person_positions: bool = False,
        couple_config: Any = None,
        include_main_contact: bool = False,
        add_global_main_entry: bool = False,
        shared_contact_strength: float = 0.90,
        contact_width: float = 0.24,
        clip: Any = None,
        main_text: str = "",
        person_1_text: str = "",
        person_2_text: str = "",
    ) -> tuple[Any, ...]:
        incoming = couple_config if isinstance(couple_config, dict) else {}
        config = normalize_config(
            {
                **incoming,
                "person_2_enabled": bool(person_2_positive),
                "use_couple_attention": use_couple_attention,
                "requested_use_couple_attention": use_couple_attention,
                "context_mode": context_mode,
                "orientation": orientation,
                "center": center,
                "transition": transition,
                "mask_floor": mask_floor,
                "swap_person_positions": swap_person_positions,
                "include_main_contact": include_main_contact,
                "add_global_main_entry": add_global_main_entry,
                "shared_contact_strength": shared_contact_strength,
                "contact_width": contact_width,
            }
        )
        return self._run(
            main_positive, person_1_positive, person_2_positive, negative, latent, config,
            clip, main_text, person_1_text, person_2_text,
        )


class SayaComfyCoupleHiDreamCopy(_HiDreamCoupleBase):
    """COPY surface: reuse a MASTER config with this branch's own encodes.

    `context_mode` defaults to "inherit" (this also works through the frontend
    widget); pick a real mode to override. No config output — COPY consumes one.
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, Any]:
        return {
            "required": {
                **_COMMON_REQUIRED,
                "couple_config": (COUPLE_CONFIG_TYPE,),
            },
            "optional": {
                "context_mode": (list(COPY_CONTEXT_MODES), {"default": "inherit"}),
                "include_main_contact": ("BOOLEAN", {"default": False}),
                "add_global_main_entry": ("BOOLEAN", {"default": False}),
                **_REENCODE_OPTIONAL,
                "negative_text": ("STRING", {"multiline": True, "default": ""}),
                # Phase-03 deferred HiDream encode. Wire the Phase-01 prompt
                # bundle JSON here (SayaImagePhase3Load.positive_prompt). When the
                # 4 CONDITIONING inputs are empty this node reads the 4 original
                # prompts from the bundle, checks the local conditioning cache,
                # and only loads the quad CLIP on a real miss -- so nothing
                # HiDream is touched before Phase 03.
                "prompt_bundle_json": ("STRING", {"forceInput": True}),
                "hidream_trigger": ("STRING", {"forceInput": True}),
            },
        }

    RETURN_TYPES = ("CONDITIONING", "CONDITIONING", "CONDITIONING", "MASK", "MASK")
    RETURN_NAMES = ("positive", "detailer_positive", "negative", "mask_person_1", "mask_person_2")
    FUNCTION = "run_copy"
    CATEGORY = "saya/hidream"

    def run_copy(
        self,
        main_positive: Any,
        person_1_positive: Any,
        person_2_positive: Any,
        negative: Any,
        latent: Any,
        couple_config: Any,
        context_mode: str = "inherit",
        include_main_contact: bool = False,
        add_global_main_entry: bool = False,
        clip: Any = None,
        main_text: str = "",
        person_1_text: str = "",
        person_2_text: str = "",
        negative_text: str = "",
        prompt_bundle_json: str = "",
        hidream_trigger: str = "",
    ) -> tuple[Any, ...]:
        # Deferred-until-Phase-03 path: if the 4 CONDITIONING inputs are empty
        # (nothing was encoded in Phase 01), resolve them now -- cache first, and
        # only load the ~13.8 GB quad CLIP on a real miss.
        pre_encoded = all(
            _has_entries(c)
            for c in (main_positive, person_1_positive, person_2_positive, negative)
        )
        if not pre_encoded or hidream_trigger.strip():
            b_txt, p1_txt, p2_txt, neg_txt = (
                str(main_text or ""), str(person_1_text or ""),
                str(person_2_text or ""), str(negative_text or ""),
            )
            if prompt_bundle_json and not any((b_txt, p1_txt, p2_txt, neg_txt)):
                b_txt, p1_txt, p2_txt, neg_txt = _prompts_from_bundle(prompt_bundle_json)
            if hidream_trigger.strip() and not any((b_txt, p1_txt, p2_txt)):
                raise ValueError("Saya HiDream trigger needs original positive text or prompt_bundle_json.")
            b_txt, p1_txt, p2_txt = (
                inject_hidream_trigger(t, hidream_trigger) for t in (b_txt, p1_txt, p2_txt)
            )
            print(f"[SAYA HIDREAM PROMPT] trigger={hidream_trigger.strip()!r}", flush=True)
            # The cache hashes these final texts; diffusion-only LoRA settings
            # do not belong in a text-conditioning cache key.
            main_text, person_1_text, person_2_text = b_txt, p1_txt, p2_txt
            main_positive, person_1_positive, person_2_positive, negative = (
                materialize_hidream_conditionings(
                    b_txt, p1_txt, p2_txt, neg_txt, clip=clip,
                )
            )

        incoming = dict(couple_config) if isinstance(couple_config, dict) else {}
        if context_mode != "inherit":
            incoming["context_mode"] = context_mode
        # A COPY still derives "does person 2 exist" from its OWN inputs.
        incoming["person_2_enabled"] = bool(person_2_positive)
        # OR-merge: True wins, otherwise keep whatever the config already carried.
        incoming["include_main_contact"] = include_main_contact or incoming.get("include_main_contact", False)
        if add_global_main_entry:
            incoming["add_global_main_entry"] = True
        config = normalize_config(incoming)
        positive, detailer_positive, negative_public, mask_p1, mask_p2, _config = self._run(
            main_positive, person_1_positive, person_2_positive, negative, latent, config,
            clip, main_text, person_1_text, person_2_text,
        )
        return positive, detailer_positive, negative_public, mask_p1, mask_p2


NODE_CLASS_MAPPINGS = {
    "SayaComfyCoupleHiDream": SayaComfyCoupleHiDream,
    "SayaComfyCoupleHiDreamCopy": SayaComfyCoupleHiDreamCopy,
}

NODE_DISPLAY_NAME_MAPPINGS = {
    "SayaComfyCoupleHiDream": "Saya Comfy Couple - HiDream (native regional)",
    "SayaComfyCoupleHiDreamCopy": "Saya Comfy Couple - HiDream - COPY",
}

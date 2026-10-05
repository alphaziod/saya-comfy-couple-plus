"""SayaCoupleImprintLoad + SayaCoupleReconstruct — Phase 02 reconstruction.

Mandatory sequence — each step can fail EXPLICITLY:

1. Read the imprint (STRICT v2; v1 = explicit "version not supported" error,
   never a silent migration).
2. Verify the recipe: STRUCTURED identities from the checkpoint hub
   (EXPLICIT INPUT ``checkpoint_identities`` — canonical JSON of
   {role, identifier, source}) vs. the imprint's ``checkpoint_identity`` /
   ``clip_identity``. EXACT equality of basenames, no normalization.
   Mismatch = HARD ERROR listing BOTH values; missing identity = HARD ERROR
   "identity missing — cannot verify". **MUST NOT consult ``models_used``**
   (hand-maintained STRING widget, not reliable — this node has NO code
   path that reads the manifest).
   No runtime comparison of ``lora_effective_chain`` — the chain's provenance
   is guaranteed STRUCTURALLY (same hub, same route); the node applies NO
   LoRA.
3. Re-encode MAIN / P1 / P2 / NEG with the phase CLIP — each cond produced by
   its OWN encoder call fed by its OWN imprint field (STRUCTURAL guard:
   provenance log + object non-aliasing; CONTENT inequality is only a
   WARNING — two identical prompts can be intentional).
4. Two independent patches are built from the SAME re-encoded conditioning,
   on two SEPARATE clones of the input model (never chained):
   - ``model_patched`` — the PPM vendor (``SayaAttentionCouplePPM``), which
     understands ``saya_couple_crop`` at runtime (crop/tile-aware — USDU,
     Detailers). Weight is baked into the mask amplitude
     (``region_masks.derive_masks``); strength written on P1/P2's PPM
     channel (``cond[0][1]["strength"]``, ``write_strength``).
   - ``model_patched_multimask`` — the SAME MultiMaskCouple core Phase 1 Sampler 2
     uses (``saya_multi_couple.apply_multimask_couple`` ->
     ``custom_nodes.MultiMaskCouple.attention_couple``). Weight travels on
     the CONDITIONING's ``mask_strength`` (raw, unweighted masks from
     ``region_masks.derive_raw_region_masks``). MultiMaskCouple has no
     runtime crop-awareness mechanism, so this output is for FULL-FRAME
     consumers only (Hires Fix, Phase 6) — never for tiled/cropped passes.
   Anti-double-hook guard (sentinel
   ``model_options['transformer_options']['saya_couple_patch']`` + a
   conservative scan of both patch families) runs once, on the shared
   input model, before either patch is applied.
5. Geometry re-derived at the CURRENT resolution (input image) — never from
   the imprint's ``reference_width/height``.

Returns: (model_patched MODEL [PPM, crop-aware], model_patched_multimask
MODEL [MultiMaskCouple, full-frame only], positive CONDITIONING = global
MAIN, negative CONDITIONING, verification_report STRING). The P1/P2 conds
stay INTERNAL to each patch (captured by the patch, not returned).
"""

from __future__ import annotations

import copy
import json
from typing import Any, Callable

from . import conditioning_cache as cache
from .couple_imprint_v2 import (
    OWNERSHIP_BACKGROUND,
    ROLE_BASE_CLIP,
    ROLE_MODEL_2,
    ROLE_PHASE_MODEL,
    build_prompts,
    SCHEMA_VERSION,
    SayaCoupleImprintError,
    build_identity,
    canonical_imprint_json,
    parse_imprint_json,
    validate_imprint_v2,
    scene_text,
)
from .region_masks import SayaMaskError, attention_weights, derive_background_mask, derive_masks, derive_raw_region_masks
from .saya_attention_couple import SayaAttentionCouplePPM
from .saya_multi_couple import apply_multimask_couple

#: Family of the SDXL conditionings disk cache.
FAMILY = "sdxl"

#: PNG key carrying the imprint.
IMPRINT_METADATA_KEY = "saya_couple_imprint"

#: Anti-double-hook sentinel: written on each patched clone, checked on the input.
COUPLE_PATCH_SENTINEL = "saya_couple_patch"

#: model_options families set by the PPM patch (both of them). REAL keys
#: written by ModelPatcher.set_model_attn2_patch / set_model_attn2_output_patch.
_PPM_PATCH_FAMILIES = ("attn2_patch", "attn2_output_patch")

#: Modules whose attn2/attn2-output (PPM) or patches_replace["attn2"]
#: (MultiMaskCouple) entries count as "couple" — a conservative fallback
#: scan for unmarked patches from either engine. A legitimate non-couple
#: patch (DetailDaemon etc.) is NEVER rejected by this scan.
_COUPLE_MODULE_MARKERS = ("attention_couple", "multimaskcouple")


class SayaCoupleReconstructError(ValueError):
    """Raised for any Phase 02 contract failure (hard errors — no silent fallback)."""


# ---------------------------------------------------------------------------
# Injectable encoder (tests without ComfyUI; runtime = core CLIPTextEncode)
# ---------------------------------------------------------------------------

def _default_encode_text(clip: Any, text: str) -> Any:
    """Encode a prompt with the phase CLIP (core CLIPTextEncode: text+clip)."""
    try:
        from nodes import CLIPTextEncode  # type: ignore
    except ImportError as error:  # pragma: no cover - ComfyUI environment absent
        raise SayaCoupleReconstructError(
            "encode: ComfyUI unavailable — inject a test encoder via "
            "nodes_couple_reconstruct._encode_text"
        ) from error
    return CLIPTextEncode().encode(clip, text)[0]


#: Test injection point (the runtime never touches this).
_encode_text: Callable[[Any, str], Any] = _default_encode_text


# ---------------------------------------------------------------------------
# Injectable patches (tests without ComfyUI; runtime = the two engines below)
# ---------------------------------------------------------------------------

def _default_apply_couple_patch(model: Any, base_cond: Any, base_mask: Any,
                                cond_1: Any, mask_1: Any, cond_2: Any, mask_2: Any,
                                cond_3: Any = None, mask_3: Any = None) -> Any:
    """PPM vendor patch (crop/tile-aware — USDU, Detailers). Exactly once."""
    # cond_3/mask_3 = the scene-only background region of the Sampler 1 map (never a third person:
    # P2 absent is a HARD ERROR upstream).
    patched, = SayaAttentionCouplePPM().couple(
        model, base_cond, base_mask, cond_1, mask_1, cond_2, mask_2, cond_3, mask_3
    )
    return patched


#: Test injection point (the runtime never touches this).
_apply_couple_patch: Callable[..., Any] = _default_apply_couple_patch


def _default_apply_multimask_patch(
    model: Any, clip: Any, mask_1: Any, mask_2: Any,
    cond_1: Any, neg_1: Any, cond_2: Any, neg_2: Any,
    strength_1: float, strength_2: float,
    base_weight: float, person_weight: float, main: Any,
    scene: Any = None, background_mask: Any = None,
) -> Any:
    """MultiMaskCouple patch (full-frame only — Hires Fix, Phase 6), reusing
    the same core Phase 1 Sampler 2 uses (``saya_multi_couple.apply_multimask_couple``).
    Returns the patched MODEL; positive/negative are decided by the caller.
    """
    return apply_multimask_couple(
        model, clip, mask_1, mask_2, cond_1, neg_1, cond_2, neg_2,
        strength_1, strength_2, base_weight, person_weight, main=main,
        scene=scene, background_mask=background_mask,
    )


#: Test injection point (the runtime never touches this).
_apply_multimask_patch: Callable[..., Any] = _default_apply_multimask_patch


# ---------------------------------------------------------------------------
# Anti-double-hook guard: sentinel THEN conservative scan
#
# Checked ONCE against the shared input model, before EITHER patch is built
# (both ``model_patched`` and ``model_patched_multimask`` derive from the
# same unpatched source via independent ``.clone()`` calls inside each
# engine — never chained). PPM patches the additive
# ``attn2_patch``/``attn2_output_patch`` families; MultiMaskCouple patches
# ``patches_replace["attn2"]`` (a REPLACE family) and unconditionally RESETS
# it on every call (vendor ``attention_couple.py``), so a double-hook there
# would silently DISCARD a prior couple patch rather than stack — the guard
# below catches this BEFORE the reset, as a hard error instead of a silent
# loss.
# ---------------------------------------------------------------------------

def _patch_families(model_options: Any) -> dict[str, list[Any]]:
    transformer = (model_options or {}).get("transformer_options", {}) or {}
    patches = transformer.get("patches", {}) or {}
    return {family: list(patches.get(family, []) or []) for family in _PPM_PATCH_FAMILIES}


def _patches_replace_attn2(model_options: Any) -> dict[str, Any]:
    transformer = (model_options or {}).get("transformer_options", {}) or {}
    return dict((transformer.get("patches_replace", {}) or {}).get("attn2", {}) or {})


def _looks_like_couple_patch(patch: Any) -> bool:
    module = str(getattr(patch, "__module__", "") or "")
    qualname = str(getattr(patch, "__qualname__", "") or getattr(patch, "__name__", "") or "")
    haystack = f"{module} {qualname}".lower()
    return any(marker in haystack for marker in _COUPLE_MODULE_MARKERS)


def detect_existing_couple_hook(model_options: Any) -> None:
    """HARD ERROR if a couple hook already exists on the incoming model.

    1) Sentinel (marker set by THIS node on a previous patch — reliable);
    2) conservative fallback: a callable of the PPM attn2/attn2-output family,
       or of ``patches_replace["attn2"]`` (MultiMaskCouple), whose module is a
       known couple module. A legitimate non-couple patch passes through.
    """
    transformer = (model_options or {}).get("transformer_options", {}) or {}
    if COUPLE_PATCH_SENTINEL in transformer:
        raise SayaCoupleReconstructError(
            "double hook: the incoming model already carries a Saya couple patch "
            f"(sentinel {COUPLE_PATCH_SENTINEL!r}) — only one couple patch per "
            "model per execution"
        )
    for family, patches in _patch_families(model_options).items():
        for patch in patches:
            if _looks_like_couple_patch(patch):
                raise SayaCoupleReconstructError(
                    f"double hook: an unmarked couple patch is already present in "
                    f"family {family!r} (module="
                    f"{getattr(patch, '__module__', '?')!r}) — only one couple patch "
                    "per model per execution"
                )
    for key, patch in _patches_replace_attn2(model_options).items():
        if _looks_like_couple_patch(patch):
            raise SayaCoupleReconstructError(
                f"double hook: an unmarked couple patch is already present in "
                f"patches_replace['attn2'][{key!r}] (module="
                f"{getattr(patch, '__module__', '?')!r}) — only one couple patch "
                "per model per execution"
            )


def mark_couple_patch(patched_model: Any, *, family: str) -> None:
    """Set the sentinel on the patched clone (after a successful patch)."""
    transformer = patched_model.model_options.setdefault("transformer_options", {})
    transformer[COUPLE_PATCH_SENTINEL] = {
        "family": family,
        "note": "set by SayaCoupleReconstruct — only one couple patch per execution",
    }


# ---------------------------------------------------------------------------
# Hub identities — EXACT equality, no normalization
# ---------------------------------------------------------------------------

def parse_hub_identities(payload: str) -> list[dict[str, str]]:
    """Parse the EXPLICIT ``checkpoint_identities`` input (canonical JSON)."""
    if isinstance(payload, list):
        entries = payload
    else:
        if not isinstance(payload, str) or not payload.strip():
            raise SayaCoupleReconstructError(
                "checkpoint_identities: empty input — an ABSENT identity means "
                "cannot verify (HARD ERROR, never a silent fallback)"
            )
        try:
            entries = json.loads(payload)
        except json.JSONDecodeError as error:
            raise SayaCoupleReconstructError(
                f"checkpoint_identities: invalid JSON: {error}"
            ) from None
    if not isinstance(entries, list) or not entries:
        raise SayaCoupleReconstructError(
            "checkpoint_identities: a non-empty list is expected — an ABSENT "
            "identity means cannot verify"
        )
    for index, entry in enumerate(entries):
        if not isinstance(entry, dict) or not all(
            isinstance(entry.get(key), str) and entry.get(key)
            for key in ("role", "identifier", "source")
        ):
            raise SayaCoupleReconstructError(
                f"checkpoint_identities[{index}]: {{role, identifier, source}} "
                "strings are mandatory"
            )
    return entries


def verify_recipe_identities(recipe: dict[str, Any], hub_identities: list[dict[str, str]]) -> list[str]:
    """imprint vs. hub: mismatch = HARD ERROR (both values); missing = HARD ERROR."""
    lines: list[str] = []
    for kind in ("checkpoint_identity", "clip_identity"):
        expected = recipe.get(kind)
        if not isinstance(expected, dict) or not expected.get("identifier"):
            raise SayaCoupleReconstructError(
                f"{kind}: identity missing — cannot verify (block absent from the "
                "imprint recipe; HARD ERROR, no fallback)"
            )
        role = expected["role"]
        hub_entry = next((h for h in hub_identities if h["role"] == role), None)
        if hub_entry is None:
            raise SayaCoupleReconstructError(
                f"{kind}: identity missing — cannot verify (no identity of role "
                f"{role!r} in the hub input; HARD ERROR)"
            )
        expected_id = expected["identifier"]
        hub_id = hub_entry["identifier"]
        # EXACT equality of basenames — NO case-folding, NO trimming, NO path
        # resolution (both values come from the same widget).
        if hub_id != expected_id:
            raise SayaCoupleReconstructError(
                f"{kind}: MISMATCH — imprint={expected_id!r} vs hub={hub_id!r} "
                "(exact equality required, no normalization)"
            )
        lines.append(f"{kind}: OK — {role} == {expected_id}")
    return lines


class SayaCoupleImprintRetarget:
    """Derive a RUNTIME-ONLY imprint + checkpoint_identities pair for a different MODEL/CLIP.

    Usage: a Model 1/2 hub selects the target ``checkpoint_identities`` (hub
    list of {role, identifier, source}); this node clones the source imprint
    in memory and replaces ONLY ``reconstruction_recipe.checkpoint_identity``
    / ``clip_identity`` with the ``phase_model``/``base_clip`` entries of that
    list, then re-validates. The persisted Phase 1 source (file / manifest)
    is never read or written here — only the derived copy is returned.

    Both outputs (derived ``imprint_json``, normalized
    ``checkpoint_identities``) come from the SAME call: they cannot drift
    apart. The hard identity check in ``SayaCoupleReconstruct`` stays
    unchanged and active on both derived values.
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "imprint_json": ("STRING", {
                    "default": "", "multiline": True, "forceInput": True,
                    "tooltip": "imprint_json output of SayaCoupleImprintResolve (or "
                               "equivalent) — never the Phase 1 source itself.",
                }),
                "checkpoint_identities": ("STRING", {
                    "default": "", "multiline": True, "forceInput": True,
                    "tooltip": "Hub list {role, identifier, source} of the target "
                               "MODEL/CLIP (phase_model + base_clip roles required).",
                }),
            },
        }

    RETURN_TYPES = ("STRING", "STRING")
    RETURN_NAMES = ("imprint_json", "checkpoint_identities")
    FUNCTION = "retarget"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Derives (in memory only) an imprint_json + checkpoint_identities pair "
        "targeting the MODEL/CLIP selected by a hub. Never touches the persisted "
        "Phase 1 source. The hard identity check stays unchanged."
    )

    def retarget(self, imprint_json: str, checkpoint_identities: str) -> tuple[str, str]:
        source = parse_imprint_json(imprint_json)
        hub = parse_hub_identities(checkpoint_identities)

        def _retargeted(role: str) -> dict[str, str]:
            entry = next((h for h in hub if h["role"] == role), None)
            if entry is None:
                raise SayaCoupleReconstructError(
                    f"retarget: checkpoint_identities has no role {role!r} — "
                    "cannot derive the imprint"
                )
            return build_identity(role=role, identifier=entry["identifier"], source=entry["source"])

        derived = copy.deepcopy(source)
        derived["reconstruction_recipe"]["checkpoint_identity"] = _retargeted(ROLE_PHASE_MODEL)
        derived["reconstruction_recipe"]["clip_identity"] = _retargeted(ROLE_BASE_CLIP)
        validate_imprint_v2(derived, context="imprint(retarget)")

        normalized_hub = json.dumps(hub, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        return canonical_imprint_json(derived), normalized_hub


class SayaCoupleCheckpointIdentities:
    """Build the strict hub identity list from live checkpoint widget strings.

    This is deliberately tiny and data-only: it prevents saved workflows from
    carrying stale duplicated checkpoint names next to the actual checkpoint
    selectors. The exact strings fed by the checkpoint hub are preserved.
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "phase_model_identifier": ("STRING", {"forceInput": True}),
                "base_clip_identifier": ("STRING", {"forceInput": True}),
            },
            "optional": {
                "model_2_identifier": ("STRING", {"forceInput": True}),
            },
        }

    RETURN_TYPES = ("STRING",)
    RETURN_NAMES = ("checkpoint_identities",)
    FUNCTION = "pack"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Builds checkpoint_identities directly from the live checkpoint hub "
        "strings. No duplicated manual model names."
    )

    def pack(
        self,
        phase_model_identifier: str,
        base_clip_identifier: str,
        model_2_identifier: str | None = None,
    ) -> tuple[str]:
        entries = [
            build_identity(role=ROLE_PHASE_MODEL, identifier=phase_model_identifier),
            build_identity(role=ROLE_BASE_CLIP, identifier=base_clip_identifier),
        ]
        if model_2_identifier:
            entries.append(build_identity(role=ROLE_MODEL_2, identifier=model_2_identifier))
        return (json.dumps(entries, sort_keys=True, separators=(",", ":"), ensure_ascii=False),)


class SayaCoupleImprintDerive:
    """Create a runtime prompt variant while preserving Couple configuration.

    Geometry, strengths and provenance come from the resolved canonical imprint.
    Only prompts and reconstruction identities are replaced. This is used by
    Naturalize so split/blur/strength/swap never become a second per-phase
    configuration surface.
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "imprint_json": ("STRING", {"forceInput": True, "multiline": True}),
                "checkpoint_identities": ("STRING", {"forceInput": True, "multiline": True}),
                "main_prompt": ("STRING", {"forceInput": True, "multiline": True}),
                "person_1_prompt": ("STRING", {"forceInput": True, "multiline": True}),
                "negative_prompt": ("STRING", {"forceInput": True, "multiline": True}),
            },
            "optional": {
                "person_2_prompt": ("STRING", {"forceInput": True, "multiline": True}),
                "solo": ("BOOLEAN", {"default": False, "forceInput": True}),
            },
        }

    RETURN_TYPES = ("SAYA_IMPRINT", "STRING")
    RETURN_NAMES = ("imprint", "imprint_json")
    FUNCTION = "derive"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Derives a runtime prompt variant from the canonical imprint while "
        "preserving geometry/strengths/provenance and retargeting identities."
    )

    def derive(
        self,
        imprint_json: str,
        checkpoint_identities: str,
        main_prompt: str,
        person_1_prompt: str,
        negative_prompt: str,
        person_2_prompt: str | None = None,
        solo: bool = False,
    ) -> tuple[dict[str, Any], str]:
        source = parse_imprint_json(imprint_json)
        hub = parse_hub_identities(checkpoint_identities)

        def _identity(role: str) -> dict[str, str]:
            entry = next((h for h in hub if h["role"] == role), None)
            if entry is None:
                raise SayaCoupleReconstructError(
                    f"derive: checkpoint_identities has no role {role!r}"
                )
            return build_identity(role=role, identifier=entry["identifier"], source=entry["source"])

        derived = copy.deepcopy(source)
        couple = derived["couple_imprint"]
        couple["prompts"] = build_prompts(
            main=main_prompt,
            person_1=person_1_prompt,
            person_2=None if solo else person_2_prompt,
            negative=negative_prompt,
        )
        derived["reconstruction_recipe"]["checkpoint_identity"] = _identity(ROLE_PHASE_MODEL)
        derived["reconstruction_recipe"]["clip_identity"] = _identity(ROLE_BASE_CLIP)
        validate_imprint_v2(derived, context="imprint(derive)")
        return derived, canonical_imprint_json(derived)


class SayaValueToString:
    """Convert any linked scalar/combo value to its exact string form."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {"required": {"value": ("*", {"forceInput": True})}}

    RETURN_TYPES = ("STRING",)
    RETURN_NAMES = ("value",)
    FUNCTION = "convert"
    CATEGORY = "saya/util"

    def convert(self, value: Any) -> tuple[str]:
        if value is None:
            raise SayaCoupleReconstructError("SayaValueToString: value is missing")
        return (str(value),)


class SayaLazyBooleanSelect:
    """Lazy generic boolean selector. Only the selected upstream executes."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "switch": ("BOOLEAN", {"forceInput": True}),
                "on_false": ("*", {"lazy": True}),
                "on_true": ("*", {"lazy": True}),
            }
        }

    RETURN_TYPES = ("*",)
    RETURN_NAMES = ("value",)
    FUNCTION = "select"
    CATEGORY = "saya/couple"

    @classmethod
    def check_lazy_status(cls, switch: bool, on_false: Any = None, on_true: Any = None) -> list[str]:
        needed = "on_true" if bool(switch) else "on_false"
        value = on_true if bool(switch) else on_false
        return [needed] if value is None else []

    def select(self, switch: bool, on_false: Any = None, on_true: Any = None) -> tuple[Any]:
        return (on_true if bool(switch) else on_false,)


# ---------------------------------------------------------------------------
# Strengths — copy-on-write, PPM channel only (model_patched_multimask carries
# its strength on the ConditioningSetMask mask_strength instead, applied
# inline by apply_multimask_couple — never through this function).
# ---------------------------------------------------------------------------

def write_strength(cond: Any, strength: float) -> Any:
    """Return a COPY of the conditioning with ``strength`` set on ``cond[0][1]``.

    * PPM channel ONLY — NEVER writes ``mask_strength`` (Phase 1 / MultiMaskCouple);
    * copy-on-write: the tensor is shared (never mutated), the metadata dict
      is copied — no contamination of other branches;
    * multi-entry conditioning: REFUSED (the PPM vendor only reads
      ``cond[0]``, ``unet_couple.py:29-32`` — writing elsewhere would be a
      misleading silence).
    """
    if not isinstance(cond, list) or not cond:
        raise SayaCoupleReconstructError(
            f"strength: expected a CONDITIONING (non-empty list), got {type(cond).__name__}"
        )
    if len(cond) != 1:
        raise SayaCoupleReconstructError(
            f"strength: multi-entry conditioning is not supported by the PPM "
            f"channel ({len(cond)} entries; the vendor only reads cond[0][1]) — refused"
        )
    entry = cond[0]
    if not isinstance(entry, (list, tuple)) or len(entry) < 2 or not isinstance(entry[1], dict):
        raise SayaCoupleReconstructError(
            "strength: expected [tensor, dict] structure on cond[0] — refused"
        )
    tensor, opts = entry[0], entry[1]
    new_opts = dict(opts)                     # copy-on-write
    new_opts["strength"] = float(strength)    # PPM key (unet_couple.py:31-32)
    return [[tensor, new_opts]]


# ---------------------------------------------------------------------------
# Structural provenance guard
# ---------------------------------------------------------------------------

class ProvenanceLedger:
    """Log of (imprint field, encoder call index) per cond."""

    def __init__(self) -> None:
        self.entries: list[tuple[str, int]] = []

    def record(self, field_name: str, encoder_call_index: int) -> None:
        self.entries.append((field_name, int(encoder_call_index)))

    def assert_distinct(self) -> None:
        fields = [f for f, _ in self.entries]
        calls = [c for _, c in self.entries]
        if len(set(fields)) != len(fields):
            raise SayaCoupleReconstructError(
                f"structural guard: duplicated imprint field among {fields} — "
                "each cond must come from its OWN field"
            )
        if len(set(calls)) != len(calls):
            raise SayaCoupleReconstructError(
                f"structural guard: duplicated encoder call index among {calls} — "
                "each cond must come from its OWN encoder call"
            )

    def lines(self) -> list[str]:
        return [f"encode[{call}] <- imprint.{field}" for field, call in self.entries]


def _metadata_dict_of(cond: Any) -> dict[str, Any] | None:
    """Metadata dict of a conditioning's entry [0], or None on an odd structure."""
    if isinstance(cond, list) and cond and isinstance(cond[0], (list, tuple)) and len(cond[0]) > 1:
        opts = cond[0][1]
        if isinstance(opts, dict):
            return opts
    return None


def assert_distinct_objects(conds: dict[str, Any]) -> None:
    """Non-aliasing of the cond objects AND their metadata dicts."""
    names = sorted(conds)
    for i, a in enumerate(names):
        for b in names[i + 1:]:
            cond_a, cond_b = conds[a], conds[b]
            if cond_a is cond_b:
                raise SayaCoupleReconstructError(
                    f"structural guard: {a} and {b} share the SAME conditioning "
                    "object (structurally distinct provenance is required)"
                )
            opts_a, opts_b = _metadata_dict_of(cond_a), _metadata_dict_of(cond_b)
            if opts_a is not None and opts_a is opts_b:
                raise SayaCoupleReconstructError(
                    f"structural guard: {a} and {b} share the SAME metadata dict "
                    "(aliasing)"
                )


def content_warnings(conds: dict[str, str], texts: dict[str, str]) -> list[str]:
    """Content inequality is only a WARNING (intentional mirrors/twins)."""
    warnings: list[str] = []
    names = sorted(conds)
    for i, a in enumerate(names):
        for b in names[i + 1:]:
            if texts.get(a) == texts.get(b):
                warnings.append(
                    f"WARNING content: {a} and {b} have the same text (legitimate "
                    "if intentional — the guard is STRUCTURAL, not content-based)"
                )
    return warnings


# ---------------------------------------------------------------------------
# PNG chunk read (legacy channel kept — SayaCoupleImprintLoad)
# ---------------------------------------------------------------------------

def read_imprint_from_png(checkpoint_path: str) -> dict[str, Any]:
    """Extract ``saya_couple_imprint`` from the valid checkpoint's PNG metadata.

    An IMAGE tensor does not carry PNG chunks: the explicit provenance is
    the PATH of the valid checkpoint (the ``checkpoint_path`` output of
    ``SayaImagePhase2Load``). Absence is a HARD ERROR.
    """
    if not checkpoint_path or not isinstance(checkpoint_path, str):
        raise SayaCoupleReconstructError(
            "imprint load: empty checkpoint path — cannot verify (no fallback)"
        )
    from ..services.imprint_integrity import read_png_info
    try:
        metadata = read_png_info(checkpoint_path)
    except FileNotFoundError:
        raise SayaCoupleReconstructError(
            f"imprint load: checkpoint not found: {checkpoint_path!r}"
        ) from None
    payload = metadata.get(IMPRINT_METADATA_KEY)
    if payload is None:
        raise SayaCoupleReconstructError(
            f"imprint load: PNG chunk {IMPRINT_METADATA_KEY!r} ABSENT from "
            f"{checkpoint_path!r} — cannot verify (no silent fallback)"
        )
    return parse_imprint_json(payload)


# ---------------------------------------------------------------------------
# ComfyUI nodes
# ---------------------------------------------------------------------------

class SayaCoupleImprintLoad:
    """Read the v2 imprint from the valid checkpoint -> SAYA_IMPRINT.

    STANDALONE node: does not extend SayaImagePhase2Load's slots (already
    stale serialized schemas — 7-8 slots vs. 6 in the code). Explicit
    provenance = the PATH of the valid checkpoint (the IMAGE tensor does not
    carry a PNG chunk). Optional ``imprint_json`` input = direct channel
    (tests, documented replays).
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "checkpoint_path": ("STRING", {
                    "default": "", "multiline": False, "forceInput": True,
                    "tooltip": "Path of the VALID Phase 1 checkpoint (checkpoint_path "
                               "output of SayaImagePhase2Load). The PNG chunk "
                               "'saya_couple_imprint' is read from it.",
                }),
            },
            "optional": {
                "imprint_json": ("STRING", {
                    "default": "", "multiline": True, "forceInput": True,
                    "tooltip": "Direct channel (override): canonical v2 JSON. "
                               "Connect ONE OR THE OTHER, never both.",
                }),
            },
        }

    RETURN_TYPES = ("SAYA_IMPRINT", "STRING")
    RETURN_NAMES = ("imprint", "imprint_json")
    FUNCTION = "load"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Reads the 'saya_couple_imprint' PNG chunk from the valid checkpoint "
        "and validates the v2 schema (v1 = explicit error, no silent migration)."
    )

    def load(self, checkpoint_path: str = "", imprint_json: str | None = None) -> tuple[dict[str, Any], str]:
        has_json = bool(imprint_json)
        has_path = bool(checkpoint_path)
        if has_json and has_path:
            raise SayaCoupleReconstructError(
                "imprint load: both checkpoint_path AND imprint_json connected — "
                "exactly one source (no ambiguity)"
            )
        data = parse_imprint_json(imprint_json) if has_json else read_imprint_from_png(checkpoint_path)
        return data, canonical_imprint_json(data)


class SayaCoupleReconstruct:
    """Complete Phase 02 reconstruction — a single couple patch per execution.

    Inputs: imprint (SAYA_IMPRINT) OR imprint_json (STRING) — one of the two;
    model MODEL (phase hub, e.g. GET saya_base_main_model); clip CLIP (GET
    saya_base_clip); checkpoint_identities STRING (JSON {role, identifier,
    source} wired from the hub's REAL widgets); image IMAGE (current pass:
    the geometry re-derivation grid).

    Outputs: model_patched, positive (global MAIN), negative, report.
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "model": ("MODEL",),
                "clip": ("CLIP",),
                "image": ("IMAGE", {
                    "tooltip": "Image of the CURRENT pass: fixes the geometry "
                               "re-derivation resolution (never the imprint's "
                               "reference_size).",
                }),
            },
            "optional": {
                # Historic inputs (unchanged for saved workflows): identities + imprint OR imprint_json.
                # Since P-A (2026-10-05) a SAYA_COUPLE_CONTEXT replaces all three (one parse per phase).
                "checkpoint_identities": ("STRING", {
                    "default": "", "multiline": True, "forceInput": True,
                    "tooltip": "Canonical JSON: list of {role, identifier, source} "
                               "from the checkpoint hub's REAL widgets. "
                               "Mismatch/absence = HARD ERROR. models_used is "
                               "NEVER consulted. Not needed with a context.",
                }),
                "imprint": ("SAYA_IMPRINT", {"forceInput": True}),
                "imprint_json": ("STRING", {
                    "default": "", "multiline": True, "forceInput": True,
                    "tooltip": "Alternative to imprint: canonical v2 JSON. "
                               "Connect ONE OR THE OTHER.",
                }),
                "solo": ("BOOLEAN", {
                    "default": False,
                    "tooltip": "OFF (default) = Couple, unchanged behavior. ON = SOLO: "
                               "MAIN + PERSON 1 only, person_2 ignored entirely, no "
                               "attention-couple patch, model returned unpatched.",
                }),
                "context": ("SAYA_COUPLE_CONTEXT", {
                    "tooltip": "SayaCoupleContextLoad output for THIS model: identities + imprint already validated "
                               "and targeted. Connect it INSTEAD of checkpoint_identities / imprint / imprint_json.",
                }),
            },
        }

    # model_patched_multimask is APPENDED last (not inserted after
    # model_patched): existing saved workflows resolve outputs by slot
    # index, so model_patched(0)/positive(1)/negative(2)/verification_report(3)
    # MUST keep their original indices — only slot 4 is new.
    RETURN_TYPES = ("MODEL", "CONDITIONING", "CONDITIONING", "STRING", "MODEL")
    RETURN_NAMES = ("model_patched", "positive", "negative", "verification_report", "model_patched_multimask")
    FUNCTION = "reconstruct"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "identities (explicit hub, exact equality) -> re-encode MAIN/P1/P2/NEG "
        "-> structural guard -> TWO independent patches from the same "
        "conditioning: model_patched (PPM, crop/tile-aware -- USDU, "
        "Detailers) and model_patched_multimask (the SAME MultiMaskCouple "
        "core Phase 1 Sampler 2 uses -- full-frame passes only: Hires Fix, Phase 6). "
        "Geometry re-derived at the current resolution. No LoRA is applied "
        "here (structural inheritance from the hub)."
    )

    @staticmethod
    def _image_grid(image: Any) -> tuple[int, int, int]:
        shape = getattr(image, "shape", None)
        if shape is None or len(shape) != 4:
            raise SayaCoupleReconstructError(
                f"image: expected a 4D tensor [B,H,W,C], got {shape!r}"
            )
        batch, height, width, _channels = (int(size) for size in shape)
        if batch < 1 or height < 1 or width < 1:
            raise SayaCoupleReconstructError(
                f"image: empty dimension {tuple(shape)}"
            )
        return batch, height, width

    def reconstruct(
        self,
        model: Any,
        clip: Any,
        image: Any,
        checkpoint_identities: str = "",
        imprint: dict[str, Any] | None = None,
        imprint_json: str | None = None,
        solo: bool = False,
        context: Any = None,
    ) -> tuple[Any, Any, Any, str, Any]:
        report: list[str] = [f"SayaCoupleReconstruct — schema v{SCHEMA_VERSION}"]

        if context is not None:
            # -- P-A: everything comes from the context, parsed / validated / identity-checked once per phase --
            if imprint is not None or imprint_json or checkpoint_identities:
                raise SayaCoupleReconstructError(
                    "context: connect EITHER a SAYA_COUPLE_CONTEXT OR the historic inputs "
                    "(checkpoint_identities + imprint / imprint_json), never both"
                )
            from .couple_context import validate_context
            validate_context(context, "context")
            data = context["imprint"]
            couple = data["couple_imprint"]
            recipe = data["reconstruction_recipe"]
            hub = context["identities"]
            report.append(f"imprint: from context (target {context['target']}, validated once by SayaCoupleContextLoad)")
            report.extend(f"{kind}: OK — {recipe[kind]['role']} == {recipe[kind]['identifier']}" for kind in ("checkpoint_identity", "clip_identity"))
            report.append("models_used: NOT consulted (hand-maintained widget)")
        else:
            # -- strict imprint read (historic path) --------------------------------
            if (imprint is None) == (not imprint_json):
                raise SayaCoupleReconstructError(
                    "imprint: connect EXACTLY ONE source (SAYA_IMPRINT or imprint_json), or a context"
                )
            try:
                data = parse_imprint_json(imprint_json) if imprint_json else validate_imprint_v2(imprint)
            except SayaCoupleImprintError as error:
                raise SayaCoupleReconstructError(f"imprint: {error}") from error
            couple = data["couple_imprint"]
            recipe = data["reconstruction_recipe"]
            report.append("imprint: v2 OK (data-only, geometry coherent)")

            # -- hub identities (MUST NOT consult models_used — no code path here) --
            # Active identically in Couple AND Solo: the hard identity check does not
            # depend on person_2 being present.
            hub = parse_hub_identities(checkpoint_identities)
            try:
                report.extend(verify_recipe_identities(recipe, hub))
            except SayaCoupleReconstructError as error:
                raise SayaCoupleReconstructError(f"identities: {error}") from error
            report.append("models_used: NOT consulted (hand-maintained widget)")

        if solo:
            return self._reconstruct_solo(model, clip, hub, data, report)

        # -- re-encode 4 roles, provenance logged -----------------------------
        prompts = couple["prompts"]
        roles = [("main", scene_text(prompts)), ("person_1", prompts["person_1"])]
        if "person_2" in prompts:
            roles.append(("person_2", prompts["person_2"]))
        else:
            raise SayaCoupleReconstructError(
                "person_2: ABSENT from the imprint — the v1 couple patch requires "
                "two regions; the P2-absent case is unsupported, explicitly refused"
            )
        roles.append(("negative", prompts["negative"]))
        conds: dict[str, Any] = {}
        texts: dict[str, str] = {}
        ledger = ProvenanceLedger()
        # Cache-first: key = hub CLIP file + LoRAs actually applied to the CLIP + exact text.
        # A modified prompt only re-encodes its own conditioning; a modified LoRA re-encodes all of them.
        base_clip = next(entry["identifier"] for entry in hub if entry["role"] == "base_clip")
        clip_key = cache.cache_key(base_clip, cache.clip_lora_fingerprint(clip))
        encoded_any = False
        for call_index, (role, text) in enumerate(roles):
            key = cache.cache_key(clip_key, text)
            conds[role] = cache.load(FAMILY, key)
            if conds[role] is None:
                conds[role] = _encode_text(clip, text)
                cache.save(FAMILY, key, conds[role])
                encoded_any = True
            texts[role] = text
            ledger.record(role, call_index)
        # Scene-only MAIN (no ACTION) for the background cells of the Sampler 1 map.
        scene_cond = None
        if any(OWNERSHIP_BACKGROUND in row for row in couple.get("ownership_map", {}).get("rows", [])):
            if prompts.get("action", "").strip():
                key = cache.cache_key(clip_key, prompts["main"])
                scene_cond = cache.load(FAMILY, key)
                if scene_cond is None:
                    scene_cond = _encode_text(clip, prompts["main"])
                    cache.save(FAMILY, key, scene_cond)
                    encoded_any = True
            else:
                scene_cond = conds["main"]
        if encoded_any:
            cache.release_clip(clip)
        ledger.assert_distinct()
        assert_distinct_objects(conds)
        report.append("re-encode: " + "; ".join(ledger.lines()))
        report.extend(content_warnings(conds, texts))

        # -- strengths: written for BOTH engines (P1/P2 only; MAIN/NEG none) ----
        strengths = couple["strengths"]
        base_cond = conds["main"]
        negative = conds["negative"]
        ppm_person_1 = write_strength(conds["person_1"], strengths["strength_1"])
        ppm_person_2 = write_strength(conds["person_2"], strengths["strength_2"])
        report.append(
            "strengths: PPM channel cond[0][1] (model_patched) + MultiMaskCouple "
            f"mask_strength (model_patched_multimask) — P1={strengths['strength_1']} "
            f"P2={strengths['strength_2']} (MAIN/NEG have no regional strength)"
        )

        # -- two independent patches, anti-double-hook guard before either -----
        model_options = getattr(model, "model_options", None)
        if not isinstance(model_options, dict):
            raise SayaCoupleReconstructError(
                "model: expected a MODEL (ModelPatcher with model_options)"
            )
        detect_existing_couple_hook(model_options)
        batch, height, width = self._image_grid(image)
        base_weight, person_weight = attention_weights(data)
        background = derive_background_mask(data, height, width, batch=batch)
        if background is not None:
            report.append(f"ownership_map: {float(background[0].mean()):.1%} background -> scene-only MAIN region (no ACTION, no person)")
        elif couple.get("ownership_map") is not None:
            report.append("ownership_map: P1/P2 cells applied, no background cell")
        scene_kwargs = {} if background is None else {"scene": scene_cond, "background_mask": background}

        # -- PPM (crop/tile-aware): weight baked into the mask amplitude -------
        try:
            ppm_masks = derive_masks(data, height, width, batch=batch,
                                     base_weight=base_weight, person_weight=person_weight,
                                     background=background is not None)
        except SayaMaskError as error:
            raise SayaCoupleReconstructError(f"geometry: {error}") from error
        if ppm_masks.person_2 is None:  # pragma: no cover — P2 absent refused upstream
            raise SayaCoupleReconstructError("mask_2 absent — internal incoherence")
        patched_ppm = _apply_couple_patch(
            model, base_cond, ppm_masks.base,
            ppm_person_1, ppm_masks.person_1,
            ppm_person_2, ppm_masks.person_2,
            *((scene_cond, ppm_masks.scene) if ppm_masks.scene is not None else ()),
        )
        mark_couple_patch(patched_ppm, family="attn2_patch/attn2_output_patch (PPM)")
        report.append(
            f"patch model_patched: 1 SayaAttentionCouplePPM call (crop-aware, "
            f"reads saya_couple_crop at runtime); sentinel {COUPLE_PATCH_SENTINEL!r} set"
        )

        # -- MultiMaskCouple (full-frame only): weight on mask_strength --------
        try:
            raw_mask_1, raw_mask_2 = derive_raw_region_masks(data, height, width, batch=batch,
                                                             background=background is not None)
        except SayaMaskError as error:
            raise SayaCoupleReconstructError(f"geometry: {error}") from error
        if raw_mask_2 is None:  # pragma: no cover — P2 absent refused upstream
            raise SayaCoupleReconstructError("mask_2 absent — internal incoherence")
        patched_multimask = _apply_multimask_patch(
            model, clip, raw_mask_1, raw_mask_2,
            conds["person_1"], negative, conds["person_2"], negative,
            strengths["strength_1"], strengths["strength_2"],
            base_weight, person_weight, base_cond, **scene_kwargs,
        )
        mark_couple_patch(patched_multimask, family="patches_replace.attn2 (MultiMaskCouple)")
        report.append(
            "patch model_patched_multimask: 1 MultiMaskCouple AttentionCouple call "
            "(custom_nodes.MultiMaskCouple.attention_couple — same algorithm as "
            "Phase 1 Sampler 2; FULL-FRAME ONLY, no crop-awareness); "
            f"base_weight={base_weight} person_weight={person_weight}; "
            f"sentinel {COUPLE_PATCH_SENTINEL!r} set"
        )
        report.append(
            f"geometry re-derived: {width}x{height} px (current image), "
            f"orientation {couple['geometry']['derived']['orientation']}, "
            f"masks ({batch},{height},{width})"
        )
        return patched_ppm, base_cond, negative, "\n".join(report), patched_multimask

    def _reconstruct_solo(
        self, model: Any, clip: Any, hub: list[dict[str, str]],
        data: dict[str, Any], report: list[str],
    ) -> tuple[Any, Any, Any, str, Any]:
        """SOLO path (Couple Mode OFF): MAIN + PERSON 1 only, person_2 ignored entirely.

        No mask derivation, no attention-couple patch — the model is returned
        UNPATCHED (there is no second region to isolate). Person 1 has no
        regional hook to carry it, so it is folded directly into the single
        returned positive prompt text (MAIN, then PERSON 1) and re-encoded as
        one ordinary conditioning — this is what actually makes Person 1
        contribute in solo mode, not a no-op strength-zeroing of the couple path.
        person_2, if present in the imprint, is read nowhere below.
        """
        couple = data["couple_imprint"]
        prompts = couple["prompts"]
        main_text = scene_text(prompts)
        person_1_text = prompts["person_1"]
        negative_text = prompts["negative"]
        solo_text = f"{main_text}, {person_1_text}" if person_1_text else main_text

        base_clip = next(entry["identifier"] for entry in hub if entry["role"] == "base_clip")
        clip_key = cache.cache_key(base_clip, cache.clip_lora_fingerprint(clip))
        encoded_any = False
        conds: dict[str, Any] = {}
        for role, text in (("solo_main_person1", solo_text), ("negative", negative_text)):
            key = cache.cache_key(clip_key, text)
            conds[role] = cache.load(FAMILY, key)
            if conds[role] is None:
                conds[role] = _encode_text(clip, text)
                cache.save(FAMILY, key, conds[role])
                encoded_any = True
        if encoded_any:
            cache.release_clip(clip)
        assert_distinct_objects(conds)

        report.append(
            "SOLO re-encode: MAIN+PERSON_1 as one conditioning "
            f"({solo_text!r}); person_2 not read"
        )
        report.append("SOLO patch: no attention-couple patch — model returned unpatched")
        return model, conds["solo_main_person1"], conds["negative"], "\n".join(report), model


class SayaLazyModelSelect:
    """Choose between a Model 1 and a Model 2 value without ever evaluating the unpicked one.

    Uses ComfyUI's native lazy-input contract (``check_lazy_status``): the
    executor only computes an input the returned list actually names, so the
    unpicked branch's whole upstream (reconstruct, attention-couple patch,
    checkpoint load, CLIP encode, everything) never runs. This is the atomic
    decision point a Model 1/2 hub wires its visible dropdown into; the
    branch producers themselves (SayaCoupleReconstruct,
    SayaCoupleImprintRetarget, etc.) live upstream, outside this node, and
    this node makes no decision of its own beyond which already-built branch
    to pass through.

    Untyped (``*``) on purpose: a hub that must keep MODEL + CLIP +
    checkpoint_identities mutually consistent (Phase 6) uses one instance of
    this node per value, all reading the same ``choice`` — one visible
    dropdown, several atomically-synchronized lazy selects.
    """

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "choice": ("STRING", {
                    "default": "Model 1", "forceInput": True,
                    "tooltip": "'Model 1' or 'Model 2', fed from a visible CustomCombo "
                               "dropdown upstream. Which branch to evaluate — the other "
                               "branch's upstream is never executed (lazy).",
                }),
                "model_1": ("*", {"lazy": True}),
                "model_2": ("*", {"lazy": True}),
            },
        }

    RETURN_TYPES = ("*",)
    RETURN_NAMES = ("value",)
    FUNCTION = "select"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Lazily selects the Model 1 or Model 2 value: only the chosen branch's "
        "upstream graph is ever evaluated."
    )

    @classmethod
    def check_lazy_status(
        cls, choice: str, model_1: Any = None, model_2: Any = None,
    ) -> list[str]:
        needed = []
        if choice == "Model 1" and model_1 is None:
            needed.append("model_1")
        if choice == "Model 2" and model_2 is None:
            needed.append("model_2")
        return needed

    def select(self, choice: str, model_1: Any = None, model_2: Any = None) -> tuple[Any]:
        if choice not in ("Model 1", "Model 2"):
            raise SayaCoupleReconstructError(f"SayaLazyModelSelect: invalid choice {choice!r}")
        return (model_1 if choice == "Model 1" else model_2,)

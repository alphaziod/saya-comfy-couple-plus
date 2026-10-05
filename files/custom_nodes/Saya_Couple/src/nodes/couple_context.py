"""SAYA_COUPLE_CONTEXT: one transport object per phase for everything the couple reconstruction needs.

Audit Fable P-A (2026-10-05, validated by Saya): the chains ``SayaCoupleImprintResolve`` / ``SayaCoupleImprintLoad``
-> ``SayaCoupleCheckpointIdentities`` (x2) -> ``SayaCoupleImprintRetarget`` -> ``SayaCoupleReconstruct`` (x2) re-parsed
and re-validated the same imprint in every node. ``SayaCoupleContextLoad`` does that work ONCE per phase and hands
each consumer a plain dict (no class, no behaviour: a transport object):

    {"schema": "saya.couple.context/1", "target": "model_1" | "model_2",
     "imprint": <validated v2 imprint, retargeted to this context's model>, "imprint_json": <canonical>,
     "identities": [{role, identifier, source}, ...] (phase_model, base_clip, model_2),
     "source": {"checkpoint_path", "chain": [...], "root", "mode"},      # where the imprint came from
     "retarget": {"applied": bool, "checkpoint_identity": {"from", "to"}, "clip_identity": {"from", "to"}},
     "phase": <phase of the loaded checkpoint, if known>, "debug": [str, ...]}

The historic nodes and inputs keep working (old workflows load unchanged); ``SayaCoupleReconstruct`` accepts either
the historic inputs or a context. ``SayaCoupleContextInspect`` prints a context for debugging without touching the
engine. The rendering is unchanged: a context carries exactly what the historic chain produced (tests prove the
equality).
"""

from __future__ import annotations

import copy
import json
from typing import Any

from .couple_imprint_resolve import resolve_imprint
from .couple_imprint_v2 import (
    ROLE_BASE_CLIP,
    ROLE_MODEL_2,
    ROLE_PHASE_MODEL,
    SayaCoupleImprintError,
    build_identity,
    canonical_imprint_json,
    parse_imprint_json,
    validate_imprint_v2,
)
from .couple_reconstruct import SayaCoupleReconstructError, parse_hub_identities, verify_recipe_identities

CONTEXT_SCHEMA = "saya.couple.context/1"
TARGETS = ("model_1", "model_2")


class SayaCoupleContextError(ValueError):
    pass


def identities_for(phase_model: str, base_clip: str, model_2: str | None) -> list[dict[str, str]]:
    """The hub identity list exactly as SayaCoupleCheckpointIdentities builds it."""
    entries = [build_identity(role=ROLE_PHASE_MODEL, identifier=phase_model), build_identity(role=ROLE_BASE_CLIP, identifier=base_clip)]
    if model_2:
        entries.append(build_identity(role=ROLE_MODEL_2, identifier=model_2))
    return entries


def retarget_imprint(data: dict[str, Any], hub: list[dict[str, str]]) -> tuple[dict[str, Any], dict[str, Any]]:
    """A copy of the imprint whose recipe identities are the hub's phase_model / base_clip (what
    SayaCoupleImprintRetarget does), plus what changed. A hub equal to the imprint's identities = no change."""
    derived = copy.deepcopy(data)
    recipe = derived["reconstruction_recipe"]
    changes: dict[str, Any] = {"applied": False}
    for kind, role in (("checkpoint_identity", ROLE_PHASE_MODEL), ("clip_identity", ROLE_BASE_CLIP)):
        entry = next((h for h in hub if h["role"] == role), None)
        if entry is None:
            raise SayaCoupleContextError(f"context: identities have no role {role!r}, cannot target the imprint")
        before = recipe.get(kind)
        after = build_identity(role=role, identifier=entry["identifier"], source=entry["source"])
        changes[kind] = {"from": (before or {}).get("identifier"), "to": after["identifier"]}
        if before != after:
            changes["applied"] = True
        recipe[kind] = after
    validate_imprint_v2(derived, context="imprint(context)")
    return derived, changes


def build_context(target: str, data: dict[str, Any], hub: list[dict[str, str]], source: dict[str, Any], phase: Any = None,
                  debug: list[str] | None = None, retarget: bool = False) -> dict[str, Any]:
    """``retarget`` = False: the imprint is used as is and its identities MUST equal the hub's (the historic hard check
    of SayaCoupleReconstruct for the model that made Phase 1); True: the imprint is retargeted to the hub (what
    SayaCoupleImprintRetarget did for MODEL 2 and for a routed model)."""
    if target not in TARGETS:
        raise SayaCoupleContextError(f"context: target {target!r}, expected one of {TARGETS}")
    if retarget:
        imprint, changes = retarget_imprint(data, hub)
    else:
        imprint = copy.deepcopy(data)
        recipe = imprint["reconstruction_recipe"]
        changes = {"applied": False, **{kind: {"from": recipe[kind]["identifier"], "to": recipe[kind]["identifier"]} for kind in ("checkpoint_identity", "clip_identity")}}
    verify_recipe_identities(imprint["reconstruction_recipe"], hub)  # the hard identity check, once, here
    return {"schema": CONTEXT_SCHEMA, "target": target, "imprint": imprint, "imprint_json": canonical_imprint_json(imprint),
            "identities": [dict(h) for h in hub], "source": dict(source), "retarget": changes, "phase": phase, "debug": list(debug or [])}


def validate_context(context: Any, where: str = "context") -> dict[str, Any]:
    """Light structural check of a transported context (it was fully validated when built)."""
    if not isinstance(context, dict) or context.get("schema") != CONTEXT_SCHEMA:
        raise SayaCoupleContextError(f"{where}: expected a SAYA_COUPLE_CONTEXT ({CONTEXT_SCHEMA}), got {type(context).__name__}")
    if context.get("target") not in TARGETS or not isinstance(context.get("imprint"), dict) or not isinstance(context.get("identities"), list):
        raise SayaCoupleContextError(f"{where}: malformed context (target / imprint / identities)")
    return context


def describe_context(context: dict[str, Any]) -> str:
    """Human-readable dump: what a phase will reconstruct from, and where it came from."""
    validate_context(context)
    couple = context["imprint"].get("couple_imprint", {})
    prompts = couple.get("prompts", {})
    geometry = couple.get("geometry", {})
    ownership = couple.get("ownership_map")
    lines = [f"SAYA_COUPLE_CONTEXT {context['schema']} — target {context['target']} (phase {context.get('phase')})",
             "identities: " + ", ".join(f"{h['role']}={h['identifier']}" for h in context["identities"]),
             "retarget: " + ("applied " + ", ".join(f"{k} {v['from']} -> {v['to']}" for k, v in context["retarget"].items() if isinstance(v, dict))
                              if context["retarget"].get("applied") else "none (imprint identities already match)"),
             f"source: {context['source'].get('mode')} {context['source'].get('checkpoint_path')}",
             "chain: " + " -> ".join(context["source"].get("chain", [])),
             "prompts: " + ", ".join(f"{k} ({len(str(v))} chars)" for k, v in prompts.items()),
             f"geometry: {json.dumps(geometry, ensure_ascii=False)[:200]}"]
    if ownership:
        cells = "".join(ownership.get("rows", []))
        share = {code: round(cells.count(code) / max(len(cells), 1), 3) for code in "12bs"}
        lines.append(f"ownership_map: grid {ownership.get('grid')} P1 {share['1']} P2 {share['2']} background {share['b']} static {share['s']}")
    else:
        lines.append("ownership_map: none (static split)")
    lines += [f"debug: {d}" for d in context.get("debug", [])]
    return "\n".join(lines)


class SayaCoupleContextLoad:
    """Checkpoint path + hub identifiers -> one context per model, the imprint parsed and validated once."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "checkpoint_path": ("STRING", {"default": "", "multiline": False, "forceInput": True,
                                               "tooltip": "checkpoint_path of the phase LOAD node: the source_file chain is walked up to the "
                                                          "Phase 1 checkpoint carrying the imprint (a Phase 1 checkpoint works directly)."}),
            },
            "optional": {
                "model_1_identifier": ("STRING", {"forceInput": True, "tooltip": "Checkpoint identifier of MODEL 1 (hub widget string). context_model_1 targets it."}),
                "model_2_identifier": ("STRING", {"forceInput": True, "tooltip": "Checkpoint identifier of MODEL 2. context_model_2 targets it (None when absent)."}),
                "identities_json": ("STRING", {"default": "", "multiline": True, "forceInput": True,
                                               "tooltip": "Alternative to model_1_identifier: a ready hub list {role, identifier, source} (e.g. the "
                                                          "'Selected Identities' of a model routing hub). context_model_1 targets its phase_model."}),
                "clip_1_identifier": ("STRING", {"forceInput": True, "tooltip": "CLIP identifier for MODEL 1; default = model_1_identifier."}),
                "clip_2_identifier": ("STRING", {"forceInput": True, "tooltip": "CLIP identifier for MODEL 2; default = model_2_identifier."}),
                "imprint_json": ("STRING", {"default": "", "multiline": True, "forceInput": True,
                                            "tooltip": "Direct channel (tests, replays): canonical v2 JSON instead of the checkpoint chain."}),
            },
        }

    RETURN_TYPES = ("SAYA_COUPLE_CONTEXT", "SAYA_COUPLE_CONTEXT", "SAYA_IMPRINT", "STRING", "STRING")
    RETURN_NAMES = ("context_model_1", "context_model_2", "imprint", "imprint_json", "report")
    FUNCTION = "load"
    CATEGORY = "saya/couple"
    DESCRIPTION = ("One node per phase: resolves the imprint from the checkpoint chain, validates it once, and hands "
                   "SayaCoupleReconstruct a ready context per model (identities + imprint targeted to that model). "
                   "Replaces Resolve/Load + CheckpointIdentities + Retarget. imprint / imprint_json stay available for other nodes.")

    def load(self, checkpoint_path: str = "", model_1_identifier: str | None = None, model_2_identifier: str | None = None,
             identities_json: str | None = None, clip_1_identifier: str | None = None, clip_2_identifier: str | None = None,
             imprint_json: str | None = None):
        if bool(model_1_identifier) == bool(identities_json):
            raise SayaCoupleContextError("context load: wire EXACTLY ONE of model_1_identifier (hub string) or identities_json (hub list)")
        if imprint_json and checkpoint_path:
            raise SayaCoupleContextError("context load: both checkpoint_path AND imprint_json connected — exactly one source")
        if imprint_json:
            try:
                data = parse_imprint_json(imprint_json)
            except SayaCoupleImprintError as error:
                raise SayaCoupleContextError(f"context load: imprint_json: {error}") from error
            source = {"mode": "imprint_json", "checkpoint_path": "", "chain": [], "root": ""}
            phase = None
        else:
            result = resolve_imprint(checkpoint_path)
            data = result.imprint
            source = {"mode": "resolve", "checkpoint_path": checkpoint_path, "chain": list(result.chain), "root": result.root}
            phase = _phase_of(checkpoint_path)
        if identities_json:
            # A routed hub (e.g. Phase 6 "Selected Identities"): retargeted, exactly like the historic Retarget node.
            hub_1 = parse_hub_identities(identities_json)
            model_1_identifier = next(h["identifier"] for h in hub_1 if h["role"] == ROLE_PHASE_MODEL)
            context_1 = build_context("model_1", data, hub_1, source, phase, retarget=True)
        else:
            # MODEL 1 = the checkpoint that made Phase 1: hard equality with the imprint (historic Reconstruct check).
            hub_1 = identities_for(model_1_identifier, clip_1_identifier or model_1_identifier, model_2_identifier or None)
            context_1 = build_context("model_1", data, hub_1, source, phase, retarget=False)
        context_2 = None
        if model_2_identifier:
            hub_2 = identities_for(model_2_identifier, clip_2_identifier or model_2_identifier, model_1_identifier)
            context_2 = build_context("model_2", data, hub_2, source, phase, retarget=True)
        report = [f"context: imprint from {source['mode']} ({source.get('root') or 'direct'}), parsed and validated once",
                  f"model_1: {model_1_identifier}" + (" (retargeted)" if context_1["retarget"]["applied"] else " (imprint identity)"),
                  f"model_2: {model_2_identifier} (retargeted)" if context_2 and context_2["retarget"]["applied"] else f"model_2: {model_2_identifier or 'none'}"]
        return context_1, context_2, data, canonical_imprint_json(data), "\n".join(report)


def _phase_of(checkpoint_path: str) -> Any:
    """Phase number of the loaded checkpoint from its sidecar manifest, if readable (debug information only)."""
    from pathlib import Path
    sidecar = Path(checkpoint_path).with_suffix(".json")
    try:
        manifest = json.loads(sidecar.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    return manifest.get("phase") if isinstance(manifest, dict) else None


class SayaCoupleContextInspect:
    """Debug: the content of a context as text (no engine instrumentation needed)."""

    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {"required": {"context": ("SAYA_COUPLE_CONTEXT",)},
                "optional": {"full_imprint_json": ("BOOLEAN", {"default": False, "tooltip": "Append the full canonical imprint JSON."})}}

    RETURN_TYPES = ("STRING", "STRING")
    RETURN_NAMES = ("summary", "imprint_json")
    FUNCTION = "inspect"
    CATEGORY = "saya/couple"
    OUTPUT_NODE = True
    DESCRIPTION = "Shows what a SAYA_COUPLE_CONTEXT carries: target, identities, retarget, source chain, prompts, geometry, ownership map."

    def inspect(self, context: Any, full_imprint_json: bool = False):
        summary = describe_context(context)
        if full_imprint_json:
            summary += "\n\n" + context["imprint_json"]
        return {"ui": {"text": [summary]}, "result": (summary, context["imprint_json"])}

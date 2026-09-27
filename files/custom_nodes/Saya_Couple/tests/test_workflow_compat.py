"""Compatibility with the workflows that actually use this pack.

Every Saya* class_type referenced by the live workflow files must resolve in
the current registry — otherwise ComfyUI paints the node red on load. Ghost
class_types from the removed pre-2026-09-18 couple system are expected inside
old embedded subgraphs and are NOT failures; they are filtered by checking
each type against the *current* registry and reporting, not asserting.

These two checks are opt-in integration tests against the maintainer's own
saved workflow files (real, historically-accumulated graphs — the whole
point is catching drift against files nobody hand-crafted for the test).
They are NOT part of the pack's autonomous default suite: the normal test
run must not depend on any personal machine's directories. Set
SAYA_TEST_EXTERNAL_WORKFLOWS=1 and list real workflow paths in
SAYA_TEST_WORKFLOW_PATHS (os.pathsep-separated) to opt in locally; with
neither set, both checks report as skipped, not failed.
"""

import json
import os
from pathlib import Path

from harness import Check, load_pack


def _opted_in_workflow_files() -> list[Path] | None:
    """Paths from SAYA_TEST_WORKFLOW_PATHS, or None if the opt-in env var is unset."""
    if os.environ.get("SAYA_TEST_EXTERNAL_WORKFLOWS") != "1":
        return None
    raw = os.environ.get("SAYA_TEST_WORKFLOW_PATHS", "")
    return [Path(p) for p in raw.split(os.pathsep) if p]


def _walk_types(obj, acc):
    if isinstance(obj, dict):
        node_type = obj.get("type") or obj.get("class_type")
        if isinstance(node_type, str) and node_type:
            acc.add(node_type)
        for value in obj.values():
            _walk_types(value, acc)
    elif isinstance(obj, list):
        for value in obj:
            _walk_types(value, acc)


# The actively maintained node families of this pack. Ghost types from the
# removed couple system (SayaDuoGen1, SayaPromptSlot*, ...) may still appear
# in archived subgraphs; the auto-phase backbone must never be one of them.
ACTIVE_TYPES = {
    "SayaImageGenerationReview",
    "SayaAttentionCouplePPM",
    "SayaPPMMasks",
    "SayaDuoSegsDetail",
    "SayaDuoTiledUpscale",
    "SayaDuoLatentShape",
    "SayaResolutionScaleCalculator",
    "SayaUpscalePresetModelLoader",
    "SayaUpscaleTargetCalculator",
    "SayaNaturalizePostProcess",
    "SayaChromaAnchor",
    "SayaHiDreamLoraLoader",
    "SayaHiDreamLoraSettings",
    "SayaHiDreamShadowControlMask",
    "SayaKSamplerConfig",
    *(f"SayaImagePhase{p}{kind}" for p in range(2, 7) for kind in ("Load", "Stop")),
}


def test_workflow_types_resolve():
    c = Check("workflow_types_resolve")
    workflow_files = _opted_in_workflow_files()
    if workflow_files is None:
        c.skip("SAYA_TEST_EXTERNAL_WORKFLOWS not set — opt-in integration check, not part of the autonomous suite")
        return c.report()
    pack = load_pack()
    registry = set(pack.NODE_CLASS_MAPPINGS)
    for path in workflow_files:
        if not path.is_file():
            c.ok(False, f"workflow missing: {path}")
            continue
        types = set()
        _walk_types(json.loads(path.read_text(encoding="utf-8")), types)
        missing = (types & ACTIVE_TYPES) - registry
        c.eq(missing, set(), f"{path.name}: active Saya types missing from registry")
    return c.report()


def test_fixed_phase_widgets_align():
    """Serialized widgets_values of fixed Load/Stop nodes must match INPUT_TYPES order.

    The fixed Load/Stop nodes (SayaImagePhase2Load..SayaImagePhase6Load,
    SayaImagePhase1Stop..SayaImagePhase6Stop) hardcode their phase, detailer,
    and checkpoint_root, so those never appear as widgets; a workflow node
    must carry exactly len(required)+len(optional) serialized values (hidden
    inputs excluded). The tolerance below also accepts graphs saved before
    the current optional widgets existed (up to two short) or with the
    removed trailing unload_after_phase value still present (one extra).
    """
    c = Check("fixed_phase_widgets_align")
    workflow_files = _opted_in_workflow_files()
    if workflow_files is None:
        c.skip("SAYA_TEST_EXTERNAL_WORKFLOWS not set — opt-in integration check, not part of the autonomous suite")
        return c.report()
    pack = load_pack()
    registry = pack.NODE_CLASS_MAPPINGS

    for path in workflow_files[:1]:
        graph = json.loads(path.read_text(encoding="utf-8"))
        for node in graph.get("nodes", []):
            node_type = node.get("type", "")
            if node_type not in registry:
                continue
            if not (node_type.startswith("SayaImagePhase") and node_type.endswith(("Load", "Stop"))):
                continue
            cls = registry[node_type]
            schema = cls.INPUT_TYPES()
            expected = len(schema.get("required", {})) + len(schema.get("optional", {}))
            values = node.get("widgets_values", [])
            # Old graphs saved before the trailing optionals existed can miss
            # up to two of them, and graphs saved with the removed trailing
            # unload_after_phase carry one extra ignored value; anything else
            # is a drift worth flagging.
            c.ok(
                len(values) in (expected + 1, expected, expected - 1, expected - 2),
                f"{path.name} node {node.get('id')} ({node_type}): "
                f"{len(values)} widget values, want {expected + 1}, {expected}, {expected - 1} or {expected - 2}",
            )
    return c.report()


TESTS = (test_workflow_types_resolve, test_fixed_phase_widgets_align)

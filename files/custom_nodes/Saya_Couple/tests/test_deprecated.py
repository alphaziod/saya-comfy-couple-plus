"""P-C: deprecated node types stay loadable and functional, live in saya/deprecated, warn once; originals untouched."""

from __future__ import annotations

import logging

from harness import Check, load_pack


class _Warnings(logging.Handler):
    def __init__(self):
        super().__init__(level=logging.WARNING)
        self.messages = []

    def emit(self, record):
        self.messages.append(record.getMessage())


def test_deprecated_nodes():
    pack = load_pack()
    import saya_couple.registry as registry
    from saya_couple.src.nodes import image_phases, saya_resolution_scale

    c = Check("deprecated_nodes")
    names = list(registry.DEPRECATED)
    c.eq(len(names), 13, "13 deprecated types")
    for name in names:
        cls = pack.NODE_CLASS_MAPPINGS[name]
        original = registry.DEPRECATED[name][0]
        c.ok(issubclass(cls, original) and cls is not original, f"{name}: registered as a subclass of the original")
        c.eq(cls.CATEGORY, "saya/deprecated", f"{name}: category")
        c.ok(cls.DESCRIPTION.startswith("DEPRECATED"), f"{name}: description")
        c.ok(pack.NODE_DISPLAY_NAME_MAPPINGS[name].startswith("[DEPRECATED] "), f"{name}: display name")
        c.eq(cls.INPUT_TYPES(), original.INPUT_TYPES(), f"{name}: same inputs (old workflows load)")
        c.eq((cls.RETURN_TYPES, getattr(cls, "RETURN_NAMES", None)), (original.RETURN_TYPES, getattr(original, "RETURN_NAMES", None)), f"{name}: same outputs")
    # originals untouched: live subclasses / aliases keep their category
    c.ok(image_phases.SayaImagePhaseCheckpointLoad.CATEGORY != "saya/deprecated" and image_phases.SayaImagePhase2Load.CATEGORY != "saya/deprecated",
         "the generic loader (parent of the live Phase N loaders) is not deprecated itself")
    c.ok(pack.NODE_CLASS_MAPPINGS["SayaUpscaleTargetCalculator"].CATEGORY != "saya/deprecated", "SayaUpscaleTargetCalculator (alias target) stays live")
    c.ok(pack.NODE_CLASS_MAPPINGS["SayaNear4KTargetCalculator"] is not pack.NODE_CLASS_MAPPINGS["SayaUpscaleTargetCalculator"], "the alias is deprecated separately")
    # still functional + one warning per type
    handler = _Warnings()
    logging.getLogger().addHandler(handler)
    try:
        gate = pack.NODE_CLASS_MAPPINGS["SayaWarmupGate"]()
        out1 = getattr(gate, gate.FUNCTION)(**_first_args(gate))
        out2 = getattr(gate, gate.FUNCTION)(**_first_args(gate))
    finally:
        logging.getLogger().removeHandler(handler)
    c.ok(out1 is not None and out2 is not None, "deprecated node still runs")
    c.eq(sum("SayaWarmupGate is deprecated" in m for m in handler.messages), 1, "exactly one warning per process")
    return c.report()


def _first_args(node):
    """Minimal valid kwargs for SayaWarmupGate (a LATENT in)."""
    import torch
    schema = node.INPUT_TYPES()
    args = {}
    for name, spec in schema.get("required", {}).items():
        kind = spec[0]
        if kind == "LATENT":
            args[name] = {"samples": torch.zeros(1, 4, 8, 8)}
        elif isinstance(kind, list):
            args[name] = kind[0]
        elif kind in ("INT", "FLOAT"):
            args[name] = spec[1].get("default", 0) if len(spec) > 1 else 0
        elif kind == "BOOLEAN":
            args[name] = spec[1].get("default", False) if len(spec) > 1 else False
        elif kind == "STRING":
            args[name] = spec[1].get("default", "") if len(spec) > 1 else ""
    return args


TESTS = (test_deprecated_nodes,)

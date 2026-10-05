"""Registry contract: every node ComfyUI can load must be fully wired."""

from harness import Check, load_pack


def test_registry_complete():
    pack = load_pack()
    c = Check("registry_complete")
    mappings = pack.NODE_CLASS_MAPPINGS
    displays = pack.NODE_DISPLAY_NAME_MAPPINGS

    c.eq(len(mappings), 59, "node count")
    c.eq(len(set(mappings)), len(mappings), "duplicate class_type keys")
    c.eq(set(displays), set(mappings), "display-name keys != registry keys")
    c.ok(bool(pack.WEB_DIRECTORY), "WEB_DIRECTORY set")

    for name, cls in mappings.items():
        fn = getattr(cls, "FUNCTION", None)
        c.ok(fn is not None and hasattr(cls, fn), f"{name}: FUNCTION {fn!r} missing on class")
        c.ok(getattr(cls, "RETURN_TYPES", None) is not None, f"{name}: RETURN_TYPES missing")
        c.ok(isinstance(getattr(cls, "CATEGORY", None), str), f"{name}: CATEGORY missing")
        schema = cls.INPUT_TYPES()
        c.ok(isinstance(schema, dict), f"{name}: INPUT_TYPES not a dict")
        for section in schema:
            c.ok(section in ("required", "optional", "hidden"), f"{name}: bad INPUT_TYPES section {section!r}")
    return c.report()


def test_fixed_phase_nodes():
    """Every fixed phase 2..6 has a Load and a Stop; phase 1 has a Stop."""
    pack = load_pack()
    c = Check("fixed_phase_nodes")
    m = pack.NODE_CLASS_MAPPINGS
    for phase in range(2, 7):
        c.ok(f"SayaImagePhase{phase}Load" in m, f"phase {phase}: Load missing")
        c.ok(f"SayaImagePhase{phase}Stop" in m, f"phase {phase}: Stop missing")
        load_cls = m.get(f"SayaImagePhase{phase}Load")
        stop_cls = m.get(f"SayaImagePhase{phase}Stop")
        if load_cls is not None:
            c.eq(load_cls.PHASE, phase, f"phase {phase}: Load PHASE attr")
        if stop_cls is not None:
            c.eq(stop_cls.PHASE, phase, f"phase {phase}: Stop PHASE attr")
    c.eq(m["SayaImagePhase1Stop"].PHASE, 1, "phase 1 Stop PHASE attr")
    return c.report()


def test_compat_alias():
    """The Near4K alias must keep resolving for pre-preset workflows."""
    pack = load_pack()
    c = Check("compat_alias")
    m = pack.NODE_CLASS_MAPPINGS
    c.ok("SayaNear4KTargetCalculator" in m, "alias registered")
    c.ok(
        issubclass(m["SayaNear4KTargetCalculator"], m["SayaUpscaleTargetCalculator"]),
        "alias must behave as SayaUpscaleTargetCalculator (deprecated subclass since 2026-10-05)",
    )
    return c.report()


TESTS = (test_registry_complete, test_fixed_phase_nodes, test_compat_alias)

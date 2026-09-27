"""Fixed-phase node behavior: phase label parsing and detailer normalization."""

from harness import Check, load_pack


def _nodes():
    load_pack()
    from saya_couple.src.nodes import image_phases as phase_nodes

    return phase_nodes


def test_parse_phase():
    phase_nodes = _nodes()
    c = Check("parse_phase")
    from saya_couple.src.services.image_phases import parse_phase

    c.eq(parse_phase("2 — HIRES / USDU"), 2, "label with em-dash")
    c.eq(parse_phase("0 — IDLE"), 0, "idle")
    c.raises(ValueError, lambda: parse_phase("9 — NOPE"), "out of range refused")
    c.raises(ValueError, lambda: parse_phase("abc"), "non-numeric refused")
    return c.report()


def test_normalize_detailer():
    c = Check("normalize_detailer")
    from saya_couple.src.services.image_phases import normalize_detailer

    c.eq(normalize_detailer("Face"), "face", "case normalized")
    c.eq(normalize_detailer("full eyes"), "full_eyes", "spaces -> underscores")
    c.eq(normalize_detailer(None), "none", "None -> none")
    c.eq(normalize_detailer("__shared__"), "none", "shared sentinel -> none")
    c.raises(ValueError, lambda: normalize_detailer("elbow"), "unknown detailer refused")
    return c.report()


TESTS = (test_parse_phase, test_normalize_detailer)

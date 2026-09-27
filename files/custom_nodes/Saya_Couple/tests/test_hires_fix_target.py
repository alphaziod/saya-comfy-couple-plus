"""SayaHiresFixTarget: original size read from the Phase 3 manifest, target = original x output_scale, hard errors."""

import json
import tempfile
from pathlib import Path

from PIL import Image, PngImagePlugin

from harness import Check, load_pack


def _mod():
    load_pack()
    from saya_couple.src.nodes import hires_fix_target as m

    return m


def _checkpoint(directory: Path, name: str, size: tuple[int, int], source: str = "") -> Path:
    info = PngImagePlugin.PngInfo()
    info.add_text("saya_phase_manifest", json.dumps({"source_file": source}))
    path = directory / name
    Image.new("RGB", size).save(path, pnginfo=info)
    return path


def test_target_from_manifest():
    m = _mod()
    c = Check("hires_target_from_manifest")
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        phase2 = _checkpoint(d, "phase_2.png", (3000, 4000))
        phase3 = _checkpoint(d, "phase_3.png", (880, 1168), str(phase2))
        c.eq(m.read_original_size(str(phase3)), (3000, 4000), "original size = Phase 2 checkpoint (not the working resolution)")
        ow, oh, tw, th, report = m.SayaHiresFixTarget().compute(str(phase3), 1.0)
        c.eq((ow, oh), (3000, 4000), "original passed through")
        c.ok(tw % 16 == 0 and th % 16 == 0 and abs(tw / th / (3000 / 4000) - 1) < 0.01, f"scale 1.0 -> {tw}x{th}: multiples of 16, original ratio")
        c.ok(abs(tw / 3000 - 1) < 0.01, "scale 1.0: target ~ original")
        c.ok(json.loads(report)["target"] == [tw, th], "report consistent")
    return c.report()


def test_output_scale_examples():
    m = _mod()
    c = Check("hires_target_output_scale")
    c.eq(m.target_size(896, 1152, 1.0), (896, 1152), "896x1152 x1.0 -> exactly the original")
    for scale in (1.2, 1.5, 2.0, 0.5):
        w, h = m.target_size(896, 1152, scale)
        c.ok(w % 16 == 0 and h % 16 == 0, f"x{scale}: multiples of 16 ({w}x{h})")
        c.ok(abs(w / (896 * scale) - 1) < 0.02 and abs(h / (1152 * scale) - 1) < 0.02, f"x{scale}: close to original x scale")
        c.ok(abs(w / h / (896 / 1152) - 1) < 0.01, f"x{scale}: original ratio")
    return c.report()


def test_hard_errors():
    m = _mod()
    c = Check("hires_target_hard_errors")

    def refuses(path, needle, label):
        try:
            m.read_original_size(path)
        except m.SayaHiresFixTargetError as error:
            c.ok(needle in str(error), f"{label}: {error}")
        else:
            c.failures.append(f"{label}: no error")

    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        refuses("", "not found", "empty path")
        refuses(str(d / "absent.png"), "not found", "missing file")
        plain = d / "plain.png"
        Image.new("RGB", (8, 8)).save(plain)
        refuses(str(plain), "missing", "PNG without manifest")
        refuses(str(_checkpoint(d, "nosrc.png", (8, 8), "")), "source_file", "empty source_file")
        refuses(str(_checkpoint(d, "badsrc.png", (8, 8), str(d / "gone.png"))), "source_file", "source_file not found")
    return c.report()


def test_registry():
    pack = load_pack()
    c = Check("hires_target_registry")
    name = "SayaHiresFixTarget"
    c.ok(name in pack.NODE_CLASS_MAPPINGS and name in pack.NODE_DISPLAY_NAME_MAPPINGS, "enregistre")
    c.eq(pack.NODE_CLASS_MAPPINGS[name].RETURN_NAMES, ("original_width", "original_height", "target_width", "target_height", "report"), "outputs (computation only, no model factor)")
    return c.report()


TESTS = (test_target_from_manifest, test_output_scale_examples, test_hard_errors, test_registry)

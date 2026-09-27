"""SayaUpscaleMode: CLASSIC vs neural model, NaN safety, workflow lanes."""

import json
import math
import os
from pathlib import Path

import torch

from harness import Check, load_pack

W = Path(os.environ.get("SAYA_TEST_WORKFLOW_DIR", "user/default/workflows"))
WORKFLOW = W / os.environ.get("SAYA_TEST_UPSCALE_WORKFLOW", "Saya_Couple_Full.json")
DEFAULT = "RealESRGAN_x4plus_anime_6B.safetensors"


class _Spy:
    """Replaces the loader + the neural upscale with a recording x4 nearest."""

    def __init__(self):
        from comfy_extras import nodes_upscale_model as mod
        from saya_couple.src.nodes import saya_upscale_mode as sut

        self.mod, self.sut = mod, sut
        self.loaded, self.applied = [], []
        self._orig = (mod.UpscaleModelLoader.execute, mod.ImageUpscaleWithModel.execute)
        spy = self

        def load(name):
            spy.loaded.append(name)
            return (("fake-model", name),)

        def apply(model, image):
            spy.applied.append((model, tuple(image.shape)))
            return (torch.repeat_interleave(torch.repeat_interleave(image, 4, dim=1), 4, dim=2),)

        mod.UpscaleModelLoader.execute = staticmethod(load)
        mod.ImageUpscaleWithModel.execute = staticmethod(apply)
        sut._MODEL_CACHE.clear()

    def restore(self):
        self.mod.UpscaleModelLoader.execute, self.mod.ImageUpscaleWithModel.execute = self._orig
        self.sut._MODEL_CACHE.clear()


def _node():
    load_pack()
    from saya_couple.src.nodes.saya_upscale_mode import SayaUpscaleMode
    return SayaUpscaleMode()


def _img(h=48, w=32):
    return torch.rand(1, h, w, 3)


def test_upscale_mode_ui():
    load_pack()
    import folder_paths
    import nodes
    from saya_couple.src.nodes.saya_upscale_mode import SayaUpscaleMode as S
    c = Check("upscale_mode_ui")
    req = S.INPUT_TYPES()["required"]
    c.eq(list(req), ["image", "upscale_mode", "upscale_model_name", "classic_method", "upscale_by"], "exact widgets")
    c.eq(req["upscale_mode"][0], ["UPSCALE MODEL", "CLASSIC"], "modes")
    c.eq(req["classic_method"][0], list(nodes.ImageScale.upscale_methods), "classic methods = ImageScale")
    installed = list(folder_paths.get_filename_list("upscale_models"))
    if installed:
        c.eq(req["upscale_model_name"][0], installed, "models = folder_paths")
    if DEFAULT in installed:
        c.ok(DEFAULT in req["upscale_model_name"][0] and req["upscale_model_name"][1]["default"] == DEFAULT, "RealESRGAN default present")
    ub = req["upscale_by"][1]
    c.ok(math.isfinite(ub["default"]) and ub["min"] <= ub["default"] <= ub["max"] and ub["min"] > 0, "upscale_by default valid")
    c.eq(S.RETURN_TYPES, ("IMAGE",), "single IMAGE output")
    return c.report()


def test_upscale_mode_classic():
    node, spy = _node(), _Spy()
    c = Check("upscale_mode_classic")
    try:
        img = _img()
        (a,) = node.prepare(img, "CLASSIC", DEFAULT, "bicubic", 1.0)
        c.ok(a is img, "A: CLASSIC 1.0 = no processing")
        (b,) = node.prepare(img, "CLASSIC", DEFAULT, "bicubic", 2.0)
        c.eq(tuple(b.shape), (1, 96, 64, 3), "B: CLASSIC x2 size")
        import nodes
        seen, orig = [], nodes.ImageScale.upscale
        nodes.ImageScale.upscale = lambda self, i, m, w, h, cr: (seen.append(m) or orig(self, i, m, w, h, cr))
        try:
            for m in nodes.ImageScale.upscale_methods:
                node.prepare(img, "CLASSIC", DEFAULT, m, 2.0)
        finally:
            nodes.ImageScale.upscale = orig
        c.eq(seen, list(nodes.ImageScale.upscale_methods), "selected classic_method is the one used")
        c.eq((spy.loaded, spy.applied), ([], []), "neural model never loaded/called in CLASSIC")
        try:
            node.prepare(img, "CLASSIC", DEFAULT, "fake", 2.0)
        except ValueError:
            c.ok(True, "unsupported method raises")
        else:
            c.ok(False, "unsupported method raises")
    finally:
        spy.restore()
    return c.report()


def test_upscale_mode_model():
    c = Check("upscale_mode_model")
    if DEFAULT not in __import__("folder_paths").get_filename_list("upscale_models"):
        c.skip(f"upscale model {DEFAULT} not installed in models/upscale_models")
        return c.report()
    node, spy = _node(), _Spy()
    try:
        img = _img()
        (c1,) = node.prepare(img, "UPSCALE MODEL", DEFAULT, "bicubic", 1.0)
        c.eq(len(spy.applied), 1, "C: neural model executed at upscale_by=1.0")
        c.eq(spy.loaded, [DEFAULT], "C: model loaded by selected name")
        c.eq(spy.applied[0][1], (1, 48, 32, 3), "C: model input = original image")
        c.eq(tuple(c1.shape), (1, 48, 32, 3), "C: back to original size")
        (d,) = node.prepare(img, "UPSCALE MODEL", DEFAULT, "bicubic", 2.0)
        c.eq(len(spy.applied), 2, "D: neural model executed at 2.0")
        c.eq(tuple(d.shape), (1, 96, 64, 3), "D: final size = input x2")
        c.eq(spy.loaded, [DEFAULT], "model cached, not reloaded")
        try:
            node.prepare(img, "UPSCALE MODEL", "nope.pth", "bicubic", 1.0)
        except ValueError as error:
            c.ok("nope.pth" in str(error), "unknown model name raises")
        else:
            c.ok(False, "unknown model name raises")
    finally:
        spy.restore()
    return c.report()


def test_upscale_mode_real_model():
    """Runs the REAL installed RealESRGAN x4 model (no spy)."""
    node = _node()
    c = Check("upscale_mode_real_model")
    if DEFAULT not in __import__("folder_paths").get_filename_list("upscale_models"):
        c.skip(f"upscale model {DEFAULT} not installed in models/upscale_models")
        return c.report()
    img = torch.rand(1, 32, 24, 3)
    (same,) = node.prepare(img, "UPSCALE MODEL", DEFAULT, "bicubic", 1.0)
    c.eq(tuple(same.shape), (1, 32, 24, 3), "real model x4 then back to input size")
    c.ok(not torch.equal(same, img), "image really went through the neural model")
    (big,) = node.prepare(img, "UPSCALE MODEL", DEFAULT, "bicubic", 2.0)
    c.eq(tuple(big.shape), (1, 64, 48, 3), "real model, final x2")
    return c.report()


def test_upscale_mode_nan_rejected():
    node = _node()
    c = Check("upscale_mode_nan_rejected")
    for bad in (float("nan"), float("inf"), 0.0, -1.0, "x", None):
        try:
            node.prepare(_img(), "CLASSIC", DEFAULT, "bicubic", bad)
        except ValueError:
            c.ok(True, f"{bad!r} rejected")
        else:
            c.ok(False, f"{bad!r} rejected")
    return c.report()


def _walk(o, path=""):
    if isinstance(o, dict):
        for k, v in o.items():
            yield from _walk(v, f"{path}/{k}")
    elif isinstance(o, list):
        for i, v in enumerate(o):
            yield from _walk(v, f"{path}[{i}]")
    else:
        yield path, o


def test_upscale_mode_workflow():
    c = Check("upscale_mode_workflow")
    if os.environ.get("SAYA_TEST_EXTERNAL_WORKFLOWS") != "1":
        c.skip("SAYA_TEST_EXTERNAL_WORKFLOWS not set — opt-in integration check against the "
               "maintainer's saved workflow file, not part of the autonomous suite")
        return c.report()
    load_pack()
    from saya_couple.src.nodes.saya_upscale_mode import SayaUpscaleMode as S
    if not WORKFLOW.exists():
        c.ok(False, f"workflow missing: {WORKFLOW}")
        return c.report()
    text = WORKFLOW.read_text()
    d = json.loads(text)
    c.ok("NaN" not in text and "Infinity" not in text, "E: no NaN/Infinity token in JSON")
    bad = [p for p, v in _walk(d) if isinstance(v, float) and not math.isfinite(v)]
    c.eq(bad, [], "E: no non-finite float")
    sub = [s for s in d["definitions"]["subgraphs"] if s["name"] == "Phase 02 · USDU"][0]
    every = d["nodes"] + [n for s in d["definitions"]["subgraphs"] for n in s["nodes"]]
    c.ok(not any("ModelSampling" in n["type"] for n in every), "F: no ModelSampling node")
    c.eq([n["type"] for n in every if "UpscaleMode" in n["type"] or "Upscale Mode" in str(n.get("title"))].count("SayaUpscaleModeSwitch"), 0, "G: no old SayaUpscaleModeSwitch")
    modes = [n for n in every if "UpscaleMode" in n["type"]]
    c.eq({n["type"] for n in modes}, {"SayaUpscaleMode"}, "G/H: a single node type")
    c.eq(len(modes), 4, "H: 4 instances")
    c.eq(len({n["id"] for n in modes}), 4, "H: distinct nodes")
    order = list(S.INPUT_TYPES()["required"])[1:]
    nn = {n["id"]: n for n in sub["nodes"]}
    links = {l["id"]: l for l in sub["links"]}
    for n in modes:
        c.eq([i["name"] for i in n["inputs"]], ["image"] + order, f"{n['id']}: inputs order")
        v = n["widgets_values"]
        c.eq(len(v), 4, f"{n['id']}: 4 widget values")
        c.ok(v[0] in ("UPSCALE MODEL", "CLASSIC") and isinstance(v[3], float) and math.isfinite(v[3]) and v[3] > 0, f"{n['id']}: valid values {v}")
        c.ok(v[1] in __import__("folder_paths").get_filename_list("upscale_models"), f"{n['id']}: installed model {v[1]}")
        c.ok(v[2] in __import__("nodes").ImageScale.upscale_methods, f"{n['id']}: supported method {v[2]}")
        c.eq(n["widgets_values_named"], dict(zip(order, v)), f"{n['id']}: widgets_values_named")
    chain = [("AUTO LOAD", 10123, 10113), ("Hires 1", 10113, 11004), ("USDU 1", 11004, 11005), ("USDU 2", 11005, 10115)]
    for lane, up, consumer in chain:
        cons_in = [i for i in nn[consumer]["inputs"] if i["name"] == "image"][0]
        sw = nn[links[cons_in["link"]]["origin_id"]]
        c.eq(sw["type"], "SayaUpscaleMode", f"chain: switch before {consumer}")
        c.eq(links[sw["inputs"][0]["link"]]["origin_id"], up, f"chain: {up} -> switch -> {consumer}")
    c.eq(links[nn[10115]["outputs"][0]["links"][0]]["target_id"], -20, "Hires 3 still feeds output")
    return c.report()


TESTS = (test_upscale_mode_ui, test_upscale_mode_classic, test_upscale_mode_model,
         test_upscale_mode_real_model, test_upscale_mode_nan_rejected, test_upscale_mode_workflow)

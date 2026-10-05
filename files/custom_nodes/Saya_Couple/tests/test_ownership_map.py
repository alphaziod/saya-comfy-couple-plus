"""Sampler 1 ownership map: imprint block, shared rasterization, HiDream partition, capture node."""

from __future__ import annotations

from harness import Check, load_pack
from test_hidream_reconstruct import _imprint


def _mods():
    load_pack()
    import torch
    from saya_couple.src.nodes import couple_hidream_reconstruct as hr
    from saya_couple.src.nodes import couple_imprint_v2 as v2
    from saya_couple.src.nodes import ownership_map as om
    from saya_couple.src.nodes import region_masks as rm

    return torch, v2, rm, hr, om


def _with_map(v2, rows, **kwargs):
    data = _imprint(v2, **kwargs)
    data["couple_imprint"]["ownership_map"] = {"grid": [len(rows), len(rows[0])], "rows": rows, "source": "s1_dynamic"}
    return v2.validate_imprint_v2(data)


def test_ownership_block():
    torch, v2, rm, hr, om = _mods()
    c = Check("ownership_block")
    data = _with_map(v2, ["1b", "s2"])
    c.eq(v2.parse_imprint_json(v2.canonical_imprint_json(data))["couple_imprint"]["ownership_map"]["rows"], ["1b", "s2"],
         "the map survives the canonical JSON round trip")
    for rows, why in ((["1x", "s2"], "unknown cell"), (["1b", "s"], "short row"), (["1b"], "missing row")):
        bad = _imprint(v2)
        bad["couple_imprint"]["ownership_map"] = {"grid": [2, 2], "rows": rows, "source": "s1_dynamic"}
        c.raises(v2.SayaCoupleImprintError, lambda bad=bad: v2.validate_imprint_v2(bad), f"refused: {why}")
    solo = _imprint(v2, p2=None)
    solo["couple_imprint"]["ownership_map"] = {"grid": [1, 1], "rows": ["1"], "source": "s1_dynamic"}
    c.raises(v2.SayaCoupleImprintError, lambda: v2.validate_imprint_v2(solo), "refused: Solo imprint with a map")
    return c.report()


def test_ownership_rasterization():
    torch, v2, rm, hr, om = _mods()
    c = Check("ownership_rasterization")
    # Left/right split at 50 %: P1 static region = left half. Map: top-left P1, top-right background,
    # bottom-left undecided, bottom-right P2.
    m1, m2 = rm.derive_raw_region_masks(_with_map(v2, ["1b", "s2"]), 4, 4)
    m1, m2 = m1[0], m2[0]
    c.ok(bool((m1[:2, :2] == 1).all() and (m2[:2, :2] == 0).all()), "P1 cell: P1 only")
    c.ok(bool((m1[2:, 2:] == 0).all() and (m2[2:, 2:] == 1).all()), "P2 cell: P2 only")
    c.ok(bool((m1[:2, 2:] == 0).all() and (m2[:2, 2:] == 1).all()), "background cell keeps the static split (right half = P2)")
    c.ok(bool((m1[2:, :2] == 1).all() and (m2[2:, :2] == 0).all()), "undecided cell keeps the static split (left half = P1)")
    # A P2 cell inside P1's static half moves the pixel to P2, hard edge, at any resolution (nearest).
    m1, m2 = rm.derive_raw_region_masks(_with_map(v2, ["2s"]), 8, 16)
    c.ok(bool((m2[0, :, :8] == 1).all() and (m1[0, :, :8] == 0).all()), "map overrides the static half (resized nearest)")
    c.ok(bool(((m1 == 0) | (m1 == 1)).all()), "binary ownership, no blend")
    ppm = rm.derive_masks(_with_map(v2, ["2s"]), 8, 16)
    c.ok(bool((ppm.person_1[0, :, :8] == 0).all() and (ppm.person_2[0, :, :8] > 0).all()), "PPM masks (USDU, Detailers) read the same map")
    plain1, plain2 = rm.derive_raw_region_masks(_imprint(v2), 8, 16)
    c.ok(bool((plain1[0, :, :8] == 1).all()), "imprint without map: static split unchanged")
    a, b = hr.pure_geometry_masks(_imprint(v2)["couple_imprint"]["geometry"], 8, 16, {"grid": [1, 2], "rows": ["2s"], "source": "s1_dynamic"})
    c.ok(bool((b[0, :, :8] == 1).all() and ((a + b) == 1).all()), "HiDream: map applied, P1/P2 still partition the frame")
    return c.report()


def _state(torch, active, block=None):
    active = torch.tensor(active).unsqueeze(0)
    return {"active": active, "map": torch.zeros(1, 3, *active.shape[1:]),
            "block": None if block is None else torch.tensor(block).unsqueeze(0)}


def test_ownership_capture():
    torch, v2, rm, hr, om = _mods()
    c = Check("ownership_capture")
    states: dict = {}
    om._engine_state = lambda: states
    calls = []
    om.apply_multimask_couple = lambda model, clip, m1, m2, *args, **kwargs: calls.append((m1, m2)) or "rebuilt"
    node = om.SayaOwnershipMapCapture()
    payload = v2.canonical_imprint_json(_imprint(v2))
    pos_1 = [[torch.zeros(1, 2, 4), {}]]
    recipe = {"model_2": "raw", "model_2_patched": "static", "clip": None, "pos_1": pos_1, "neg_1": 2, "pos_2": 3, "neg_2": 4,
              "strength_1": 1.0, "strength_2": 1.0, "main": 5, "dynamic": True}
    key = id(pos_1[0][0])  # the engine keys its state by the P1 conditioning tensor
    latent = {"samples": torch.zeros(1, 4, 16, 16)}

    states[key] = _state(torch, [[0, 2], [-1, 1]])
    out, model, report = node.capture(latent, payload, recipe)
    got = v2.parse_imprint_json(out)["couple_imprint"].get("ownership_map")
    c.eq(got and got["rows"], ["1b", "s2"], "engine state -> imprint ownership_map (P1, background, undecided, P2)")
    c.eq(states, {}, "the engine state is consumed once")
    c.eq(model, "rebuilt", "MODEL_2 rebuilt on the map")
    c.ok(bool((calls[-1][0][0, :64, :64] == 1).all()) and tuple(calls[-1][0].shape[-2:]) == (128, 128),
         "Sampler 2 masks come from the same imprint, at the latent's pixel size")
    out2, _, report2 = node.capture(latent, payload, recipe)
    c.ok(out2 == out and "reused" in report2, "cached Sampler 1 latent: same map, not another sampling's state")

    states[key] = _state(torch, [[0, 1], [1, 0]])
    out, model, report = node.capture({"samples": torch.zeros(1, 4, 16, 16)}, payload, {**recipe, "dynamic": False})
    c.ok("ownership_map" not in v2.parse_imprint_json(out)["couple_imprint"] and model == "static",
         "static ownership: imprint unchanged, static MODEL_2")
    out, model, report = node.capture({"samples": torch.zeros(1, 4, 16, 16)}, payload, recipe)
    c.ok("ownership_map" not in v2.parse_imprint_json(out)["couple_imprint"] and model == "static" and "no ownership_map" in report,
         "new Sampler 1 latent without engine state: static, never a stale map")

    states["k"] = _state(torch, [[-1, -1], [-1, 1]], block=[[0, -1], [1, -1]])
    out, _, _ = node.capture({"samples": torch.zeros(1, 4, 16, 16)}, payload, None)
    c.eq(v2.parse_imprint_json(out)["couple_imprint"]["ownership_map"]["rows"], ["1s", "22"], "zone_fallback blocks join the map")
    states["k"] = _state(torch, [[0, 1]])
    out, model, report = node.capture({"samples": torch.zeros(1, 4, 16, 16)}, payload, None)
    c.ok("ownership_map" not in v2.parse_imprint_json(out)["couple_imprint"] and model is None, "grid / latent aspect mismatch: no map")
    states["k"] = _state(torch, [[0, 1], [1, 0]])
    solo = v2.canonical_imprint_json(_imprint(v2, p2=None))
    out, _, report = node.capture({"samples": torch.zeros(1, 4, 16, 16)}, solo, None)
    c.ok("ownership_map" not in v2.parse_imprint_json(out)["couple_imprint"] and "Solo" in report, "Solo: no map")
    states["k"] = _state(torch, [[0, 1], [1, 0]])
    cached = {"samples": torch.zeros(1, 4, 16, 16)}
    node.capture(cached, payload, None)
    out, _, report = node.capture(cached, solo, None)
    c.ok("ownership_map" not in v2.parse_imprint_json(out)["couple_imprint"] and "Solo" in report,
         "cached Sampler 1 latent, imprint turned Solo: map dropped, no hard error")
    return c.report()


def test_background_region():
    """Map background cells: scene-only MAIN (no ACTION, no person) in every pass that can take a 3rd region."""
    torch, v2, rm, hr, om = _mods()
    c = Check("ownership_background_region")
    data = _with_map(v2, ["1b", "s2"])
    m1, m2 = rm.derive_raw_region_masks(data, 4, 4, background=True)
    scene = rm.derive_background_mask(data, 4, 4)
    c.ok(bool((m1[0, :2, 2:] == 0).all() and (m2[0, :2, 2:] == 0).all() and (scene[0, :2, 2:] == 1).all()),
         "background cell: neither person, scene region only")
    c.ok(bool((scene[0] + m1[0] + m2[0] == 1).all()), "P1 + P2 + scene cover every pixel exactly once")
    ppm = rm.derive_masks(data, 4, 4, background=True)
    c.ok(bool((ppm.base[0, :2, 2:] == 0).all() and (ppm.scene[0, :2, 2:] > 0).all() and (ppm.base[0, 2:, :] > 0).all()),
         "PPM: global MAIN (with ACTION) off the background, scene-only MAIN on it")
    c.ok(bool(((ppm.base + ppm.person_1 + ppm.person_2 + ppm.scene) > 0).all()), "PPM: every pixel keeps a positive sum")
    c.ok(rm.derive_background_mask(_with_map(v2, ["12"]), 4, 4) is None and rm.derive_background_mask(_imprint(v2), 4, 4) is None,
         "no background cell / no map: no scene region")
    a, b = hr.pure_geometry_masks(data["couple_imprint"]["geometry"], 4, 4, data["couple_imprint"]["ownership_map"])
    c.ok(bool(((a + b) == 1).all()), "HiDream (2 regions max): background keeps the P1/P2 partition")

    from saya_couple.src.nodes import saya_multi_couple as mc
    seen = {}
    cond = {name: [[torch.full((1, 2, 4), float(k)), {}]] for k, name in enumerate(("p1", "n1", "p2", "n2", "main", "scene"))}
    mc.apply_multimask_couple(None, None, m1, m2, cond["p1"], cond["n1"], cond["p2"], cond["n2"], 1.0, 1.0, 0.65, 0.35,
                              main=cond["main"], couple_fn=lambda model, clip, pos, neg: seen.update(pos=pos, neg=neg) or (model,),
                              scene=cond["scene"], background_mask=scene)
    c.ok(seen["pos"][-1][0] is cond["scene"][0][0] and bool((seen["pos"][-1][1]["mask"] == scene).all()),
         "MultiMaskCouple (S2, Hires, Phase 6): scene region on the background mask")
    c.eq(len(seen["neg"]), 3, "the background keeps a negative")

    from saya_couple.src.nodes import couple_reconstruct as cr
    from test_conditioning_cache import _Env
    from saya_couple.src.nodes import conditioning_cache as cache
    from test_multi_couple import _FakeClip, _FakeModel
    import json
    identities = json.dumps([{"role": "phase_model", "identifier": "base.safetensors", "source": "checkpoint_hub_widget"},
                             {"role": "base_clip", "identifier": "base.safetensors", "source": "checkpoint_hub_widget"}])
    calls = {}

    def fake_ppm(model, *args):
        calls["ppm"] = args
        return model.clone()

    def fake_mm(model, *args, **kwargs):
        calls["mm"] = kwargs
        return model.clone()

    saved = cr._apply_couple_patch, cr._apply_multimask_patch, cr.mark_couple_patch
    cr._apply_couple_patch, cr._apply_multimask_patch, cr.mark_couple_patch = fake_ppm, fake_mm, lambda *a, **k: None
    try:
        with _Env(torch, cache, cr) as env:
            with_action = _imprint(v2, identifier="base.safetensors")
            with_action["couple_imprint"]["prompts"]["action"] = "2girls, kissing"
            with_action["couple_imprint"]["ownership_map"] = {"grid": [2, 2], "rows": ["1b", "s2"], "source": "s1_dynamic"}
            report = cr.SayaCoupleReconstruct().reconstruct(model=_FakeModel(), clip=_FakeClip(), checkpoint_identities=identities,
                                                            image=torch.zeros(1, 64, 64, 3), imprint=v2.validate_imprint_v2(with_action))[3]
            c.ok("main" in env.encoded and "main,\n2girls, kissing" in env.encoded, "scene (MAIN) encoded apart from MAIN + ACTION")
            c.eq(len(calls["ppm"]), 8, "PPM (USDU, Detailers): cond_3/mask_3 = scene region")
            c.ok(calls["mm"].get("background_mask") is not None and calls["mm"].get("scene") is not None,
                 "MultiMaskCouple reconstruct: scene region")
            c.ok("scene-only MAIN" in report, "report names the background region")
            calls.clear()
            cr.SayaCoupleReconstruct().reconstruct(model=_FakeModel(), clip=_FakeClip(), checkpoint_identities=identities,
                                                   image=torch.zeros(1, 64, 64, 3), imprint=_imprint(v2, identifier="base.safetensors"))
            c.ok(len(calls["ppm"]) == 6 and calls["mm"] == {}, "imprint without map: patches unchanged")
    finally:
        cr._apply_couple_patch, cr._apply_multimask_patch, cr.mark_couple_patch = saved
    return c.report()


TESTS = (test_ownership_block, test_ownership_rasterization, test_ownership_capture, test_background_region)

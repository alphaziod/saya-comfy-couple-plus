"""SayaHiDreamSafeScale: deterministic token-ceiling barrier, HiDream ratio families, never upscales."""

import json

import torch

from harness import Check, load_pack

BUDGET = 4032


def _mod():
    load_pack()
    from saya_couple.src.nodes import hidream_safe_scale as m

    return m


def test_official_buckets():
    m = _mod()
    c = Check("safe_scale_official_buckets")
    c.eq(set(m.HIDREAM_BUCKETS), {(1024, 1024), (768, 1360), (1360, 768), (880, 1168), (1168, 880), (832, 1248), (1248, 832)},
         "buckets = official HiDream-I1 RESOLUTION_OPTIONS")
    for (w, h), expected in (((1024, 1024), (1024, 1024)), ((896, 1152), (880, 1168)), ((832, 1216), (832, 1248)),
                             ((1216, 832), (1248, 832)), ((1152, 648), (1360, 768)), ((1536, 864), (1360, 768)),
                             ((3000, 4000), (880, 1168)), ((1536, 2048), (880, 1168))):
        c.eq(m.nearest_bucket(w, h)[0], expected, f"{w}x{h} -> family {expected}")
    return c.report()


def test_a_under_ceiling():
    m = _mod()
    c = Check("safe_scale_test_A_under_ceiling")
    for w, h in ((640, 768), (768, 768), (704, 928), (512, 512), (832, 832), (896, 1152), (1152, 896), (864, 1152), (1008, 1008), (1024, 768)):
        r = m.select_size(w, h, BUDGET, 0.01)
        c.ok(not r["was_scaled"] and (r["selected_width"], r["selected_height"]) == (w, h), f"{w}x{h} ({m.tokens_of(w, h)} tokens): passthrough")
    r = m.select_size(896, 1152, BUDGET, 0.01)
    c.ok(r["was_scaled"] is False and r["selected_tokens"] == 4032, "896x1152 = 4032 tokens = ceiling: passthrough")
    r = m.select_size(1024, 1024, BUDGET, 0.01)  # 4096 tokens: slightly above the ceiling, must not pass through
    c.ok(r["was_scaled"] and r["selected_tokens"] <= BUDGET, "1024x1024 (4096 tokens > 4032): reduced")
    return c.report()


def test_b_far_above_ceiling():
    m = _mod()
    c = Check("safe_scale_test_B_above_ceiling")
    for w, h in ((3000, 4000), (1536, 2048), (2048, 2048), (1024, 1024), (4096, 2304), (1441, 999), (1792, 2304)):
        r = m.select_size(w, h, BUDGET, 0.01)
        sw, sh = r["selected_width"], r["selected_height"]
        c.ok(r["was_scaled"], f"{w}x{h}: reduction required")
        c.ok(m.tokens_of(sw, sh) <= BUDGET and r["selected_pixels"] <= BUDGET * 256, f"{w}x{h} -> {sw}x{sh}: under the hard ceiling")
        c.ok(sw % 16 == 0 and sh % 16 == 0 and sw <= w and sh <= h, f"{w}x{h}: multiples of 16, never upscaled")
        c.ok(abs(sw / sh / (w / h) - 1) <= 0.05, f"{w}x{h}: ratio preserved as closely as possible ({sw / sh:.3f} vs {w / h:.3f})")
        c.ok(m.tokens_of(sw, sh) > 0.9 * BUDGET, f"{w}x{h}: largest safe size, no needless reduction ({m.tokens_of(sw, sh)} tokens)")
        c.ok("hard safe ceiling" in r["reason"], f"{w}x{h}: reason = hard ceiling")
    r = m.select_size(1792, 2304, BUDGET, 0.01)
    c.eq((r["selected_width"], r["selected_height"]), (896, 1152), "1792x2304 (same ratio) -> exactly 896x1152")
    r = m.select_size(3000, 4000, BUDGET, 0.01)
    c.eq(r["nearest_hidream_family"], "880x1168", "3000x4000 -> family 880x1168")
    c.ok(r["ceiling_utilization_pct"] >= 90.0 and r["linear_scale_factor"] < 0.5 and "STRONG REDUCTION" in r["quality_warning"],
         "3000x4000: utilization >= 90 %, strong reduction reported in quality_warning")
    r = m.select_size(1536, 2048, BUDGET, 0.01)
    c.ok(r["quality_warning"] == "" and r["ceiling_utilization_pct"] >= 90.0, f"1536x2048 -> {r['selected_width']}x{r['selected_height']}: moderate reduction, no warning")
    c.ok(m.tokens_of(r["selected_width"], r["selected_height"]) <= r["hard_ceiling_tokens"], "quality floor never allows exceeding the ceiling")
    return c.report()


def test_ceiling_property():
    """No input size can produce a selection above the ceiling (wide sweep, varied ratios)."""
    m = _mod()
    c = Check("safe_scale_ceiling_property")
    worst = 0
    bad = []
    for w in list(range(16, 4200, 173)) + [1024, 3000, 4096, 8192]:
        for h in list(range(16, 4200, 197)) + [1024, 4000, 4096, 8192]:
            r = m.select_size(w, h, BUDGET, 0.01)
            tokens = m.tokens_of(r["selected_width"], r["selected_height"])
            if r["was_scaled"]:
                worst = max(worst, tokens)
                if tokens > BUDGET or r["selected_width"] > w or r["selected_height"] > h or r["selected_width"] < 16 or r["selected_height"] < 16:
                    bad.append((w, h, r["selected_width"], r["selected_height"]))
            elif (r["selected_width"], r["selected_height"]) != (w, h) or m.tokens_of(w, h) > BUDGET:
                bad.append((w, h, "invalid passthrough"))
    c.ok(not bad, f"never above the ceiling nor upscaled ({len(bad)} violations, ex. {bad[:3]})")
    c.ok(worst <= BUDGET, f"worst selected case {worst} <= {BUDGET} tokens")
    return c.report()


def test_node_and_report():
    m = _mod()
    c = Check("safe_scale_node")
    node = m.SayaHiDreamSafeScale()
    small = torch.rand(1, 768, 640, 3)
    out, ow, oh, ww, wh, was, factor, report = node.scale(small, BUDGET, 0.01)
    c.ok(out is small and (ow, oh, ww, wh) == (640, 768, 640, 768) and was is False and factor == 1.0, "TEST A: same tensor, was_scaled False")
    big = torch.rand(1, 2048, 1536, 3)
    out, ow, oh, ww, wh, was, factor, report = node.scale(big, BUDGET, 0.01)
    c.eq((ow, oh), (1536, 2048), "original size passed through")
    c.eq(tuple(out.shape), (1, wh, ww, 3), "working image = reported size")
    c.ok(was is True and abs(factor - ww / 1536) < 1e-9 and m.tokens_of(ww, wh) <= BUDGET, "TEST B: reduced under the ceiling, real factor")
    info = json.loads(report)
    for key in ("original_width", "original_height", "original_pixels", "selected_width", "selected_height", "selected_pixels",
                "hard_safe_work_token_budget", "hard_safe_work_pixel_budget", "hard_ceiling_tokens", "ceiling_utilization_pct", "linear_scale_factor", "quality_warning", "selected_tokens", "original_tokens", "nearest_hidream_family", "scale_factor", "was_scaled",
                "reason", "remaining_margin_tokens"):
        c.ok(key in info, f"report contains {key}")
    c.ok(info["remaining_margin_tokens"] >= 0, "remaining margin >= 0")
    return c.report()


def test_registry():
    pack = load_pack()
    c = Check("safe_scale_registry")
    name = "SayaHiDreamSafeScale"
    c.ok(name in pack.NODE_CLASS_MAPPINGS and name in pack.NODE_DISPLAY_NAME_MAPPINGS, "registered")
    c.eq(pack.NODE_CLASS_MAPPINGS[name].RETURN_TYPES, ("IMAGE", "INT", "INT", "INT", "INT", "BOOLEAN", "FLOAT", "STRING"), "RETURN_TYPES")
    c.eq(list(pack.NODE_CLASS_MAPPINGS[name].INPUT_TYPES()["required"]), ["image", "hard_safe_work_token_budget", "snap_tolerance"], "inputs")
    return c.report()


TESTS = (test_official_buckets, test_a_under_ceiling, test_b_far_above_ceiling, test_ceiling_property, test_node_and_report, test_registry)

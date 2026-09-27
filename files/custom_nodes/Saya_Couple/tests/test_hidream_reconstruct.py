"""SayaCoupleHiDreamReconstruct: texts, pure masks, hard errors, registry."""

from __future__ import annotations

import tempfile
from pathlib import Path

from harness import Check, load_pack


def _mods():
    load_pack()
    import torch
    from saya_couple.src.nodes import couple_hidream_reconstruct as hr
    from saya_couple.src.nodes import couple_imprint_v2 as v2

    return torch, hr, v2


def _imprint(v2, *, main="main", p1="p1", p2="p2", neg="bad, ugly", orientation=None,
             split=0.5, swap=False, feather=0.0, unit="axis_fraction", floor=0.0,
             identifier="base.safetensors"):
    return v2.build_imprint_v2(
        prompts=v2.build_prompts(main=main, person_1=p1, person_2=p2, negative=neg),
        geometry=v2.build_two_region_geometry(
            orientation=orientation or v2.ORIENTATION_LEFT_RIGHT,
            reference_width=64, reference_height=64,
            split_fraction=split, swap=swap, feather=feather, feather_unit=unit,
            person_2_present=p2 is not None and bool(p2.strip()), mask_floor=floor,
        ),
        strengths=v2.build_strengths(),
        reconstruction_recipe=v2.build_reconstruction_recipe(
            checkpoint_identity=v2.build_identity(role=v2.ROLE_PHASE_MODEL, identifier=identifier),
            clip_identity=v2.build_identity(role=v2.ROLE_BASE_CLIP, identifier=identifier),
        ),
        provenance=v2.build_provenance(
            workflow_version="test", pack_discriminant="sha256:test",
            created_at="2026-09-23T00:00:00+00:00",
        ),
    )


def _run(torch, hr, imprint, latent=None, trigger="", encoder=None, solo=False):
    """Run the node with a fake encoder; returns (outputs, encoded texts)."""
    calls: list[str] = []

    def fake(clip, text):
        calls.append(text)
        return [[torch.zeros(1, 4, 8), {"conditioning_llama3": torch.zeros(1, 1, 4, 8), "text": text}]]

    original, original_root, original_release = hr._encode_text, hr.cache.CACHE_ROOT, hr.cache.release_clip
    hr._encode_text = encoder or fake
    hr.cache.release_clip = lambda clip: None
    try:
        with tempfile.TemporaryDirectory() as cache_root:  # fresh cache: we test encoding here, not the cache
            hr.cache.CACHE_ROOT = Path(cache_root)
            if latent is None:
                latent = {"samples": torch.zeros(1, 16, 96, 96)}
            out = hr.SayaCoupleHiDreamReconstruct().reconstruct(imprint, object(), latent, "clip-files", trigger, solo)
    finally:
        hr._encode_text, hr.cache.CACHE_ROOT, hr.cache.release_clip = original, original_root, original_release
    return out, calls


def _expect(c, hr, fn, needle, label):
    try:
        fn()
    except hr.SayaCoupleHiDreamReconstructError as error:
        c.ok(needle in str(error), f"{label}: message {str(error)!r} lacks {needle!r}")
        return
    except Exception as error:
        c.failures.append(f"{label}: raised {type(error).__name__}, want SayaCoupleHiDreamReconstructError")
        return
    c.failures.append(f"{label}: no exception")


def test_compose_truth_table():
    _, hr, _ = _mods()
    c = Check("hidream_compose_truth_table")
    base = {"main": "main", "person_1": "p1", "person_2": "p2", "negative": "bad"}
    r = hr.compose_prompts(base, "TRG")
    c.eq(r, {"a": "TRG, main, p1", "b": "TRG, main, p2", "unmasked": "TRG, main", "negative": "bad"},
         "trigger + P2")
    c.ok("TRG" not in r["negative"], "no trigger in negative")
    r = hr.compose_prompts(base, "")
    c.eq((r["a"], r["b"], r["unmasked"]), ("main, p1", "main, p2", "main"), "no trigger")
    for text in (r["a"], r["b"], r["unmasked"]):
        c.ok(not text.startswith(",") and not text.endswith(",") and ",," not in text, f"clean commas {text!r}")
    no_p2 = {k: v for k, v in base.items() if k != "person_2"}
    r = hr.compose_prompts(no_p2, "TRG")
    c.eq(r["b"], "TRG, main", "P2 absent -> b = base")
    c.eq(r["b"], r["unmasked"], "P2 absent: b == unmasked")
    r = hr.compose_prompts({**no_p2, "main": ""}, "TRG")
    c.eq((r["a"], r["unmasked"]), ("TRG, p1", "TRG"), "empty MAIN")
    r = hr.compose_prompts(base, "  \t ")
    c.eq((r["a"], r["unmasked"]), ("main, p1", "main"), "whitespace trigger = empty")
    c.eq(hr.compose_prompts(base, "  TRG ")["a"], "TRG, main, p1", "trigger stripped")
    return c.report()


def test_node_encoded_texts():
    torch, hr, v2 = _mods()
    c = Check("hidream_node_encoded_texts")
    imprint = _imprint(v2)
    (ca, cb, cu, cn, _, _, regional), calls = _run(torch, hr, imprint, trigger="TRG")
    c.eq(regional, True, "Couple mode keeps regional path enabled")
    c.eq(calls, ["TRG, main, p1", "TRG, main, p2", "TRG, main", "bad, ugly"], "4 encode calls in order")
    c.eq(calls[3], "bad, ugly", "negative byte-for-byte with trigger set")
    c.eq([x[0][1]["text"] for x in (ca, cb, cu, cn)], calls, "outputs map to roles")
    _, calls_off = _run(torch, hr, imprint, trigger="")
    c.eq(calls_off, ["main, p1", "main, p2", "main", "bad, ugly"], "trigger off texts")
    diff = [i for i in range(4) if calls[i] != calls_off[i]]
    c.eq(diff, [0, 1, 2], "trigger on/off differ only in the 3 positives")
    return c.report()


def _is_binary(t):
    return bool(((t == 0) | (t == 1)).all())


def test_masks():
    torch, hr, v2 = _mods()
    c = Check("hidream_masks")
    out, _ = _run(torch, hr, _imprint(v2))
    a, b = out[4], out[5]
    c.eq(tuple(a.shape), (1, 768, 768), "mask_a shape")
    c.eq(tuple(b.shape), (1, 768, 768), "mask_b shape")
    c.ok(bool((a[..., :384] == 1).all()) and bool((a[..., 384:] == 0).all()), "A = left half")
    c.ok(bool((b[..., :384] == 0).all()) and bool((b[..., 384:] == 1).all()), "B = right half")
    c.ok(bool((a + b == 1).all()), "a + b == 1 everywhere")
    c.ok(_is_binary(a) and _is_binary(b), "binary")

    out, _ = _run(torch, hr, _imprint(v2, orientation=v2.ORIENTATION_TOP_BOTTOM))
    a, b = out[4], out[5]
    c.ok(bool((a[:, :384] == 1).all()) and bool((a[:, 384:] == 0).all()), "TOP_BOTTOM: A = top")
    c.ok(bool((a + b == 1).all()) and _is_binary(a), "TOP_BOTTOM: complement + binary")

    out, _ = _run(torch, hr, _imprint(v2, swap=True))
    a, b = out[4], out[5]
    c.ok(bool((a[..., 384:] == 1).all()) and bool((a[..., :384] == 0).all()), "swap: P1 on the right")
    c.ok(bool((b[..., :384] == 1).all()), "swap: P2 on the left")

    out, _ = _run(torch, hr, _imprint(v2, split=0.25))
    c.eq(int(out[4][0, 0].sum().item()), 192, "split 0.25 -> 192 px")

    out, _ = _run(torch, hr, _imprint(v2), latent={"samples": torch.zeros(1, 16, 96, 100)})
    c.eq(tuple(out[4].shape), (1, 768, 800), "non-square: latent*8, not reference 64x64")
    c.eq(tuple(out[5].shape), (1, 768, 800), "non-square mask_b")

    for unit in ("pixel_sigma", "axis_fraction"):
        out, _ = _run(torch, hr, _imprint(v2, feather=0.3, unit=unit, floor=0.2))
        a, b = out[4], out[5]
        c.ok(_is_binary(a) and _is_binary(b), f"{unit}: feather/floor ignored -> binary")
        c.ok(bool((a + b == 1).all()), f"{unit}: complement")
        c.ok(bool((a[..., :384] == 1).all()) and bool((a[..., 384:] == 0).all()), f"{unit}: hard edge at split")
    return c.report()


def test_p2_absent():
    torch, hr, v2 = _mods()
    c = Check("hidream_p2_absent")
    imprint = _imprint(v2, p2=None)
    c.ok("person_2" not in imprint["couple_imprint"]["prompts"], "fixture: P2 key absent")
    (ca, cb, cu, cn, a, b, regional), calls = _run(torch, hr, imprint, trigger="TRG")
    c.eq(regional, True, "P2 absent alone does not imply Solo")
    c.eq(calls, ["TRG, main, p1", "TRG, main", "bad, ugly"], "b and unmasked share one encode")
    c.eq(cb[0][1]["text"], cu[0][1]["text"], "b text == unmasked text")
    c.ok(bool((b == 0).all()), "mask_b all zeros")
    c.eq((tuple(b.shape), b.dtype), (tuple(a.shape), a.dtype), "mask_b same shape/dtype as mask_a")
    c.ok(bool((a[..., :384] == 1).all()) and bool((a[..., 384:] == 0).all()), "mask_a = P1 region")
    c.ok("person_2" not in imprint["couple_imprint"]["prompts"], "imprint not mutated (P2 still absent)")
    return c.report()



def test_solo_global():
    torch, hr, v2 = _mods()
    c = Check("hidream_solo_global")
    imprint = _imprint(v2)
    (ca, cb, cu, cn, a, b, regional), calls = _run(torch, hr, imprint, trigger="TRG", solo=True)
    c.eq(calls, ["TRG, main, p1", "bad, ugly"], "Solo encodes only global MAIN+P1 and NEG")
    c.eq([x[0][1]["text"] for x in (ca, cb, cu)], ["TRG, main, p1"] * 3, "all positive outputs are global MAIN+P1")
    c.ok(bool((a == 0).all()) and bool((b == 0).all()), "Solo masks are empty")
    c.eq(regional, False, "Solo disables regional patch path")
    c.eq(cn[0][1]["text"], "bad, ugly", "negative unchanged")
    return c.report()

def test_hard_errors():
    torch, hr, v2 = _mods()
    c = Check("hidream_hard_errors")
    good = _imprint(v2)
    _expect(c, hr, lambda: _run(torch, hr, {"version": 1, "prompts": {}}), "imprint:", "v1 imprint")
    _expect(c, hr, lambda: _run(torch, hr, "not a dict"), "imprint:", "non-dict imprint")
    for label, latent in (("3D", {"samples": torch.zeros(16, 96, 96)}),
                          ("no samples", {}), ("not dict", [])):
        _expect(c, hr, lambda latent=latent: _run(torch, hr, good, latent=latent), "latent:", f"latent {label}")

    # imprint builders accept "" for main/person_1 (type check only)
    try:
        empty = _imprint(v2, main="", p1="")
    except Exception as error:
        c.failures.append(f"fixture: empty MAIN+P1 imprint unbuildable: {error}")
    else:
        _expect(c, hr, lambda: _run(torch, hr, empty), "MAIN and PERSON_1 are both empty", "MAIN and P1 empty")
        c.eq(hr.compose_prompts(empty["couple_imprint"]["prompts"], "")["a"], "", "compose a == ''")
        # a trigger alone is enough to keep A non-empty (documented compose behavior)
        _, calls = _run(torch, hr, empty, trigger="TRG")
        c.eq(calls[0], "TRG", "trigger alone keeps A non-empty")

    def no_llama(clip, text):
        cond = {} if text == "p2" or text.endswith(", p2") else {"conditioning_llama3": 1}
        return [[torch.zeros(1, 4, 8), cond]]

    _expect(c, hr, lambda: _run(torch, hr, good, encoder=no_llama), "b: conditioning_llama3", "missing llama3 names role")

    def no_llama_neg(clip, text):
        cond = {} if text == "bad, ugly" else {"conditioning_llama3": 1}
        return [[torch.zeros(1, 4, 8), cond]]

    _expect(c, hr, lambda: _run(torch, hr, good, encoder=no_llama_neg), "negative: conditioning_llama3", "missing llama3 on negative")
    c.ok(hr._encode_text is hr._default_encode_text, "_encode_text restored after errors")
    return c.report()


def test_imprint_identities_not_compared():
    torch, hr, v2 = _mods()
    c = Check("hidream_identities_ignored")
    imprint = _imprint(v2, identifier="waiIllustriousSDXL_v170.safetensors")
    out, calls = _run(torch, hr, imprint)
    c.eq(len(calls), 4, "runs with SDXL identities, no hub input")
    c.eq(tuple(out[4].shape), (1, 768, 768), "masks produced")
    return c.report()


def test_registry_and_signature():
    pack = load_pack()
    _, hr, _ = _mods()
    c = Check("hidream_registry_signature")
    name = "SayaCoupleHiDreamReconstruct"
    c.ok(pack.NODE_CLASS_MAPPINGS.get(name) is hr.SayaCoupleHiDreamReconstruct, "class registered")
    c.ok(bool(pack.NODE_DISPLAY_NAME_MAPPINGS.get(name)), "display name registered")
    node = hr.SayaCoupleHiDreamReconstruct
    c.eq(node.RETURN_TYPES, ("CONDITIONING", "CONDITIONING", "CONDITIONING", "CONDITIONING", "MASK", "MASK", "BOOLEAN"),
         "RETURN_TYPES")
    c.eq(node.RETURN_NAMES, ("conditioning_a", "conditioning_b", "conditioning_unmasked", "negative",
                             "mask_a", "mask_b", "regional_enabled"), "RETURN_NAMES")
    types = node.INPUT_TYPES()
    c.eq(list(types["required"]), ["imprint", "clip", "latent", "clip_identity"], "required")
    c.ok(types["required"]["clip"][1].get("lazy") is True, "clip is lazy (the Quad CLIP loader only runs if the cache is incomplete)")
    c.eq(list(types["optional"]), ["hidream_trigger", "solo"], "optional")
    return c.report()


def test_malformed_imprint_is_a_clean_error():
    """Structurally broken JSON must raise SayaCoupleImprintError, never a raw TypeError/KeyError."""
    import copy
    import json

    _, _, v2 = _mods()
    from saya_couple.src.nodes import couple_imprint as v1

    c = Check("malformed_imprint_clean_error")
    good = _imprint(v2)

    def broken(mutate):
        data = copy.deepcopy(good)
        mutate(data)
        return json.dumps(data)

    cases = {
        "person_2 not a string": lambda d: d["couple_imprint"]["prompts"].__setitem__("person_2", 123),
        "prompts.main missing": lambda d: d["couple_imprint"]["prompts"].pop("main"),
        "unexpected prompts key": lambda d: d["couple_imprint"]["prompts"].__setitem__("extra", "x"),
        "lora entry incomplete": lambda d: d["reconstruction_recipe"].__setitem__(
            "lora_effective_chain", [{"name": "x", "strength_model": 1.0}]),
    }
    for label, mutate in cases.items():
        try:
            v2.parse_imprint_json(broken(mutate))
        except v2.SayaCoupleImprintError:
            pass
        except Exception as error:
            c.failures.append(f"{label}: raised {type(error).__name__} instead of SayaCoupleImprintError")
        else:
            c.failures.append(f"{label}: accepted")

    legacy = json.loads(v1.canonical_imprint_json(v1.build_imprint(
        main_prompt="m", person_1_prompt="p1", negative_prompt="n", direction="vertical", split=50, blur=0)))
    del legacy["main_prompt"]
    try:
        v1.parse_imprint_json(json.dumps(legacy))
    except v1.SayaCoupleImprintError:
        pass
    except Exception as error:
        c.failures.append(f"v1 missing key: raised {type(error).__name__}")
    else:
        c.failures.append("v1 missing key: accepted")
    return c.report()


TESTS = (
    test_compose_truth_table,
    test_node_encoded_texts,
    test_masks,
    test_p2_absent,
    test_solo_global,
    test_hard_errors,
    test_imprint_identities_not_compared,
    test_registry_and_signature,
    test_malformed_imprint_is_a_clean_error,
)

"""SayaCoupleImprintResolve: walking the source_file chain (transport only)."""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

from harness import Check, load_pack


def _mods():
    load_pack()
    from saya_couple.src.nodes import couple_imprint_resolve as resolve
    from saya_couple.src.nodes import couple_imprint_v2 as v2
    from saya_couple.src.services import imprint_integrity as integrity

    return resolve, v2, integrity


def _imprint(v2, main="scene"):
    imprint = v2.build_imprint_v2(
        prompts=v2.build_prompts(main=main, person_1="left", person_2="right", negative="bad"),
        geometry=v2.build_two_region_geometry(
            orientation=v2.ORIENTATION_LEFT_RIGHT,
            reference_width=64,
            reference_height=64,
            feather_unit="axis_fraction",
        ),
        strengths=v2.build_strengths(),
        reconstruction_recipe=v2.build_reconstruction_recipe(
            checkpoint_identity=v2.build_identity(role=v2.ROLE_PHASE_MODEL, identifier="base.safetensors"),
            clip_identity=v2.build_identity(role=v2.ROLE_BASE_CLIP, identifier="base.safetensors"),
        ),
        provenance=v2.build_provenance(
            workflow_version="test",
            pack_discriminant="sha256:test",
            created_at="2026-09-23T00:00:00+00:00",
        ),
    )
    return v2.canonical_imprint_json(imprint)


def _write(directory: Path, name: str, phase: int, source: str, *, imprint=None,
           pointer_for=None, transaction="tx-{n}", sidecar=True, embed=True,
           sidecar_transaction=None, raw_embedded=None, raw_sidecar=None) -> Path:
    """Write a real PNG (+ sidecar); each checkpoint has ITS OWN transaction."""
    _, _, integrity = _mods()
    from PIL import Image, PngImagePlugin

    manifest = {
        "phase": phase,
        "transaction_uuid": transaction.format(n=name),
        "source_file": source,
        "status": "validated",
    }
    if pointer_for is not None:
        manifest = integrity.attach_integrity_pointer(manifest, pointer_for)
    info = PngImagePlugin.PngInfo()
    if raw_embedded is not None:
        info.add_text("saya_phase_manifest", raw_embedded)
    elif embed:
        info.add_text("saya_phase_manifest", json.dumps(manifest))
    if imprint is not None:
        info.add_text("saya_couple_imprint", imprint)
    path = directory / f"{name}.png"
    Image.new("RGB", (4, 4)).save(path, pnginfo=info)
    if sidecar:
        side = dict(manifest)
        if sidecar_transaction is not None:
            side["transaction_uuid"] = sidecar_transaction
        text = raw_sidecar if raw_sidecar is not None else json.dumps(side)
        path.with_suffix(".json").write_text(text, encoding="utf-8")
    return path


def _phase1(directory, v2, name="phase_1", imprint_json=None, pointer_for=None):
    payload = imprint_json if imprint_json is not None else _imprint(v2)
    return _write(directory, name, 1, "", imprint=payload,
                  pointer_for=payload if pointer_for is None else pointer_for)


def _expect(c, resolve, fn, needle, label):
    try:
        fn()
    except resolve.SayaCoupleImprintResolveError as error:
        message = str(error)
        c.ok(needle in message, f"{label}: message {message!r} lacks {needle!r}")
        return message
    except Exception as error:
        c.failures.append(f"{label}: raised {type(error).__name__}, want SayaCoupleImprintResolveError")
        return ""
    c.failures.append(f"{label}: no exception")
    return ""


def test_happy_chain():
    resolve, v2, _ = _mods()
    c = Check("resolve_happy_chain")
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        p1 = _phase1(d, v2)
        p2 = _write(d, "phase_2", 2, str(p1))
        p3 = _write(d, "phase_3_in", 3, str(p2))
        result = resolve.resolve_imprint(str(p3))
        c.eq(len(result.chain), 3, "chain length")
        c.eq(result.root, str(p1.resolve()), "root is phase 1")
        c.eq(result.chain[0], str(p3.resolve()), "chain starts at start")
        c.eq(result.imprint_json, _imprint(v2), "canonical imprint returned")
    return c.report()


def test_direct_hit():
    resolve, v2, _ = _mods()
    c = Check("resolve_direct_hit")
    with tempfile.TemporaryDirectory() as tmp:
        p1 = _phase1(Path(tmp), v2)
        result = resolve.resolve_imprint(str(p1))
        c.eq(len(result.chain), 1, "chain length 1")
        c.eq(result.root, str(p1.resolve()), "root is start")
    return c.report()


def test_hard_errors():
    resolve, v2, _ = _mods()
    c = Check("resolve_hard_errors")
    run = resolve.resolve_imprint
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        good = _phase1(d, v2, "good")
        _expect(c, resolve, lambda: run(""), "empty path", "empty path")
        _expect(c, resolve, lambda: run(str(d / "nope.png")), "source file missing", "missing file")

        bare = _write(d, "bare", 2, str(good), embed=False)
        m = _expect(c, resolve, lambda: run(str(bare)), "saya_phase_manifest", "no embedded manifest")
        c.ok("bare.png" in m, "no embedded manifest: names path")

        nosidecar = _write(d, "nosidecar", 2, str(good), sidecar=False)
        _expect(c, resolve, lambda: run(str(nosidecar)), "invalid manifest", "missing sidecar")

        mismatch = _write(d, "mismatch", 2, str(good), sidecar_transaction="other")
        _expect(c, resolve, lambda: run(str(mismatch)), "transaction_uuid", "transaction mismatch")

        notjson = _write(d, "notjson", 2, str(good), raw_embedded="{not json")
        _expect(c, resolve, lambda: run(str(notjson)), "invalid manifest", "embedded manifest not JSON")
        badside = _write(d, "badside", 2, str(good), raw_sidecar="{not json")
        _expect(c, resolve, lambda: run(str(badside)), "invalid manifest", "sidecar not JSON")
        listside = _write(d, "listside", 2, str(good), raw_sidecar="[]")
        _expect(c, resolve, lambda: run(str(listside)), "invalid manifest", "sidecar not an object")

        empty_src = _write(d, "empty_src", 2, "")
        _expect(c, resolve, lambda: run(str(empty_src)), "source_file absent", "empty source_file")
        nonstr = _write(d, "nonstr", 2, None)
        _expect(c, resolve, lambda: run(str(nonstr)), "source_file absent", "non-str source_file")

        dangling = _write(d, "dangling", 2, str(d / "ghost.png"))
        _expect(c, resolve, lambda: run(str(dangling)), "source_file invalid/missing", "dangling source_file")

        a = d / "cyc_a.png"
        b = d / "cyc_b.png"
        _write(d, "cyc_a", 2, str(b.resolve()))
        _write(d, "cyc_b", 2, str(a.resolve()))
        m = _expect(c, resolve, lambda: run(str(a)), "cycle in source chain", "cycle")
        c.ok("cyc_b.png" in m, "cycle: lists chain")

        root = _write(d, "root_noimp", 1, "")
        mid = _write(d, "mid", 2, str(root))
        _expect(c, resolve, lambda: run(str(mid)), "no imprint found up to chain root", "root without imprint")

        v1 = json.dumps({"version": 1, "prompts": {}})
        v1png = _write(d, "v1png", 1, "", imprint=v1, pointer_for=v1)
        _expect(c, resolve, lambda: run(str(v1png)), "invalid/unsupported imprint", "v1 imprint")

        mis = _write(d, "integ", 1, "", imprint=_imprint(v2, "scene"), pointer_for=_imprint(v2, "other"))
        _expect(c, resolve, lambda: run(str(mis)), "integrity", "integrity mismatch")
        nopointer = _write(d, "nopointer", 1, "", imprint=_imprint(v2))
        _expect(c, resolve, lambda: run(str(nopointer)), "integrity", "integrity pointer absent")

        # chain too long
        previous = good
        for i in range(resolve.MAX_CHAIN_LENGTH + 1):
            previous = _write(d, f"long_{i}", 2, str(previous))
        _expect(c, resolve, lambda: run(str(previous)), "too long", "chain cap")
    return c.report()


def test_node_matches_load():
    resolve, v2, _ = _mods()
    from saya_couple.src.nodes.couple_reconstruct import read_imprint_from_png

    c = Check("resolve_node_matches_load")
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        p1 = _phase1(d, v2)
        p2 = _write(d, "phase_2", 2, str(p1))
        node = resolve.SayaCoupleImprintResolve()
        imprint, imprint_json = node.resolve(str(p2))
        c.eq(imprint, read_imprint_from_png(str(p1)), "same imprint as read_imprint_from_png")
        c.eq(imprint_json, v2.canonical_imprint_json(imprint), "canonical json")
        c.raises(resolve.SayaCoupleImprintResolveError, lambda: node.resolve(""), "empty path via node")
    c.eq(node.RETURN_TYPES, ("SAYA_IMPRINT", "STRING"), "return types")
    c.eq(node.RETURN_NAMES, ("imprint", "imprint_json"), "return names")
    c.eq(list(node.INPUT_TYPES()["required"]), ["checkpoint_path"], "single input")
    return c.report()


def test_registry_keys():
    pack = load_pack()
    resolve, _, _ = _mods()
    c = Check("resolve_registry_keys")
    c.ok(pack.NODE_CLASS_MAPPINGS.get("SayaCoupleImprintResolve") is resolve.SayaCoupleImprintResolve,
         "class registered")
    c.eq(pack.NODE_DISPLAY_NAME_MAPPINGS.get("SayaCoupleImprintResolve"),
         "Saya Couple Imprint · Resolve (ancestry)", "display name")
    return c.report()


TESTS = (test_happy_chain, test_direct_hit, test_hard_errors, test_node_matches_load, test_registry_keys)

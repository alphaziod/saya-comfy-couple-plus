"""P-A (2026-10-05): SAYA_COUPLE_CONTEXT carries exactly what the historic chain produced (Resolve/Load +
CheckpointIdentities + Retarget), SayaCoupleReconstruct gives the same result from a context, the imprint is parsed
once per phase, Solo works, errors are explicit, the context is never mutated, and it can be inspected."""

from __future__ import annotations

import json
import os
import tempfile
import time
from pathlib import Path

from harness import Check, load_pack
from test_conditioning_cache import _Env
from test_imprint_resolve import _phase1, _write
from test_multi_couple import _FakeClip, _FakeModel


def _mods():
    load_pack()
    import torch
    from saya_couple.src.nodes import conditioning_cache as cache
    from saya_couple.src.nodes import couple_context as cc
    from saya_couple.src.nodes import couple_imprint_resolve as resolve
    from saya_couple.src.nodes import couple_imprint_v2 as v2
    from saya_couple.src.nodes import couple_reconstruct as cr
    return torch, cache, cc, resolve, v2, cr


def _same_conds(torch, a, b) -> bool:
    if type(a) is not type(b):
        return False
    if torch.is_tensor(a):
        return torch.equal(a, b)
    if isinstance(a, dict):
        return a.keys() == b.keys() and all(_same_conds(torch, a[k], b[k]) for k in a)
    if isinstance(a, (list, tuple)):
        return len(a) == len(b) and all(_same_conds(torch, x, y) for x, y in zip(a, b))
    return a == b


def _patch_shape(model):
    """Which patch families a reconstructed model carries (functions are not comparable, their layout is)."""
    options = model.model_options.get("transformer_options", {})
    patches = {k: len(v) for k, v in options.get("patches", {}).items()}
    replace = {k: sorted(map(str, v)) for k, v in options.get("patches_replace", {}).items()}
    return patches, replace


def _fake_apply(model, *args):
    def hook(*a, **k):
        del a, k
    hook.__module__ = "saya_couple.src.nodes.saya_attention_couple"
    patched = model.clone()
    patches = patched.model_options.setdefault("transformer_options", {}).setdefault("patches", {})
    patches.setdefault("attn2_patch", []).append(hook)
    patches.setdefault("attn2_output_patch", []).append(hook)
    return patched


def test_context_equals_historic_chain():
    torch, cache, cc, resolve, v2, cr = _mods()
    c = Check("context_equals_historic_chain")
    canon = lambda hub: json.dumps(hub, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        p1 = _phase1(root, v2)                       # imprint identity = base.safetensors
        p2 = _write(root, "phase_2", 2, str(p1))     # a Phase 2 checkpoint pointing at it

        # --- historic chain ---------------------------------------------------------------------
        imprint, imprint_json = resolve.SayaCoupleImprintResolve().resolve(str(p2))
        ids_1 = cr.SayaCoupleCheckpointIdentities().pack("base.safetensors", "base.safetensors", "other.safetensors")[0]
        ids_2 = cr.SayaCoupleCheckpointIdentities().pack("other.safetensors", "other.safetensors", "base.safetensors")[0]
        json_2, ids_2n = cr.SayaCoupleImprintRetarget().retarget(imprint_json, ids_2)

        # --- context ----------------------------------------------------------------------------
        ctx_1, ctx_2, imp, js, report = cc.SayaCoupleContextLoad().load(str(p2), "base.safetensors", "other.safetensors")
        c.eq(ctx_1["imprint_json"], imprint_json, "context_model_1 imprint == Resolve output (identity already matches: no retarget)")
        c.eq(canon(ctx_1["identities"]), ids_1, "context_model_1 identities == CheckpointIdentities(model_1, model_1, model_2)")
        c.eq(ctx_2["imprint_json"], json_2, "context_model_2 imprint == Retarget output")
        c.eq(canon(ctx_2["identities"]), ids_2n, "context_model_2 identities == Retarget's normalized hub")
        c.ok(not ctx_1["retarget"]["applied"] and ctx_2["retarget"]["applied"], "retarget flagged only for model_2")
        c.eq((js, imp), (imprint_json, imprint), "imprint / imprint_json outputs == Resolve outputs (other consumers unchanged)")
        c.eq(ctx_1["source"]["chain"], [str(p2.resolve()), str(p1.resolve())], "source chain recorded")
        c.eq((ctx_1["phase"], ctx_1["target"], ctx_2["target"]), (2, "model_1", "model_2"), "phase and targets")
        c.ok("parsed and validated once" in report, "report")

        # identities_json alternative (Phase 6 routing hands a ready hub list)
        ctx_sel = cc.SayaCoupleContextLoad().load(str(p2), identities_json=ids_2)[0]
        c.eq((ctx_sel["imprint_json"], canon(ctx_sel["identities"])), (json_2, ids_2n), "identities_json: same context as model_2 (target label aside)")
        c.ok(cc.SayaCoupleContextLoad().load(str(p1), "base.safetensors")[1] is None, "no model_2: context_model_2 is None")
        c.raises(cc.SayaCoupleContextError, lambda: cc.SayaCoupleContextLoad().load(str(p2)), "neither identifier nor hub list: error")
        c.raises(cc.SayaCoupleContextError, lambda: cc.SayaCoupleContextLoad().load(str(p2), "base.safetensors", identities_json=ids_1), "both: error")
        c.raises(cr.SayaCoupleReconstructError, lambda: cc.SayaCoupleContextLoad().load(str(p2), "wrong.safetensors", "other.safetensors"),
                 "a model_1 that is not the imprint's: the hard identity check fires in the loader")

        # --- SayaCoupleReconstruct: historic inputs vs context, model_1 and model_2, Couple and Solo ----
        original_apply = cr._apply_couple_patch
        cr._apply_couple_patch = _fake_apply
        try:
            with _Env(torch, cache, cr) as env:
                image = torch.zeros(1, 64, 64, 3)
                old = lambda ids, **kw: cr.SayaCoupleReconstruct().reconstruct(model=_FakeModel(), clip=_FakeClip(), image=image, checkpoint_identities=ids, **kw)
                new = lambda ctx, **kw: cr.SayaCoupleReconstruct().reconstruct(model=_FakeModel(), clip=_FakeClip(), image=image, context=ctx, **kw)
                before = json.dumps(ctx_1, sort_keys=True, default=str)
                for label, o, n in (("model_1", old(ids_1, imprint=imprint), new(ctx_1)),
                                    ("model_2", old(ids_2n, imprint_json=json_2), new(ctx_2)),
                                    ("model_1 solo", old(ids_1, imprint=imprint, solo=True), new(ctx_1, solo=True))):
                    c.ok(_same_conds(torch, o[1], n[1]) and _same_conds(torch, o[2], n[2]), f"{label}: positive / negative identical")
                    c.eq(_patch_shape(n[0]), _patch_shape(o[0]), f"{label}: same patches on model_patched")
                    c.eq(_patch_shape(n[4]) if n[4] is not None else None, _patch_shape(o[4]) if o[4] is not None else None, f"{label}: same patches on model_patched_multimask")
                    tail = lambda r: [l for l in r.splitlines() if not l.startswith(("imprint:", "checkpoint_identity:", "clip_identity:"))]
                    c.eq(tail(n[3]), tail(o[3]), f"{label}: same report (apart from the provenance lines)")
                    c.ok("from context" in n[3], f"{label}: report says the imprint came from the context")
                c.eq(json.dumps(ctx_1, sort_keys=True, default=str), before, "the context is never mutated by Reconstruct")
                c.raises(cr.SayaCoupleReconstructError, lambda: old(ids_1, imprint=imprint, context=ctx_1), "context + historic inputs together: error")
                c.raises(cr.SayaCoupleReconstructError, lambda: new(None), "no context and no imprint: error")
                c.raises(cc.SayaCoupleContextError, lambda: new({"schema": "nope"}), "a foreign dict is refused")
        finally:
            cr._apply_couple_patch = original_apply

        # --- one parse per phase: count parse/validate calls, historic chain vs context (2 reconstructs) ----
        counts = {"parse": 0, "validate": 0}
        real_parse, real_validate = v2.parse_imprint_json, v2.validate_imprint_v2

        def counted_parse(payload):
            counts["parse"] += 1
            return real_parse(payload)

        def counted_validate(data, context="imprint"):
            counts["validate"] += 1
            return real_validate(data, context)

        def install():
            for module in (v2, resolve, cr, cc):
                if hasattr(module, "parse_imprint_json"):
                    module.parse_imprint_json = counted_parse
                if hasattr(module, "validate_imprint_v2"):
                    module.validate_imprint_v2 = counted_validate

        def uninstall():
            for module in (v2, resolve, cr, cc):
                if hasattr(module, "parse_imprint_json"):
                    module.parse_imprint_json = real_parse
                if hasattr(module, "validate_imprint_v2"):
                    module.validate_imprint_v2 = real_validate

        cr._apply_couple_patch = _fake_apply
        try:
            with _Env(torch, cache, cr):
                image = torch.zeros(1, 64, 64, 3)

                def historic():
                    imprint_, json_ = resolve.SayaCoupleImprintResolve().resolve(str(p2))
                    i1 = cr.SayaCoupleCheckpointIdentities().pack("base.safetensors", "base.safetensors", "other.safetensors")[0]
                    i2 = cr.SayaCoupleCheckpointIdentities().pack("other.safetensors", "other.safetensors", "base.safetensors")[0]
                    j2, i2n = cr.SayaCoupleImprintRetarget().retarget(json_, i2)
                    cr.SayaCoupleReconstruct().reconstruct(model=_FakeModel(), clip=_FakeClip(), image=image, checkpoint_identities=i1, imprint=imprint_)
                    cr.SayaCoupleReconstruct().reconstruct(model=_FakeModel(), clip=_FakeClip(), image=image, checkpoint_identities=i2n, imprint_json=j2)

                def with_context():
                    a, b, *_ = cc.SayaCoupleContextLoad().load(str(p2), "base.safetensors", "other.safetensors")
                    cr.SayaCoupleReconstruct().reconstruct(model=_FakeModel(), clip=_FakeClip(), image=image, context=a)
                    cr.SayaCoupleReconstruct().reconstruct(model=_FakeModel(), clip=_FakeClip(), image=image, context=b)

                install()
                try:
                    counts.update(parse=0, validate=0); historic(); old_counts = dict(counts)
                    counts.update(parse=0, validate=0); with_context(); new_counts = dict(counts)
                finally:
                    uninstall()
                c.ok(new_counts["parse"] + new_counts["validate"] < old_counts["parse"] + old_counts["validate"],
                     f"fewer parse/validate calls per phase: historic {old_counts} -> context {new_counts}")
                c.eq(new_counts["parse"], 1, "the imprint JSON is parsed exactly once per phase with a context")
                for fn in (historic, with_context):
                    fn()  # warm (disk cache)
                timings = {}
                for name, fn in (("historic", historic), ("context", with_context)):
                    t = time.perf_counter()
                    for _ in range(20):
                        fn()
                    timings[name] = (time.perf_counter() - t) / 20
                c.ok(timings["context"] <= timings["historic"] * 1.25, f"CPU per phase (2 reconstructs, fake encode): historic {timings['historic']*1000:.1f} ms, context {timings['context']*1000:.1f} ms")
                measures = os.environ.get("SAYA_MEASURES_DIR")  # maintainer only: where to drop the measure file
                if measures:
                    Path(measures).mkdir(parents=True, exist_ok=True)
                    Path(measures, "pa_context_cpu.json").write_text(json.dumps(
                        {"parse_validate_calls": {"historic": old_counts, "context": new_counts}, "cpu_ms_per_phase": {k: round(v * 1000, 2) for k, v in timings.items()}}, indent=1))
        finally:
            cr._apply_couple_patch = original_apply

        # --- inspect --------------------------------------------------------------------------------
        text = cc.describe_context(ctx_2)
        c.ok("target model_2" in text and "retarget: applied" in text and "other.safetensors" in text and "ownership_map: none" in text and str(p1.resolve()) in text,
             f"describe_context: target, retarget, identities, chain, map: {text[:160]!r}")
        out = cc.SayaCoupleContextInspect().inspect(ctx_2, full_imprint_json=True)
        c.ok(out["result"][0].endswith(ctx_2["imprint_json"]) and out["result"][1] == ctx_2["imprint_json"], "Inspect node: summary + imprint_json")
    return c.report()


TESTS = (test_context_equals_historic_chain,)

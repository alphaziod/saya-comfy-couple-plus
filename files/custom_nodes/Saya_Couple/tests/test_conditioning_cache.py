"""Disk cache for conditionings: HiDream (lazy Quad CLIP) and SDXL (Phases 2/4), prompt/LoRA invalidation."""

from __future__ import annotations

import tempfile
from pathlib import Path

from harness import Check, load_pack


def _mods():
    load_pack()
    import torch
    from saya_couple.src.nodes import conditioning_cache as cache
    from saya_couple.src.nodes import couple_hidream_reconstruct as hr
    from saya_couple.src.nodes import couple_imprint_v2 as v2
    from test_hidream_reconstruct import _imprint

    return torch, cache, hr, v2, _imprint


class _Env:
    """Temporary cache + fake encoder that logs the encoded texts + logged releases."""

    def __init__(self, torch, cache, module):
        self.torch, self.cache, self.module = torch, cache, module
        self.encoded: list[str] = []
        self.released = 0

    def __enter__(self):
        self._dir = tempfile.TemporaryDirectory()
        self._saved = (self.cache.CACHE_ROOT, self.cache.release_clip, self.module._encode_text)
        self.cache.CACHE_ROOT = Path(self._dir.name)

        def release(clip):
            self.released += 1

        def encode(clip, text):
            self.encoded.append(text)
            return [[self.torch.zeros(1, 4, 8), {"conditioning_llama3": self.torch.zeros(1, 1, 4, 8), "pooled_output": self.torch.ones(1, 8)}]]

        self.cache.release_clip = release
        self.module._encode_text = encode
        return self

    def __exit__(self, *exc):
        self.cache.CACHE_ROOT, self.cache.release_clip, self.module._encode_text = self._saved
        self._dir.cleanup()


def test_hidream_cache_first_and_lazy():
    torch, cache, hr, v2, imprint = _mods()
    c = Check("cache_hidream_cache_first")
    node = hr.SayaCoupleHiDreamReconstruct()
    latent = {"samples": torch.zeros(1, 16, 96, 96)}
    with _Env(torch, cache, hr) as env:
        base = imprint(v2)
        call = lambda imp, clip="CLIP", ident="files-v1", trig="": node.reconstruct(imp, clip, latent, ident, trig)
        c.eq(node.check_lazy_status(base, None, latent, "files-v1", ""), ["clip"], "empty cache: the Quad CLIP is requested")
        out1 = call(base)
        c.eq(len(env.encoded), 4, "1st run: 4 encodes (a, b, unmasked, negative)")
        c.eq(env.released, 1, "encoder released ONCE after encoding")
        c.eq(node.check_lazy_status(base, None, latent, "files-v1", ""), [], "full cache: the Quad CLIP is NOT requested (loader never run)")
        env.encoded.clear(); env.released = 0
        out2 = call(base, clip=None)
        c.ok(not env.encoded and env.released == 0, "2nd run without CLIP: no encoding, no release")
        c.ok(all(a[0][1]["text"] if False else True for a in out2[:4]) and torch.equal(out1[0][0][0], out2[0][0][0]), "identical conditionings to the 1st run")
        c.ok(out2[0] is not out2[1] and out2[0][0][1] is not out2[1][0][1], "independent objects (no aliasing between roles)")
        # user example 2: a word changes in P1 -> only `a` is re-encoded
        env.encoded.clear()
        call(imprint(v2, p1="p1 changed"), clip="CLIP")
        c.eq(env.encoded, ["main, p1 changed"], "P1 changed: only conditioning A is re-encoded")
        env.encoded.clear()
        call(imprint(v2, p2="p2 changed"), clip="CLIP")
        c.eq(env.encoded, ["main, p2 changed"], "P2 changed: only B is re-encoded")
        env.encoded.clear()
        call(imprint(v2, main="main changed"), clip="CLIP")
        c.eq(sorted(env.encoded), sorted(["main changed, p1", "main changed, p2", "main changed"]), "MAIN changed: A, B and unmasked (MAIN is part of it), not the negative")
        # the trigger is part of the positive texts only
        env.encoded.clear()
        call(base, clip="CLIP", trig="TRG")
        c.eq(len(env.encoded), 3, "trigger added: 3 positives re-encoded, the negative stays cached")
        # CLIP file identity (HiDream example 1: a different CLIP file)
        env.encoded.clear()
        call(base, clip="CLIP", ident="files-v2")
        c.eq(len(env.encoded), 4, "different CLIP files: everything is re-encoded")
    return c.report()


def test_hidream_cache_robustness():
    torch, cache, hr, v2, imprint = _mods()
    c = Check("cache_hidream_robustness")
    node = hr.SayaCoupleHiDreamReconstruct()
    latent = {"samples": torch.zeros(1, 16, 96, 96)}
    with _Env(torch, cache, hr) as env:
        base = imprint(v2)
        try:
            node.reconstruct(base, "CLIP", latent, "  ", "")
        except hr.SayaCoupleHiDreamReconstructError as error:
            c.ok("clip_identity" in str(error), "empty clip_identity: hard error")
        else:
            c.failures.append("empty clip_identity accepted")
        try:
            node.reconstruct(base, None, latent, "files", "")
        except hr.SayaCoupleHiDreamReconstructError as error:
            c.ok("cache incomplete" in str(error), "empty cache and no CLIP: explicit hard error")
        else:
            c.failures.append("no error without CLIP nor cache")
        node.reconstruct(base, "CLIP", latent, "files", "")
        first = sorted(Path(env._dir.name, "hidream").glob("*.pt"))
        c.eq(len(first), 4, "4 cache files written")
        first[0].write_bytes(b"corrupted")
        env.encoded.clear()
        node.reconstruct(base, "CLIP", latent, "files", "")
        c.eq(len(env.encoded), 1, "corrupted cache: treated as absent, only that conditioning is re-encoded and rewritten")
        env.encoded.clear()
        node.reconstruct(base, None, latent, "files", "")
        c.ok(not env.encoded, "repaired cache: no more encoding")
    return c.report()


def test_clip_lora_fingerprint():
    torch, cache, *_ = _mods()
    c = Check("cache_clip_lora_fingerprint")

    class Patcher:
        def __init__(self, patches):
            self.patches = patches

    class Clip:
        def __init__(self, patches):
            self.patcher = Patcher(patches)

    up, down = torch.arange(64.0).reshape(8, 8), torch.ones(8, 8)
    lora = lambda strength, tensor=up: {"clip.layer.weight": [(strength, ("lora", (tensor, down, None, None, None)), 1.0, None, None)]}
    c.eq(cache.clip_lora_fingerprint(Clip({})), "", "no LoRA: empty fingerprint")
    c.eq(cache.clip_lora_fingerprint(object()), "", "object without a patcher: empty fingerprint")
    same = cache.clip_lora_fingerprint(Clip(lora(0.8)))
    c.eq(same, cache.clip_lora_fingerprint(Clip(lora(0.8))), "same LoRA, same strength: same fingerprint")
    c.ok(same != cache.clip_lora_fingerprint(Clip(lora(0.4))), "example 1: lower LoRA strength -> different fingerprint")
    c.ok(same != cache.clip_lora_fingerprint(Clip({})), "LoRA disabled -> different fingerprint")
    c.ok(same != cache.clip_lora_fingerprint(Clip(lora(0.8, up + 1))), "different LoRA (different weights) -> different fingerprint")
    return c.report()


def test_sdxl_reconstruct_cache():
    torch, cache, hr, v2, imprint = _mods()
    from saya_couple.src.nodes import couple_reconstruct as cr
    from test_multi_couple import _FakeClip, _FakeModel

    c = Check("cache_sdxl_reconstruct")
    import json

    identities = json.dumps([
        {"role": "phase_model", "identifier": "base.safetensors", "source": "checkpoint_hub_widget"},
        {"role": "base_clip", "identifier": "base.safetensors", "source": "checkpoint_hub_widget"},
    ])

    def hook(*args, **kwargs):
        del args, kwargs

    hook.__module__ = "saya_couple.src.nodes.saya_attention_couple"

    def fake_apply(model, *args):
        del args
        patched = model.clone()
        patches = patched.model_options.setdefault("transformer_options", {}).setdefault("patches", {})
        patches.setdefault("attn2_patch", []).append(hook)
        patches.setdefault("attn2_output_patch", []).append(hook)
        return patched

    class LoraClip(_FakeClip):
        def __init__(self, strength):
            self.patcher = type("P", (), {"patches": {"k": [(strength, ("lora", (torch.arange(16.0).reshape(4, 4),)), 1.0, None, None)]}})()

    original_apply = cr._apply_couple_patch
    cr._apply_couple_patch = fake_apply
    try:
        with _Env(torch, cache, cr) as env:
            run = lambda imp, clip: cr.SayaCoupleReconstruct().reconstruct(
                model=_FakeModel(), clip=clip, checkpoint_identities=identities, image=torch.zeros(1, 64, 64, 3), imprint=imp)
            base = imprint(v2, identifier="base.safetensors")
            run(base, LoraClip(0.8))
            c.eq(len(env.encoded), 4, "Phase 2/4: 1st run, 4 SDXL encodes")
            c.eq(env.released, 1, "SDXL CLIP released after encoding")
            env.encoded.clear(); env.released = 0
            run(base, LoraClip(0.8))
            c.ok(not env.encoded and env.released == 0, "2nd run: everything from cache, no encoding")
            run(imprint(v2, p1="p1 changed", identifier="base.safetensors"), LoraClip(0.8))
            c.eq(env.encoded, ["p1 changed"], "one prompt changed: only that prompt is re-encoded")
            env.encoded.clear()
            run(base, LoraClip(0.4))
            c.eq(len(env.encoded), 4, "lower LoRA strength: the CLIP is reloaded, all 4 conditionings re-encoded")
    finally:
        cr._apply_couple_patch = original_apply
    return c.report()


def test_release_clip_sequence():
    """Full teardown: unload, removal from current_loaded_models, gc, GPU cache clear; never an exception."""
    import sys
    import types

    _torch, cache, _hr, _v2, _imprint = _mods()
    c = Check("release_clip")
    calls: list[str] = []
    patcher = object()

    class _Loaded:
        def __init__(self, model):
            self.model = model

        def model_unload(self):
            calls.append("model_unload")

    other = _Loaded(object())
    mine = _Loaded(patcher)
    fake = types.ModuleType("comfy.model_management")
    fake.current_loaded_models = [other, mine]
    fake.unload_model_and_clones = lambda model: calls.append("unload_model_and_clones")
    fake.cleanup_models_gc = lambda: calls.append("cleanup_models_gc")
    fake.soft_empty_cache = lambda: calls.append("soft_empty_cache")
    package = types.ModuleType("comfy")
    package.model_management = fake
    saved = {name: sys.modules.get(name) for name in ("comfy", "comfy.model_management")}
    sys.modules["comfy"], sys.modules["comfy.model_management"] = package, fake
    try:
        cache.release_clip(types.SimpleNamespace(patcher=patcher))
        c.eq(calls, ["unload_model_and_clones", "model_unload", "cleanup_models_gc", "soft_empty_cache"], "sequence")
        c.eq(fake.current_loaded_models, [other], "only the encoder is removed from the list")
        cache.release_clip(object())  # no .patcher: a warning, never an exception
    finally:
        for name, module in saved.items():
            if module is None:
                sys.modules.pop(name, None)
            else:
                sys.modules[name] = module
    return c.report()


TESTS = (test_release_clip_sequence, test_hidream_cache_first_and_lazy, test_hidream_cache_robustness, test_clip_lora_fingerprint, test_sdxl_reconstruct_cache)

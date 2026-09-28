"""SayaDuoSegsDetail: saya_couple_crop plumbing (real SEGS crop position, not a
full-frame squash) — the fix for the confirmed Detailer regional-attention bug.

Impact Pack is stubbed (sys.modules["impact.core"], sys.modules["impact.hooks"])
with the minimal real contract this pack depends on, so these tests run without
the actual ComfyUI-Impact-Pack install (mirrors test_release_clip_sequence's
comfy.model_management stub in test_conditioning_cache.py).
"""

from __future__ import annotations

import sys
import types
from typing import Any

from harness import Check, load_pack


class _Seg:
    """Minimal stand-in for Impact Pack's SEG namedtuple (the fields this
    module reads: crop_region, bbox, cropped_mask)."""

    def __init__(self, crop_region, bbox, cropped_mask_all_zero=False):
        self.crop_region = crop_region
        self.bbox = bbox
        self.cropped_mask = _FakeMask(cropped_mask_all_zero)


class _FakeMask:
    """``(mask == 0).all()`` without a real tensor/ndarray dependency."""

    def __init__(self, all_zero: bool):
        self._all_zero = all_zero

    def __eq__(self, other):
        return self

    def all(self):
        return self

    def item(self):
        return self._all_zero

    def __bool__(self):
        return self._all_zero


class _FakeModel:
    """Minimal ModelPatcher double: model_options clone independent of the original."""

    def __init__(self):
        self.model_options: dict[str, Any] = {}

    def clone(self):
        new = _FakeModel.__new__(_FakeModel)
        new.model_options = {k: (dict(v) if isinstance(v, dict) else v) for k, v in self.model_options.items()}
        return new


class _FakeImage:
    """``.shape`` only — [B,H,W,C], matching what `_predicted_processed_segs`/canvas reads."""

    def __init__(self, height, width):
        self.shape = (1, height, width, 3)


def _install_impact_stub(segs_scale_match=None):
    """Install minimal impact.core / impact.hooks stand-ins mirroring the real
    ComfyUI-Impact-Pack contract this module depends on. Returns the restore
    callback (call it in a finally: block)."""
    saved = {name: sys.modules.get(name) for name in ("impact", "impact.core", "impact.hooks", "impact.impact_pack")}

    impact_pkg = types.ModuleType("impact")
    impact_core = types.ModuleType("impact.core")
    impact_hooks = types.ModuleType("impact.hooks")
    impact_pack = types.ModuleType("impact.impact_pack")

    class DetailerForEach:
        """Records the hook ``do_detail`` receives (positional arg 20)."""

        calls: list[Any] = []

        @staticmethod
        def do_detail(*args, **kwargs):
            DetailerForEach.calls.append(args[20])
            return (args[0],)

    impact_pack.DetailerForEach = DetailerForEach

    impact_core.segs_scale_match = segs_scale_match or (lambda segs, shape: segs)

    class DetailerHook:
        def touch_scaled_size(self, w, h):
            return w, h

        def pre_ksample(self, model, seed, steps, cfg, sampler_name, scheduler,
                         positive, negative, upscaled_latent, denoise):
            return model, seed, steps, cfg, sampler_name, scheduler, positive, negative, upscaled_latent, denoise

    class DetailerHookCombine(DetailerHook):
        def __init__(self, hook1, hook2):
            self.hook1, self.hook2 = hook1, hook2

        def touch_scaled_size(self, w, h):
            w, h = self.hook1.touch_scaled_size(w, h)
            return self.hook2.touch_scaled_size(w, h)

        def pre_ksample(self, model, seed, steps, cfg, sampler_name, scheduler,
                         positive, negative, upscaled_latent, denoise):
            model, seed, steps, cfg, sampler_name, scheduler, positive, negative, upscaled_latent, denoise = \
                self.hook1.pre_ksample(model, seed, steps, cfg, sampler_name, scheduler,
                                       positive, negative, upscaled_latent, denoise)
            return self.hook2.pre_ksample(model, seed, steps, cfg, sampler_name, scheduler,
                                          positive, negative, upscaled_latent, denoise)

    impact_hooks.DetailerHook = DetailerHook
    impact_hooks.DetailerHookCombine = DetailerHookCombine

    sys.modules["impact"] = impact_pkg
    sys.modules["impact.core"] = impact_core
    sys.modules["impact.hooks"] = impact_hooks
    sys.modules["impact.impact_pack"] = impact_pack

    def restore():
        for name, module in saved.items():
            if module is None:
                sys.modules.pop(name, None)
            else:
                sys.modules[name] = module

    return restore


def _expected(regions, size=(64, 64)):
    """Predictions in the ``_predicted_processed_segs`` shape: (crop_region, scaled size)."""
    return [(region, size) for region in regions]


def _sample_pre_ksample(hook):
    """Drive one seg through the hook exactly like core.py's enhance_detail:
    touch_scaled_size(...) then pre_ksample(model, ...). Returns the model
    pre_ksample hands back."""
    hook.touch_scaled_size(64, 64)
    model, *_ = hook.pre_ksample(_FakeModel(), 0, 1, 1.0, "euler", "normal", [], [], {}, 1.0)
    return model


def test_detailer_crop_left_vs_right():
    load_pack()
    restore = _install_impact_stub()
    try:
        from saya_couple.src.nodes.detailer import _build_couple_crop_hook

        c = Check("detailer_crop_left_vs_right")
        left_region = [0, 0, 50, 100]
        right_region = [150, 0, 200, 100]
        hook_left = _build_couple_crop_hook(_expected([left_region]), (200, 100), None)
        hook_right = _build_couple_crop_hook(_expected([right_region]), (200, 100), None)

        model_left = _sample_pre_ksample(hook_left)
        model_right = _sample_pre_ksample(hook_right)

        crop_l = model_left.model_options["transformer_options"]["saya_couple_crop"]
        crop_r = model_right.model_options["transformer_options"]["saya_couple_crop"]
        c.eq(crop_l["crop_region"], [0.0, 0.0, 50.0, 100.0], "left crop: exact region attached")
        c.eq(crop_r["crop_region"], [150.0, 0.0, 200.0, 100.0], "right crop: exact region attached")
        c.eq((crop_l["full_width"], crop_l["full_height"]), (200, 100), "canvas size attached (left)")
        c.eq((crop_r["full_width"], crop_r["full_height"]), (200, 100), "canvas size attached (right)")
        c.ok(crop_l["crop_region"] != crop_r["crop_region"], "left and right crops are distinct")
        return c.report()
    finally:
        restore()


def test_detailer_crop_straddling_p1_p2():
    load_pack()
    restore = _install_impact_stub()
    try:
        from saya_couple.src.nodes.detailer import _build_couple_crop_hook
        from saya_couple.src.ppm_vendor.attention_couple.common import crop_mask_to_tile
        import torch

        c = Check("detailer_crop_straddling_p1_p2")
        # P1/P2 split at x=100 on a 200x100 canvas; the seg's crop straddles it.
        mask = torch.zeros(2, 1, 100, 200)
        mask[1, :, :, 100:] = 1.0  # channel 1 = P2 weight
        straddling_region = [80, 0, 120, 100]

        hook = _build_couple_crop_hook(_expected([straddling_region]), (200, 100), None)
        model = _sample_pre_ksample(hook)
        crop_meta = model.model_options["transformer_options"]["saya_couple_crop"]
        c.eq(crop_meta["crop_region"], [80.0, 0.0, 120.0, 100.0], "straddling crop region attached exactly")

        tile = crop_mask_to_tile(mask, crop_meta)
        p2_channel = tile[1]
        c.ok(float(p2_channel.min()) < 0.5 < float(p2_channel.max()),
             "the straddling tile still shows BOTH P1 (0.0, left half) and P2 (1.0, right half) — "
             "real geometry preserved, not squashed to a single value")
        return c.report()
    finally:
        restore()


def test_detailer_crop_successive_no_leak():
    load_pack()
    restore = _install_impact_stub()
    try:
        from saya_couple.src.nodes.detailer import _build_couple_crop_hook

        c = Check("detailer_crop_successive_no_leak")
        regions = [[0, 0, 40, 40], [40, 0, 80, 40], [80, 0, 120, 40], [120, 0, 160, 40]]
        hook = _build_couple_crop_hook(_expected(regions), (160, 40), None)

        seen = []
        for _ in regions:
            model = _sample_pre_ksample(hook)
            seen.append(tuple(model.model_options["transformer_options"]["saya_couple_crop"]["crop_region"]))

        c.eq(seen, [tuple(float(v) for v in r) for r in regions],
             "each successive seg gets its OWN crop, in order, no repeat/leak")
        c.ok(len(set(seen)) == len(seen), "no two segs share the same crop metadata")
        return c.report()
    finally:
        restore()


def test_detailer_crop_model1_model2_independent():
    """Two separate hook instances (Model 1 / Model 2 reconstructions) never
    share state — each ``_build_couple_crop_hook`` call is its own closure."""
    load_pack()
    restore = _install_impact_stub()
    try:
        from saya_couple.src.nodes.detailer import _build_couple_crop_hook

        c = Check("detailer_crop_model1_model2_independent")
        region_m1 = [[0, 0, 40, 40]]
        region_m2 = [[100, 0, 140, 40]]
        hook_m1 = _build_couple_crop_hook(_expected(region_m1), (200, 40), None)
        hook_m2 = _build_couple_crop_hook(_expected(region_m2), (200, 40), None)

        # Drive model 2's hook TWICE before touching model 1's — independent
        # position counters, no shared state between the two node instances.
        _sample_pre_ksample(hook_m2)
        m1 = _sample_pre_ksample(hook_m1)
        crop1 = m1.model_options["transformer_options"]["saya_couple_crop"]
        c.eq(crop1["crop_region"], [0.0, 0.0, 40.0, 40.0], "model 1's hook unaffected by model 2's hook calls")
        return c.report()
    finally:
        restore()


def test_detailer_crop_absent_metadata_is_safe():
    """No predicted crops (e.g. detection/prediction failed) -> the model
    passes through unmodified, chained hook still runs, nothing crashes."""
    load_pack()
    restore = _install_impact_stub()
    try:
        from saya_couple.src.nodes.detailer import _build_couple_crop_hook

        c = Check("detailer_crop_absent_metadata_is_safe")
        hook = _build_couple_crop_hook([], (200, 100), None)
        model_in = _FakeModel()
        model_in.model_options["marker"] = "untouched"
        hook.touch_scaled_size(64, 64)
        model_out, *_ = hook.pre_ksample(model_in, 0, 1, 1.0, "euler", "normal", [], [], {}, 1.0)
        c.ok(model_out is model_in, "no crop predicted: the SAME model object passes through (no clone, no crop key)")
        c.ok("saya_couple_crop" not in model_out.model_options.get("transformer_options", {}),
             "no saya_couple_crop key attached when nothing was predicted")
        return c.report()
    finally:
        restore()


def test_detailer_crop_chains_caller_hook():
    """An existing user-supplied detailer_hook still runs (DetailerHookCombine),
    and still receives the crop-patched model from the couple hook."""
    load_pack()
    restore = _install_impact_stub()
    try:
        from saya_couple.src.nodes.detailer import _build_couple_crop_hook

        c = Check("detailer_crop_chains_caller_hook")
        seen_models = []

        class _RecordingHook:
            def touch_scaled_size(self, w, h):
                return w, h

            def pre_ksample(self, model, seed, steps, cfg, sampler_name, scheduler,
                             positive, negative, upscaled_latent, denoise):
                seen_models.append(model)
                return model, seed, steps, cfg, sampler_name, scheduler, positive, negative, upscaled_latent, denoise

        hook = _build_couple_crop_hook(_expected([[10, 10, 50, 50]], (32, 32)), (100, 100), _RecordingHook())
        hook.touch_scaled_size(32, 32)
        hook.pre_ksample(_FakeModel(), 0, 1, 1.0, "euler", "normal", [], [], {}, 1.0)

        c.eq(len(seen_models), 1, "the caller's hook ran exactly once")
        c.ok("saya_couple_crop" in seen_models[0].model_options.get("transformer_options", {}),
             "the caller's hook receives the ALREADY crop-patched model (couple hook runs first)")
        return c.report()
    finally:
        restore()


def test_detailer_predicted_segs_filters_empty_mask():
    load_pack()
    restore = _install_impact_stub()
    try:
        from saya_couple.src.nodes.detailer import _predicted_processed_segs

        c = Check("detailer_predicted_segs_filters_empty_mask")
        kept = _Seg([0, 0, 40, 40], [0, 0, 40, 40], cropped_mask_all_zero=False)
        empty = _Seg([40, 0, 80, 40], [40, 0, 80, 40], cropped_mask_all_zero=True)
        segs = ((40, 80), [kept, empty])
        image = _FakeImage(40, 80)

        result = _predicted_processed_segs(segs, image, guide_size=512, guide_size_for_bbox=True,
                                           max_size=1024, force_inpaint=True)
        c.eq(result, [([0, 0, 40, 40], (512, 512))], "the all-zero-mask seg is excluded, the real one is kept with its scaled size")
        return c.report()
    finally:
        restore()


def test_detailer_predicted_segs_skips_large_bbox_without_force_inpaint():
    load_pack()
    restore = _install_impact_stub()
    try:
        from saya_couple.src.nodes.detailer import _predicted_processed_segs

        c = Check("detailer_predicted_segs_skips_large_bbox_without_force_inpaint")
        # bbox already >= guide_size on both axes, force_inpaint OFF: Impact Pack skips it.
        big = _Seg([0, 0, 600, 600], [0, 0, 600, 600], cropped_mask_all_zero=False)
        segs = ((600, 600), [big])
        image = _FakeImage(600, 600)

        result = _predicted_processed_segs(segs, image, guide_size=512, guide_size_for_bbox=True,
                                           max_size=1024, force_inpaint=False)
        c.eq(result, [], "a bbox already >= guide_size is skipped when force_inpaint is OFF")

        result_forced = _predicted_processed_segs(segs, image, guide_size=512, guide_size_for_bbox=True,
                                                   max_size=1024, force_inpaint=True)
        c.eq(result_forced, [([0, 0, 600, 600], (600, 600))], "the same seg is kept when force_inpaint is ON, at its own size (upscale <= 1)")
        return c.report()
    finally:
        restore()


class _Warnings:
    """Collects the warnings the detailer module logs (context manager)."""

    def __enter__(self):
        import logging

        self.records = []
        self._handler = logging.Handler()
        self._handler.emit = self.records.append
        self._logger = logging.getLogger("saya_couple.src.nodes.detailer")
        self._logger.addHandler(self._handler)
        return self.records

    def __exit__(self, *exc):
        self._logger.removeHandler(self._handler)


def test_detailer_crop_size_desync_disables_rest():
    """Impact's scaled size differs from the prediction: that seg AND every
    later seg get NO crop metadata (never a possibly-wrong region), one warning."""
    load_pack()
    restore = _install_impact_stub()
    try:
        from saya_couple.src.nodes.detailer import _build_couple_crop_hook

        c = Check("detailer_crop_size_desync_disables_rest")
        regions = [[0, 0, 40, 40], [40, 0, 80, 40], [80, 0, 120, 40]]
        hook = _build_couple_crop_hook(_expected(regions), (160, 40), None)
        with _Warnings() as warnings:
            first = _sample_pre_ksample(hook)
            hook.touch_scaled_size(32, 32)  # seg #1: size mismatch
            second, *_ = hook.pre_ksample(_FakeModel(), 0, 1, 1.0, "euler", "normal", [], [], {}, 1.0)
            third = _sample_pre_ksample(hook)  # seg #2 matches again, but mapping is no longer trusted

        c.eq(first.model_options["transformer_options"]["saya_couple_crop"]["crop_region"],
             [0.0, 0.0, 40.0, 40.0], "seg #0 (sizes agree) keeps its crop")
        c.ok("saya_couple_crop" not in second.model_options.get("transformer_options", {}),
             "desynced seg: no crop metadata")
        c.ok("saya_couple_crop" not in third.model_options.get("transformer_options", {}),
             "every later seg stays disabled for the rest of the call")
        c.eq(len(warnings), 1, "the desync is logged exactly once")
        return c.report()
    finally:
        restore()


def test_detailer_crop_extra_seg_is_desync():
    """Impact samples more segs than predicted: the extra seg gets nothing."""
    load_pack()
    restore = _install_impact_stub()
    try:
        from saya_couple.src.nodes.detailer import _build_couple_crop_hook

        c = Check("detailer_crop_extra_seg_is_desync")
        hook = _build_couple_crop_hook(_expected([[0, 0, 40, 40]]), (80, 40), None)
        with _Warnings() as warnings:
            _sample_pre_ksample(hook)
            extra = _sample_pre_ksample(hook)
        c.ok("saya_couple_crop" not in extra.model_options.get("transformer_options", {}),
             "unpredicted seg: no crop metadata")
        c.eq(len(warnings), 1, "desync logged once")
        return c.report()
    finally:
        restore()


def _run_detail(wildcard, detailer_hook=None):
    from saya_couple.src.nodes.detailer import SayaDuoSegsDetail
    from impact.impact_pack import DetailerForEach  # the stub

    DetailerForEach.calls.clear()
    segs = ((40, 80), [_Seg([0, 0, 40, 40], [0, 0, 40, 40])])
    SayaDuoSegsDetail().detail(
        _FakeImage(40, 80), segs, _FakeModel(), None, None, [], [],
        512, True, 1024, 0, 1, 1.0, "euler", "normal", 0.5, 5, True, True, wildcard,
        detailer_hook=detailer_hook,
    )
    return DetailerForEach.calls[-1]


def test_detailer_wildcard_gates_crop_metadata():
    """Empty wildcard: the crop hook is built. [ASC]/[DSC]/[SKIP]/any
    non-empty wildcard: the caller's detailer_hook passes through untouched
    (no positional crop metadata at all), with exactly one warning per call."""
    load_pack()
    restore = _install_impact_stub()
    try:
        c = Check("detailer_wildcard_gates_crop_metadata")
        with _Warnings() as warnings:
            hook = _run_detail("")
        c.ok(hook is not None and hasattr(hook, "_desynced"), "empty wildcard: crop hook built")
        c.eq(len(warnings), 0, "empty wildcard: no warning")

        caller_hook = object()
        for wildcard in ("[ASC] a", "[DSC] a", "[ASC] [SKIP][SEP]b", "[LAB]\n[x] [STOP]", "  face  "):
            with _Warnings() as warnings:
                passed_none = _run_detail(wildcard)
                passed_caller = _run_detail(wildcard, detailer_hook=caller_hook)
            c.ok(passed_none is None, f"{wildcard!r}: no crop hook injected")
            c.ok(passed_caller is caller_hook, f"{wildcard!r}: caller's detailer_hook passed through as-is")
            c.eq(len(warnings), 2, f"{wildcard!r}: one warning per detail() call")
        return c.report()
    finally:
        restore()


TESTS = (
    test_detailer_crop_left_vs_right,
    test_detailer_crop_straddling_p1_p2,
    test_detailer_crop_successive_no_leak,
    test_detailer_crop_model1_model2_independent,
    test_detailer_crop_absent_metadata_is_safe,
    test_detailer_crop_chains_caller_hook,
    test_detailer_predicted_segs_filters_empty_mask,
    test_detailer_predicted_segs_skips_large_bbox_without_force_inpaint,
    test_detailer_crop_size_desync_disables_rest,
    test_detailer_crop_extra_seg_is_desync,
    test_detailer_wildcard_gates_crop_metadata,
)

"""Regression tests for bugs found by the v0.3.0 deep audit that the rest of the
suite did not catch. Each test names the audit finding it guards."""

import tempfile
from pathlib import Path

import torch

from harness import Check, load_pack, sample_v2_imprint_json


def _services(tmp_path):
    load_pack()
    from saya_couple.src.services import image_phases as services

    services.output_directory = lambda: tmp_path
    return services


def _save_phase1(services, root, seed):
    services.save_candidate(
        images=torch.full((1, 32, 32, 3), seed / 1000.0), phase=1, detailer="none",
        checkpoint_root=root, source_path="", seed=seed, positive_prompt="pos",
        negative_prompt="neg", models=[], vaes=[], samplers={},
        imprint_json=sample_v2_imprint_json(),
    )
    return services.read_manifest(services.checkpoint_paths(1, root).candidate_manifest)


def test_validate_refuses_replaced_candidate():
    """A1-01: a second Phase-1 render must not be validated in place of the reviewed one."""
    c = Check("audit_validate_refuses_replaced_candidate")
    with tempfile.TemporaryDirectory() as tmp:
        services = _services(Path(tmp))
        root = "audit/checkpoints"
        reviewed = _save_phase1(services, root, 111)["transaction_uuid"]
        _save_phase1(services, root, 222)
        c.raises(ValueError, lambda: services.promote_candidate(1, "none", root, reviewed),
                 "promotion with the reviewed transaction refuses a replaced candidate")
        paths = services.checkpoint_paths(1, root)
        c.ok(not paths.validated_image.exists(), "nothing was validated")
        c.ok(paths.candidate_image.exists(), "the newer candidate is left for review")

        current = services.read_manifest(paths.candidate_manifest)["transaction_uuid"]
        validated = services.promote_candidate(1, "none", root, current)
        c.eq(validated["seed"], 222, "the matching transaction still promotes")
        c.eq(validated["transaction_uuid"], current, "validated carries the reviewed transaction")
    return c.report()


def test_validate_without_transaction_unchanged():
    """A1-01: callers that do not send a transaction keep the previous behaviour."""
    c = Check("audit_validate_without_transaction_unchanged")
    with tempfile.TemporaryDirectory() as tmp:
        services = _services(Path(tmp))
        root = "audit/checkpoints"
        _save_phase1(services, root, 7)
        c.eq(services.promote_candidate(1, "none", root)["seed"], 7, "no transaction: promotes")
    return c.report()


def test_couple_refuses_stock_core():
    """A4-01/A4-02: on a core without the Saya patch, Couple must fail loudly, not render MAIN only."""
    c = Check("audit_couple_refuses_stock_core")
    load_pack()
    from saya_couple.src.nodes import saya_dual_attention as dual

    c.eq(dual.core_patch_missing(), None, "patched core is accepted")
    # Another pack wrapping forward at import (ComfyUI-Easy-Use does) must not look like a stock core.
    block = dual.attention.BasicTransformerBlock
    original_forward = block.forward
    block.forward = lambda self, *a, **k: original_forward(self, *a, **k)
    try:
        c.eq(dual.core_patch_missing(), None, "patched core with a wrapped forward is accepted")
    finally:
        block.forward = original_forward
    live = dual.attention
    from test_core_saya_dual import _head_module

    dual.attention = _head_module()
    try:
        c.ok(dual.core_patch_missing() is not None, "stock core is detected")
        mask = torch.ones(8, 8)
        conds = [[[torch.rand(1, 4, 8), {}]] for _ in range(3)]
        c.raises(RuntimeError, lambda: dual.enable_dual_attention(object(), *conds, mask, mask),
                 "enable_dual_attention raises before touching the model")
    finally:
        dual.attention = live
    return c.report()


def _engine_in(directory, crop_source):
    """A fake USDU engine loaded from `directory`, then dropped from sys.modules like the real pack does."""
    import importlib.util
    import sys

    (directory / "crop_model_patch.py").write_text(crop_source)
    (directory / "usdu_nodes.py").write_text(
        "class UltimateSDUpscaleCustomSample:\n"
        "    def upscale(self, **kwargs):\n"
        "        return ('image',)\n"
    )
    spec = importlib.util.spec_from_file_location("_audit_fake_usdu_nodes", directory / "usdu_nodes.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    del sys.modules[spec.name]
    return module.UltimateSDUpscaleCustomSample


def test_couple_usdu_warns_on_stock_engine():
    """A3-02/A4-05: Couple USDU on an engine without the couple-crop gate still runs (the patch is
    optional) but says so instead of silently applying full-frame masks to every tile."""
    import logging

    c = Check("audit_couple_usdu_warns_on_stock_engine")
    load_pack()
    from saya_couple.src.duo_geometry import usdu_pass_node as node

    records = []
    handler = logging.Handler()
    handler.emit = records.append
    logging.getLogger().addHandler(handler)
    try:
        with tempfile.TemporaryDirectory() as tmp:
            stock = _engine_in(Path(tmp), "def crop_model_cond():\n    pass\n")
            c.ok(node._couple_crop_gate_missing(stock) is not None, "stock engine detected")
            c.eq(node._delegate_with_couple_crop_env(stock, {}, True), ("image",), "Couple on a stock engine still runs")
            c.ok(any("couple crop unavailable" in r.getMessage() for r in records), "Couple on a stock engine warns")
            records.clear()
            c.eq(node._delegate_with_couple_crop_env(stock, {}, False), ("image",), "Solo on a stock engine runs")
            c.eq(records, [], "Solo never warns")
    finally:
        logging.getLogger().removeHandler(handler)
    with tempfile.TemporaryDirectory() as tmp:
        patched = _engine_in(Path(tmp), 'SAYA_COUPLE_CROP_ENV = "SAYA_USDU_COUPLE_CROP"\n')
        c.eq(node._couple_crop_gate_missing(patched), None, "patched engine accepted")
    return c.report()


def test_couple_empty_person_2_refused_in_phase1():
    """ORCH-02: Couple with an empty PERSON_2 used to pass Phase 1 and fail in Phase 2."""
    c = Check("audit_couple_empty_person_2_refused_in_phase1")
    load_pack()
    from saya_couple.src.nodes.couple_imprint_v2 import SayaCoupleImprintError, SayaCoupleImprintPackV2

    def pack(person_2, solo):
        kwargs = dict(
            main_prompt="scene", person_1_prompt="left", person_2_prompt=person_2, negative_prompt="bad",
            direction="vertical", split=50, blur=0.0, strength_1=1.0, strength_2=1.0,
            reference_width=832, reference_height=1216, phase_model_identifier="m.safetensors",
            base_clip_identifier="m.safetensors", workflow_version="audit",
        )
        if solo is not None:
            kwargs["solo"] = solo
        return SayaCoupleImprintPackV2().pack(**kwargs)

    for empty in ("", "   ", None):
        c.raises(SayaCoupleImprintError, lambda: pack(empty, False), f"Couple + PERSON_2 {empty!r} refused")
        c.ok("person_2" not in pack(empty, True)[0]["couple_imprint"]["prompts"], f"Solo + PERSON_2 {empty!r} packs P2 absent")
        c.ok("person_2" not in pack(empty, None)[0]["couple_imprint"]["prompts"], "solo unconnected: previous behaviour")
    c.eq(pack("right", False)[0]["couple_imprint"]["prompts"]["person_2"], "right", "Couple + PERSON_2 packs P2")
    return c.report()


TESTS = [
    test_couple_empty_person_2_refused_in_phase1,
    test_couple_usdu_warns_on_stock_engine,
    test_couple_refuses_stock_core,
    test_validate_refuses_replaced_candidate,
    test_validate_without_transaction_unchanged,
]

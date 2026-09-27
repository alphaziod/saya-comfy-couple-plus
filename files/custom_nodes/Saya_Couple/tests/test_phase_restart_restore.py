"""Regression proof for Phase-1 restart and imprint checkpoint integrity."""

from __future__ import annotations

import json
import torch
from PIL import Image, PngImagePlugin

from harness import Check, load_pack, sample_v2_imprint_json


def _services(tmp_path):
    load_pack()
    from saya_couple.src.services import image_phases as services

    services.output_directory = lambda: tmp_path
    return services


def _imprint_json():
    return sample_v2_imprint_json()


def _save_phase1(services, root, imprint_json=None):
    return services.save_candidate(
        images=torch.zeros(1, 64, 64, 3),
        phase=1,
        detailer="none",
        checkpoint_root=root,
        source_path="",
        seed=123,
        positive_prompt="scene",
        negative_prompt="bad",
        models=[],
        vaes=[],
        samplers={},
        imprint_json=imprint_json,
    )


def test_phase1_native_v2_producer():
    load_pack()
    from saya_couple.src.nodes.couple_imprint_v2 import (
        SayaCoupleImprintError,
        SayaCoupleImprintPackV2,
        canonical_imprint_json,
        parse_imprint_json,
    )
    from saya_couple.src.nodes.couple_imprint import SayaCoupleImprintPack

    c = Check("phase1_native_v2_producer")
    _imprint, payload = SayaCoupleImprintPackV2().pack(
        main_prompt="scene",
        person_1_prompt="left",
        person_2_prompt="right",
        negative_prompt="bad",
        direction="vertical",
        split=50,
        blur=8.0,
        strength_1=1.0,
        strength_2=1.0,
        reference_width=896,
        reference_height=1152,
        phase_model_identifier="main.safetensors",
        base_clip_identifier="main.safetensors",
        workflow_version="restored-test",
    )
    parsed = parse_imprint_json(payload)
    c.eq(parsed["version"], 2, "producer emits schema v2")
    c.eq(canonical_imprint_json(parsed), payload, "Review canonical round-trip")
    c.eq(
        parsed["couple_imprint"]["geometry"]["feather"],
        8.0 / 1152.0,
        "legacy blur is preserved as normalized pixel sigma",
    )

    _legacy, legacy_payload = SayaCoupleImprintPack().pack(
        "scene", "left", "bad", "vertical", 50, 0.0,
        person_2_prompt="right",
    )
    try:
        parse_imprint_json(legacy_payload)
    except SayaCoupleImprintError as error:
        c.ok("version 1 imprint is not supported" in str(error), "v1 remains rejected")
    else:
        c.ok(False, "v1 remains rejected")
    return c.report()


def test_review_checkpoint_and_phase2_accept_v2(tmp_path):
    services = _services(tmp_path)
    from saya_couple.src.nodes.couple_imprint_v2 import SayaCoupleImprintPackV2
    from saya_couple.src.nodes.couple_reconstruct import (
        SayaCoupleImprintLoad,
        SayaCoupleReconstruct,
    )
    from saya_couple.src.nodes import couple_reconstruct as reconstruct_module
    from saya_couple.src.nodes.image_phases import SayaImageGenerationReview
    from test_multi_couple import _FakeClip, _FakeModel
    import nodes

    c = Check("review_checkpoint_and_phase2_accept_v2")
    _imprint, payload = SayaCoupleImprintPackV2().pack(
        main_prompt="scene",
        person_1_prompt="left",
        person_2_prompt="right",
        negative_prompt="bad",
        direction="vertical",
        split=50,
        blur=0.0,
        strength_1=1.0,
        strength_2=1.0,
        reference_width=64,
        reference_height=64,
        phase_model_identifier="main.safetensors",
        base_clip_identifier="main.safetensors",
        workflow_version="restored-test",
    )

    class _PreviewImage:
        def save_images(self, images, filename_prefix=""):
            del images, filename_prefix
            return {"ui": {"images": []}}

    original_preview = nodes.PreviewImage
    nodes.PreviewImage = _PreviewImage
    try:
        result = SayaImageGenerationReview().review(
            image=torch.zeros(1, 64, 64, 3),
            seed=123,
            positive_prompt="scene",
            negative_prompt="bad",
            models_json="[]",
            vaes_json="[]",
            samplers_json="{}",
            checkpoint_root="review-v2",
            imprint_json=payload,
        )
    finally:
        nodes.PreviewImage = original_preview
    candidate_manifest = json.loads(result["result"][1])
    candidate_path = services.checkpoint_paths(1, "review-v2").candidate_image
    with Image.open(candidate_path) as image:
        candidate_payload = image.info.get("saya_couple_imprint")
    c.eq(candidate_payload, payload, "Review candidate PNG carries canonical v2")
    c.ok("imprint_integrity" in candidate_manifest, "Review manifest carries imprint integrity")

    services.promote_candidate(1, "none", "review-v2")
    validated_path = services.checkpoint_paths(1, "review-v2").validated_image
    with Image.open(validated_path) as image:
        validated_payload = image.info.get("saya_couple_imprint")
    c.eq(validated_payload, payload, "CONTINUE checkpoint carries canonical v2 chunk")

    loaded, loaded_json = SayaCoupleImprintLoad().load(str(validated_path))
    c.eq(loaded_json, payload, "Phase 2 loader returns the validated canonical v2")
    identities = json.dumps([
        {"role": "phase_model", "identifier": "main.safetensors", "source": "checkpoint_hub_widget"},
        {"role": "base_clip", "identifier": "main.safetensors", "source": "checkpoint_hub_widget"},
    ])
    def _couple_hook(*args, **kwargs):
        del args, kwargs

    _couple_hook.__module__ = "saya_couple.src.nodes.saya_attention_couple"

    def _fake_apply(model, *args):
        del args
        patched = model.clone()
        patches = patched.model_options.setdefault("transformer_options", {}).setdefault("patches", {})
        patches.setdefault("attn2_patch", []).append(_couple_hook)
        patches.setdefault("attn2_output_patch", []).append(_couple_hook)
        return patched

    original_apply = reconstruct_module._apply_couple_patch
    reconstruct_module._apply_couple_patch = _fake_apply
    try:
        _model, _positive, _negative, report = SayaCoupleReconstruct().reconstruct(
            model=_FakeModel(),
            clip=_FakeClip(),
            checkpoint_identities=identities,
            image=torch.zeros(1, 64, 64, 3),
            imprint=loaded,
        )
    finally:
        reconstruct_module._apply_couple_patch = original_apply
    c.ok("imprint: v2 OK" in report, "Phase 2 reconstruct accepts validated v2")
    return c.report()


def test_phase1_requires_imprint(tmp_path):
    services = _services(tmp_path)
    c = Check("phase1_requires_imprint")
    root = "missing"
    _save_phase1(services, root)
    c.raises(ValueError, lambda: services.promote_candidate(1, "none", root),
             "phase 1 candidate without imprint cannot be validated")
    return c.report()


def test_phase1_imprint_roundtrip(tmp_path):
    services = _services(tmp_path)
    c = Check("phase1_imprint_roundtrip")
    root = "roundtrip"
    _save_phase1(services, root, _imprint_json())
    validated = services.promote_candidate(1, "none", root)
    paths = services.checkpoint_paths(1, root)
    with Image.open(paths.validated_image) as image:
        c.ok("saya_couple_imprint" in image.info, "validated PNG carries imprint")
    c.ok("imprint_integrity" in validated, "validated manifest carries integrity pointer")
    _image, loaded, _path = services.load_validated_source(2, "none", root)
    c.eq(loaded["transaction_uuid"], validated["transaction_uuid"], "valid checkpoint loads")
    return c.report()


def test_phase1_missing_imprint_cannot_feed_phase2(tmp_path):
    services = _services(tmp_path)
    c = Check("phase1_missing_imprint_cannot_feed_phase2")
    root = "corrupt"
    _save_phase1(services, root, _imprint_json())
    services.promote_candidate(1, "none", root)
    paths = services.checkpoint_paths(1, root)
    with Image.open(paths.validated_image) as image:
        image.save(paths.validated_image, format="PNG")
    c.raises(ValueError, lambda: services.load_validated_source(2, "none", root),
             "phase 2 refuses a phase 1 PNG without imprint")
    return c.report()


def test_phase1_status_rejects_incomplete_checkpoint(tmp_path):
    services = _services(tmp_path)
    c = Check("phase1_status_rejects_incomplete_checkpoint")
    root = "status-corrupt"
    _save_phase1(services, root, _imprint_json())
    services.promote_candidate(1, "none", root)
    paths = services.checkpoint_paths(1, root)
    with Image.open(paths.validated_image) as image:
        image.save(paths.validated_image, format="PNG")
    status = services.phase_status(1, "none", root)
    c.eq(status["validated"], "", "incomplete phase 1 is not reported as validated")
    c.ok("validated_error" in status, "status exposes the validation failure")
    return c.report()


def test_phase1_imprint_mismatch_cannot_feed_phase2(tmp_path):
    services = _services(tmp_path)
    c = Check("phase1_imprint_mismatch_cannot_feed_phase2")
    root = "mismatch"
    _save_phase1(services, root, _imprint_json())
    services.promote_candidate(1, "none", root)
    paths = services.checkpoint_paths(1, root)
    with Image.open(paths.validated_image) as image:
        pixels = image.copy()
        metadata = dict(image.info)
    imprint = json.loads(metadata["saya_couple_imprint"])
    imprint["couple_imprint"]["prompts"]["main"] = "tampered"
    png_info = PngImagePlugin.PngInfo()
    for key, value in metadata.items():
        if isinstance(value, str):
            png_info.add_text(key, value)
    png_info.add_text(
        "saya_couple_imprint",
        json.dumps(imprint, ensure_ascii=False, sort_keys=True, separators=(",", ":")),
    )
    pixels.save(paths.validated_image, format="PNG", pnginfo=png_info)
    c.raises(ValueError, lambda: services.load_validated_source(2, "none", root),
             "phase 2 refuses imprint/manifest digest mismatch")
    return c.report()


def test_restart_invalidates_old_phase1(tmp_path):
    services = _services(tmp_path)
    c = Check("restart_invalidates_old_phase1")
    root = "restart"
    _save_phase1(services, root, _imprint_json())
    services.promote_candidate(1, "none", root)
    paths = services.checkpoint_paths(1, root)
    result = services.invalidate_phase(1, root)
    c.ok(result["removed"], "restart reports removed artifacts")
    c.ok(not paths.validated_image.exists(), "old validated PNG removed")
    c.ok(not paths.validated_manifest.exists(), "old validated manifest removed")
    return c.report()


def test_imprint_swap_and_v2_security():
    load_pack()
    import copy
    import json
    from saya_couple.src.nodes.couple_imprint_v2 import (
        SayaCoupleImprintError,
        SayaCoupleImprintPackV2,
        parse_imprint_json,
    )
    from saya_couple.src.nodes.couple_imprint import SayaCoupleImprintPack

    c = Check("imprint_swap_and_v2_security")
    kwargs = dict(
        main_prompt="scene", person_1_prompt="left", person_2_prompt="right",
        negative_prompt="bad", direction="vertical", split=50, blur=0.0,
        strength_1=1.0, strength_2=1.0, reference_width=896, reference_height=1152,
        phase_model_identifier="main.safetensors", base_clip_identifier="main.safetensors",
        workflow_version="swap-test",
    )
    node = SayaCoupleImprintPackV2()
    _i, off = node.pack(**kwargs)
    _i, on = node.pack(swap=True, **kwargs)
    c.eq(parse_imprint_json(off)["couple_imprint"]["geometry"]["derived"]["swap"], False, "swap=false in imprint")
    parsed = parse_imprint_json(on)
    c.eq(parsed["couple_imprint"]["geometry"]["derived"]["swap"], True, "swap=true in imprint")
    p1 = parsed["couple_imprint"]["geometry"]["regions"]["person_1"]
    c.ok(p1["x0"] >= 0.5, "P1 region is on the swapped side")

    tampered = json.loads(on)
    tampered["couple_imprint"]["geometry"]["derived"]["swap"] = False
    try:
        parse_imprint_json(json.dumps(tampered))
    except SayaCoupleImprintError:
        c.ok(True, "modified v2 imprint refused")
    else:
        c.ok(False, "modified v2 imprint refused")
    _l, legacy = SayaCoupleImprintPack().pack("scene", "left", "bad", "vertical", 50, 0.0, person_2_prompt="right")
    try:
        parse_imprint_json(legacy)
    except SayaCoupleImprintError:
        c.ok(True, "v1 imprint refused")
    else:
        c.ok(False, "v1 imprint refused")
    return c.report()


def run(tmp_path):
    return [
        test_phase1_native_v2_producer(),
        test_imprint_swap_and_v2_security(),
        test_review_checkpoint_and_phase2_accept_v2(tmp_path),
        test_phase1_requires_imprint(tmp_path),
        test_phase1_imprint_roundtrip(tmp_path),
        test_phase1_missing_imprint_cannot_feed_phase2(tmp_path),
        test_phase1_status_rejects_incomplete_checkpoint(tmp_path),
        test_phase1_imprint_mismatch_cannot_feed_phase2(tmp_path),
        test_restart_invalidates_old_phase1(tmp_path),
    ]


if __name__ == "__main__":
    import tempfile
    from pathlib import Path

    with tempfile.TemporaryDirectory(prefix="saya_restart_restore_") as directory:
        failures = []
        for name, problems in run(Path(directory)):
            print(("PASS" if not problems else "FAIL"), name)
            failures.extend(problems)
        raise SystemExit(bool(failures))

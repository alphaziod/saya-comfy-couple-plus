"""Additive ``saya_role`` tEXt chunk on phase_N PNGs: format, lineage, imprint untouched."""

import json
import tempfile
from pathlib import Path

import torch

from harness import Check, load_pack, sample_v2_imprint_json
from test_family_save import png_text_chunks


def _services(tmp_path):
    load_pack()
    from saya_couple.src.services import image_phases as services

    services.output_directory = lambda: tmp_path
    return services


def _save(services, phase, root, seed=7, imprint=None):
    services.save_candidate(
        images=torch.rand(1, 16, 16, 3), phase=phase, detailer="none", checkpoint_root=root,
        source_path="", seed=seed, positive_prompt="pos", negative_prompt="neg",
        models=[], vaes=[], samplers={}, imprint_json=imprint,
    )
    services.promote_candidate(phase, "none", root)
    return dict(png_text_chunks(services.checkpoint_paths(phase, root).validated_image))


def test_saya_role_chunk(tmp_path):
    services = _services(tmp_path)
    c = Check("saya_role_chunk")
    root = "saya-tests/ui-abc123"
    imprint = sample_v2_imprint_json()

    chunks1 = _save(services, 1, root, imprint=imprint)
    chunks3 = _save(services, 3, root)
    chunks6 = _save(services, 6, root)
    other = _save(services, 3, "saya-tests/ui-other", seed=7)

    c.eq(chunks3["saya_role"], '{"role":"preview","phase":3,"total":6,"lineage":"saya-tests/ui-abc123"}',
         "phase 3 = exact compact preview tag")
    c.eq(chunks6["saya_role"], '{"role":"final","phase":6,"total":6,"lineage":"saya-tests/ui-abc123"}',
         "phase 6 = exact compact final tag")
    c.eq(json.loads(chunks1["saya_role"])["role"], "preview", "phase 1 = preview")
    c.eq(json.loads(chunks1["saya_role"])["lineage"], json.loads(chunks6["saya_role"])["lineage"],
         "same run -> same lineage")
    c.ok(json.loads(other["saya_role"])["lineage"] != json.loads(chunks3["saya_role"])["lineage"],
         "other run, same seed -> different lineage")

    # Imprint + manifest byte-identical to what they were before the feature.
    from saya_couple.src.nodes.couple_imprint_v2 import canonical_imprint_json, parse_imprint_json
    c.eq(chunks1["saya_couple_imprint"], canonical_imprint_json(parse_imprint_json(imprint)),
         "imprint chunk = canonical imprint, byte for byte")
    c.ok("saya_role" not in chunks1["saya_couple_imprint"], "role never inside the imprint")
    manifest = json.loads(chunks1["saya_phase_manifest"])
    c.ok("saya_role" not in manifest and "lineage" not in manifest, "role never inside the manifest")
    disk = json.loads(services.checkpoint_paths(1, root).validated_manifest.read_text(encoding="utf-8"))
    c.ok("saya_role" not in disk and "lineage" not in disk, "manifest JSON on disk unchanged (no role key)")
    c.eq(sorted(k for k in chunks1 if k.startswith("saya_")),
         ["saya_couple_imprint", "saya_phase_manifest", "saya_role"], "separate chunk, nothing else added")

    # default shared root -> hash fallback, stable for same seed+prompt
    a = services.saya_role_lineage("image/checkpoints", 5, "p")
    c.eq(a, services.saya_role_lineage("", 5, "p"), "default root hash stable")
    c.ok(a != services.saya_role_lineage("image/checkpoints", 6, "p"), "hash depends on seed")
    return c.report()


def run(tmp_path):
    return [test_saya_role_chunk(tmp_path)]


TESTS = ()

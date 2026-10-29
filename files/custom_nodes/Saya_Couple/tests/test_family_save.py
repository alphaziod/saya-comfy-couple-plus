"""SayaFamilySave: family layout, additive ``saya_family`` chunk, sidecar, no overwrite.

Reads the saved PNG back with a byte-level chunk parser that mirrors the app's
reader (``saya-studio crates/studio-core/src/image_restore.rs::text_chunks``:
tEXt latin-1 / zTXt zlib / iTXt utf-8, CRC checked like the ``png`` crate does),
so the test proves what Saya Studio will actually see — not what PIL remembers.

Rouge -> vert: the checks import ``SayaFamilySave`` from the pack; without the
node (or without its registry entry) the import fails and every test crashes.

Runs WITHOUT GPU and WITHOUT a ComfyUI server:

    PYTHONDONTWRITEBYTECODE=1 <ComfyUI python> tests/test_family_save.py
"""

from __future__ import annotations

import json
import struct
import tempfile
import zlib
from pathlib import Path

from harness import Check, load_pack


def png_text_chunks(path: Path) -> list[tuple[str, str]]:
    """Mirror of the app's text_chunks(): tEXt + zTXt + iTXt, CRC verified."""
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    chunks: list[tuple[str, str]] = []
    offset = 8
    while offset + 12 <= len(data):
        length = struct.unpack(">I", data[offset:offset + 4])[0]
        ctype = data[offset + 4:offset + 8]
        body = data[offset + 8:offset + 8 + length]
        crc = struct.unpack(">I", data[offset + 8 + length:offset + 12 + length])[0]
        assert zlib.crc32(ctype + body) & 0xFFFFFFFF == crc, f"bad CRC in {ctype!r}"
        if ctype == b"tEXt":
            keyword, text = body.split(b"\x00", 1)
            chunks.append((keyword.decode("latin-1"), text.decode("latin-1")))
        elif ctype == b"zTXt":
            keyword, rest = body.split(b"\x00", 1)
            chunks.append((keyword.decode("latin-1"), zlib.decompress(rest[1:]).decode("latin-1")))
        elif ctype == b"iTXt":
            keyword, rest = body.split(b"\x00", 1)
            compressed = rest[0] == 1
            rest = rest[2:]
            _language, rest = rest.split(b"\x00", 1)
            _translated, text = rest.split(b"\x00", 1)
            if compressed:
                text = zlib.decompress(text)
            chunks.append((keyword.decode("latin-1"), text.decode("utf-8")))
        offset += 12 + length
        if ctype == b"IEND":
            return chunks
    raise AssertionError("IEND missing")


PROMPT = {
    "10": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": "MKLAN POISONED NOOB AI FUTA_00001_.safetensors"}},
    "12": {"class_type": "LoraLoader", "inputs": {"lora_name": "illustrious_slime_girl_v1.safetensors", "strength_model": 0.75, "strength_clip": 0.85, "model": ["10", 0]}},
    "20": {"class_type": "KSampler", "inputs": {"seed": 1234567890123, "steps": 30, "cfg": 5.0, "model": ["12", 0]}},
}
EXTRA = {"workflow": {"nodes": [], "links": [], "extra": {"saya_couple_version": "0.3.0"}}}


class _Output:
    """Temporary output root, plugged through the module's OUTPUT_ROOT override."""

    def __init__(self, module):
        self.module = module

    def __enter__(self):
        self._dir = tempfile.TemporaryDirectory()
        self._saved = self.module.OUTPUT_ROOT
        self.module.OUTPUT_ROOT = Path(self._dir.name)
        return Path(self._dir.name)

    def __exit__(self, *exc):
        self.module.OUTPUT_ROOT = self._saved
        self._dir.cleanup()


def _mods():
    load_pack()
    import torch
    from saya_couple.src.nodes import saya_family_save
    from saya_couple.src.nodes.saya_family_save import SayaFamilySave

    return torch, saya_family_save, SayaFamilySave


def test_family_layout_chunks_sidecar_no_overwrite():
    torch, module, saver_class = _mods()
    c = Check("family_save_main_contract")
    node = saver_class()
    images = torch.rand(1, 32, 48, 3)  # CPU only: (batch, H, W, C), no GPU anywhere
    with _Output(module) as out:
        ui = node.save(images, famille="hum", perso_id="ch--0001-nelia-thorne", prompt=PROMPT, extra_pnginfo=EXTRA)
        saved = ui["result"][0].splitlines()
        c.eq(len(saved), 1, "one path per image")
        png = Path(saved[0])
        c.eq(str(png.parent), str(out / "saya" / "hum"), "layout: <output>/saya/<famille>/")
        c.eq(png.name, "ch--0001-nelia-thorne-noobai_00001_.png",
             "stem + checkpoint-derived model tag + standard ComfyUI counter")

        # second save, same identity: the counter moves, nothing is overwritten
        node.save(images, famille="hum", perso_id="ch--0001-nelia-thorne", prompt=PROMPT, extra_pnginfo=EXTRA)
        c.ok(png.is_file(), "1st PNG untouched by the 2nd save")
        second = Path(node.save(images, famille="hum", perso_id="ch--0001-nelia-thorne",
                                prompt=PROMPT, extra_pnginfo=EXTRA)["result"][0])
        c.eq(second.name, "ch--0001-nelia-thorne-noobai_00003_.png", "3rd save -> _00003_")

        chunks = dict(png_text_chunks(png))
        c.ok("prompt" in chunks, "chunk prompt present (app reader contract)")
        c.ok("workflow" in chunks, "chunk workflow present (app reader contract)")
        c.ok("saya_family" in chunks, "additive chunk saya_family present")
        api = json.loads(chunks["prompt"])
        c.eq(api["10"]["inputs"]["ckpt_name"], PROMPT["10"]["inputs"]["ckpt_name"], "prompt chunk = the API graph")
        ui_graph = json.loads(chunks["workflow"])
        c.eq(ui_graph["extra"]["saya_couple_version"], "0.3.0", "workflow chunk keeps extra.saya_couple_version")

        payload = json.loads(chunks["saya_family"])
        c.eq(payload["famille"], "hum", "famille")
        c.eq(payload["prefix_id"], "ch", "prefix_id parsed from the stem (canonical ch kept)")
        c.eq(payload["perso_id"], "ch--0001-nelia-thorne", "perso_id = the complete stem, untouched")
        c.eq(payload["numero"], 1, "numero parsed")
        c.eq(payload["nom"], "nelia-thorne", "nom parsed")
        c.eq(payload["stem"], "ch--0001-nelia-thorne-noobai", "file stem")
        c.eq(payload["modele"], "noobai", "model tag from the checkpoint filename")
        c.eq(payload["seed"], 1234567890123, "executed seed read from the API graph")
        c.eq(payload["checkpoints"], [PROMPT["10"]["inputs"]["ckpt_name"]], "checkpoints list")
        c.eq(payload["loras"], [{"name": "illustrious_slime_girl_v1", "strength_model": 0.75, "strength_clip": 0.85}], "loras list")
        c.eq(payload["saya_couple_version"], "0.3.0", "version from the workflow extra")
        c.eq(payload["fichier"], png.name, "fichier = the saved PNG name")

        sidecar = png.with_suffix(".saya_family.json")
        c.ok(sidecar.is_file(), "sidecar JSON written beside the PNG")
        c.eq(json.loads(sidecar.read_text(encoding="utf-8")), payload, "sidecar = the chunk bytes")
    return c.report()


def test_lot_segment_and_stem_model_suffix_rules():
    torch, module, saver_class = _mods()
    c = Check("family_save_stem_rules")
    node = saver_class()
    images = torch.rand(1, 8, 8, 3)
    with _Output(module) as out:
        first = Path(node.save(images, famille="slm", perso_id="slm-00007-gloo-illustrious",
                               lot="2026-10-25 batch A", prompt=PROMPT, extra_pnginfo=EXTRA)["result"][0])
        c.eq(str(first.parent), str(out / "saya" / "2026-10-25-batch-a" / "slm"), "lot slugified into a segment")
        # the stem's own -illustrious tag is re-derived from the checkpoint (noobai), never doubled
        c.eq(first.name, "slm-00007-gloo-noobai_00001_.png", "model tag follows the checkpoint that ran")
        # no checkpoint in the graph: the stem stays exactly as given (canonical)
        bare = Path(node.save(images, famille="kmn", perso_id="kmn-0003-miko-illustrious",
                              prompt={"1": {"class_type": "KSampler", "inputs": {"seed": ["30", 0]}}},
                              extra_pnginfo=EXTRA)["result"][0])
        c.eq(bare.name, "kmn-0003-miko-illustrious_00001_.png", "no checkpoint -> stem untouched")
        payload = json.loads(dict(png_text_chunks(bare))["saya_family"])
        c.eq(payload["seed"], None, "seed carried as a link -> null, never invented")
        c.eq(payload["checkpoints"], [], "no checkpoint detected -> empty list")
        # empty perso_id: family fallback naming (the model tag lives in the chunk, not the file)
        anon = Path(node.save(images, famille="bny", prompt=PROMPT, extra_pnginfo=EXTRA)["result"][0])
        c.eq(anon.name, "bny_00001_.png", "empty stem -> family fallback")
        anon_payload = json.loads(dict(png_text_chunks(anon))["saya_family"])
        c.eq(anon_payload["perso_id"], None, "perso_id null when empty")
        c.eq(anon_payload["modele"], "noobai", "model tag still recorded in the chunk")
    return c.report()


def test_prefix_override_custom_family_and_errors():
    torch, module, saver_class = _mods()
    from saya_couple.src.nodes.saya_family_save import SayaFamilySaveError, _resolve_famille

    c = Check("family_save_override_custom_errors")
    node = saver_class()
    images = torch.rand(1, 8, 8, 3)
    with _Output(module) as out:
        overridden = Path(node.save(images, famille="custom", famille_custom="Zorro!", prompt=PROMPT,
                                    extra_pnginfo=EXTRA, filename_prefix="saya_preview/v1/run")["result"][0])
        c.eq(str(overridden.parent), str(out / "saya_preview" / "v1"), "filename_prefix = native override (subfolders kept)")
        c.eq(overridden.name, "run_00001_.png", "native prefix naming + counter")
        payload = json.loads(dict(png_text_chunks(overridden))["saya_family"])
        c.eq(payload["famille"], "zorro", "custom famille slugified into the chunk")
        c.eq(payload["lot"], None, "lot absent -> null")

        c.raises(SayaFamilySaveError,
                 lambda: node.save(images, famille="custom", famille_custom="  ", prompt=PROMPT, extra_pnginfo=EXTRA),
                 "custom with an empty famille_custom = hard error")
        c.raises(SayaFamilySaveError, lambda: _resolve_famille("ch", ""), "unknown combo value = hard error")
        c.eq(_resolve_famille("lcs", ""), "lcs", "known family passes")
    return c.report()


TESTS = (test_family_layout_chunks_sidecar_no_overwrite, test_lot_segment_and_stem_model_suffix_rules, test_prefix_override_custom_family_and_errors)

if __name__ == "__main__":
    failures = []
    for fn in TESTS:
        name, problems, _skip = fn()
        print(("PASS" if not problems else "FAIL"), name)
        failures.extend(problems)
    raise SystemExit(bool(failures))

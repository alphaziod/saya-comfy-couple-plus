"""SayaLoraFamilyFilter: Model 1's LoRAs on Model 2, filtered by the LoRA Manager family (base_model)."""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

from harness import Check, load_pack


def test_lora_family_filter():
    load_pack()
    from saya_couple.src.nodes import lora_family_filter as lf

    c = Check("lora_family_filter")
    c.eq(lf.family_of("SDXL 1.0"), "sdxl", "SDXL 1.0 = sdxl")
    c.eq(lf.family_of("Illustrious"), "illustrious", "Illustrious")
    c.eq(lf.family_of("Illustrious/anime/waiIllustriousSDXL_v150"), "illustrious", "illustrious beats the sdxl of the name")
    c.eq(lf.family_of("Unknown"), None, "Unknown = no family")
    c.eq(lf.parse_loras("<lora:a:0.85> <lora:b c:0.25:0.3>", "", "<lora:d:1>"), [("a", 0.85), ("b c", 0.25), ("d", 1.0)], "loaded_loras parsing")
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "loras"
        for name, family in (("illu", "Illustrious"), ("xl", "SDXL 1.0"), ("pony", "Pony"), ("nometa", None)):
            sub = root / "sub"
            sub.mkdir(parents=True, exist_ok=True)
            (sub / f"{name}.safetensors").write_bytes(b"x")
            if family:
                (sub / f"{name}.metadata.json").write_text(json.dumps({"base_model": family}))
        ck = Path(tmp) / "SDXL 1.0" / "anime"
        ck.mkdir(parents=True)
        (ck / "m2.safetensors").write_bytes(b"x")
        (ck / "m2.metadata.json").write_text(json.dumps({"base_model": "Unknown"}))
        applied = []

        def fake_apply(model, path, strength):
            applied.append((path.stem, strength))
            return model + [path.stem]

        stack = "<lora:illu:0.85> <lora:xl:0.5> <lora:pony:1> <lora:nometa:0.3> <lora:absent:1>"
        node = lf.SayaLoraFamilyFilter()
        model, report = node.filter([], "SDXL 1.0/anime/m2.safetensors", True, "", stack, _roots=[root], _ckpt=ck / "m2.safetensors", _apply=fake_apply)
        c.eq(model, ["xl", "nometa"], "SDXL model 2 (Unknown metadata, read from folder): SDXL LoRA + unknown family pass, Illustrious / Pony blocked")
        c.ok("family sdxl (folder)" in report and "BLOCK illu" in report and "SKIP absent" in report, "report: family source, blocks, missing file")
        model, _ = node.filter([], "Illustrious/x.safetensors", True, "", stack, _roots=[root], _ckpt=Path(tmp) / "none.safetensors", _apply=fake_apply)
        c.eq(model, ["illu", "xl", "nometa"], "Illustrious model 2: Illustrious + SDXL pass, Pony blocked")
        model, _ = node.filter([], "Flux/f.safetensors", True, "", stack, _roots=[root], _ckpt=Path(tmp) / "none.safetensors", _apply=fake_apply)
        c.eq(model, ["illu", "xl", "pony", "nometa"], "other family: everything passes for now")
        model, _ = node.filter([], "misc/f.safetensors", True, "", stack, _roots=[root], _ckpt=Path(tmp) / "none.safetensors", _apply=fake_apply)
        c.eq(model, ["illu", "xl", "pony", "nometa"], "unknown family: everything passes")
        applied.clear()
        model, report = node.filter(["m"], "SDXL 1.0/anime/m2.safetensors", False, "", stack, _roots=[root], _apply=fake_apply)
        c.ok(model == ["m"] and not applied, "OFF: Model 2 untouched")
        model, report = node.filter([], "SDXL 1.0/m2.safetensors", True, "", "<lora:xl:0>", _roots=[root], _ckpt=ck / "m2.safetensors", _apply=fake_apply)
        c.eq(model, [], "strength 0 is not applied")
        model, report = node.filter([], "Flux/f.safetensors", True, "SDXL 1.0/anime/m2.safetensors", stack, _roots=[root], _ckpt=ck / "m2.safetensors", _apply=fake_apply)
        c.ok(model == ["xl", "nometa"] and "SDXL 1.0/anime/m2.safetensors" in report, "ckpt_name_in (Model 2 Identifier) wins over the combo")
    return c.report()


TESTS = [test_lora_family_filter]

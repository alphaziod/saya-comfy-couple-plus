#!/usr/bin/env python3
"""Stub engine for the V3 smoke (casting sorts/filters): same CLI contract as
test_solo.py --casting N --persos ID1,ID2,… --graine G (PYTHONHASHSEED=0 via os.execvpe,
catalogue written to sortie/casting/catalogue.json, CASTING/CATALOGUE messages on
stdout) but with no real draw: the real engine is proven by tests/tests_casting.py,
tests/tests_casting_persos.py on the engine side and crates/studio-core on the app side.

V3: the four fixture ids carry DIFFERENT families, models and LoRAs
(real `loras` lines of loaders 662/663/664 of the master preset: nyalia,
AddMicroDetails_Illustrious_v5, Matte_Skin_Illustrious_v4) to prove that the
grid filters really separate them. Any other mode (--famille, --pose-fixe,
--actions, --poses) is loudly refused: this stub only covers character targeting.
"""
import json
import os
import sys
from pathlib import Path

args = sys.argv[1:]
if os.environ.get("PYTHONHASHSEED") != "0":
    os.execvpe(sys.executable, [sys.executable, *sys.argv], {**os.environ, "PYTHONHASHSEED": "0"})


def valeur(flag, defaut=None):
    return args[args.index(flag) + 1] if flag in args else defaut


if "--famille" in args or "--pose-fixe" in args or "--actions" in args or "--poses" in args:
    raise RuntimeError(f"stub V3 : mode non couvert — uniquement --casting N --persos a,b,… : {args}")
cibles = valeur("--persos")
if cibles is None:
    raise RuntimeError("stub V3 : --persos requis")
nombre = int(valeur("--casting", "0"))
ids = [x.strip() for x in cibles.split(",") if x.strip()]
if len(ids) != nombre:
    raise RuntimeError(f"casting : --casting {nombre} et --persos ({len(ids)} ids) divergent")
graine = int(valeur("--graine", "1115320527758383"))

NOOBAI = "NoobAI/no tags/MKLAN POISONED NOOB AI FUTA_00001_.safetensors"
ILLU = "Illustrious/no tags/Mklan-Sayapacito-Echo_00001_.safetensors"
P1_STUB = "1girl, solo, elf, real lop ears, (long white floppy ears:1.3)"

# The LoRA names exist in the Lora Loader (LoraManager) lines of the master
# preset: casting.rs re-enables them by exact name (no invented line).
LORA_662 = {"name": "nyalia", "strength": "0.20", "active": True, "clipStrength": "0.20"}
LORA_663 = {"name": "AddMicroDetails_Illustrious_v5", "strength": "0.25", "active": True, "clipStrength": "0.25"}
LORA_664 = {"name": "Matte_Skin_Illustrious_v4", "strength": "0.70", "active": True, "clipStrength": "0.70"}


def passe_un(checkpoint, lora_loaders):
    return {
        "prefixe": "embedding:lazypos, embedding:lazyneg",
        "decor_libre": "night club, neon signs",
        "action": "standing, one leg up",
        "p1": P1_STUB,
        "anatomy": "(small wings:1.25), cute_tail",
        "negative": "embedding:lazyneg",
        "p2": "",
        "p2_anatomy": "",
        "checkpoint": checkpoint,
        "lora_loaders": {
            "662": {"loras": lora_loaders.get("662", []), "text": ""},
            "663": {"loras": lora_loaders.get("663", []), "text": ""},
            "664": {"loras": lora_loaders.get("664", []), "text": ""},
        },
        "format_paysage": False,
        "seed": -1,
    }


TABLE = {
    "CH--9101": {
        "stem": "bny-01001-stub-v3-noobai", "name": "Stub V3 Delta", "famille": "bny",
        "model": "noobai", "checkpoint": NOOBAI,
        "lora_loaders": {"662": [LORA_662]},
    },
    "CH--9102": {
        "stem": "bny-01002-stub-v3-illustrious", "name": "Stub V3 Echo", "famille": "bny",
        "model": "illustrious", "checkpoint": ILLU,
        "lora_loaders": {"663": [LORA_663]},
    },
    "CH--9103": {
        "stem": "kmn-00003-stub-v3-noobai", "name": "Stub V3 Fox", "famille": "kmn",
        "model": "noobai", "checkpoint": NOOBAI,
        "lora_loaders": {"662": [LORA_662], "664": [LORA_664]},
    },
    "CH--9104": {
        "stem": "kmn-00004-stub-v3-illustrious", "name": "Stub V3 Golf", "famille": "kmn",
        "model": "illustrious", "checkpoint": ILLU,
        "lora_loaders": {},
    },
}

shots = []
for i, pid in enumerate(ids, 1):
    ligne = TABLE.get(pid)
    if ligne is None:
        raise RuntimeError(
            f"casting persos : {pid} absent du lot (ids du fixture V3 : {' '.join(sorted(TABLE))})")
    shots.append({
        "id": pid,
        "stem": ligne["stem"],
        "name": ligne["name"],
        "famille": ligne["famille"],
        "model": ligne["model"],
        "seed": graine + i,
        "note": "negatif",
        "negatif_fige": True,
        "p1": P1_STUB,
        "anatomy": "(small wings:1.25), cute_tail",
        "pose": "standing, one leg up",
        "bg": "night club, neon signs",
        "lazy": True,
        "casting": passe_un(ligne["checkpoint"], ligne["lora_loaders"]),
    })

dossier = Path("sortie") / "casting"
dossier.mkdir(parents=True, exist_ok=True)
# Post-run proof: the EXACT argv received from the launcher (the .sh checks it against the targeted ids).
(dossier / "argv.json").write_text(json.dumps(args, ensure_ascii=False))
(dossier / "catalogue.json").write_text(json.dumps(shots, ensure_ascii=False, indent=2) + "\n")
print(f"CASTING PERSOS sortie/casting — {len(shots)} persos ciblés ({' '.join(ids)}), rien envoyé", flush=True)
print(f"CATALOGUE {dossier / 'catalogue.json'}", flush=True)

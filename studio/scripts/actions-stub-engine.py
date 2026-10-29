#!/usr/bin/env python3
"""Stub engine for the V2 casting smoke (targeted actions): same CLI contract as
test_solo.py --casting N [--famille X] --actions a,b,c --graine G (PYTHONHASHSEED=0 via
os.execvpe, catalogue written to sortie/casting/catalogue.json, CASTING/CATALOGUE messages
on stdout) but with no real draw: the real engine is proven by tests/tests_actions.py
and crates/studio-core/tests/actions_bank.rs. The action names received are traced in
sortie/casting/argv.json (the .sh launcher checks them against the selection made in the UI)
and every shot carries pose = the cycle's targeted action + an actions_ciblees field.
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


nombre = int(valeur("--casting", "0"))
famille = valeur("--famille")
actions = [a.strip() for a in (valeur("--actions") or "").split(",") if a.strip()]
if famille not in (None, "bny"):
    raise RuntimeError(f"famille de casting inconnue : {famille} (attendues : hum bny)")
if not actions:
    raise RuntimeError("casting actions ciblées : liste vide (--actions id1,id2)")
if "--graine" in args:
    graine = int(valeur("--graine", "0"))
else:
    graine = 1115320527758383

# Real checkpoints from the captured object_info: the phase 1 conversion rejects an
# absent combo name (validate_graph, "Choix non disponible").
NOOBAI = "NoobAI/no tags/MKLAN POISONED NOOB AI FUTA_00001_.safetensors"
ILLU = "Illustrious/no tags/Mklan-Sayapacito-Echo_00001_.safetensors"


def passe_un(p1, pose, checkpoint):
    return {
        "prefixe": "embedding:lazypos, embedding:lazyneg",
        "decor_libre": "night club, neon signs",
        "action": pose,
        "p1": p1,
        "anatomy": "(small wings:1.25), cute_tail",
        "negative": "embedding:lazyneg",
        "p2": "",
        "p2_anatomy": "",
        "checkpoint": checkpoint,
        "lora_loaders": {
            "662": {"loras": [], "text": ""},
            "663": {"loras": [], "text": ""},
            "664": {"loras": [], "text": ""},
        },
        "format_paysage": False,
        "seed": -1,
    }


P1_STUB = "1girl, solo, elf, real lop ears, (long white floppy ears:1.3)"
shots = []
for index in range(nombre):
    pair = index % 2 == 0
    pose = actions[index % len(actions)]
    shots.append({
        "id": f"CH-9{index:04d}",
        "stem": f"bny-{index + 1:05d}-stub-oc-{'noobai' if pair else 'illustrious'}",
        "name": f"Stub OC {index + 1}",
        "famille": famille or "bny",
        "model": "noobai" if pair else "illustrious",
        "seed": graine + index,
        "note": "oc",
        "p1": P1_STUB,
        "anatomy": "(small wings:1.25), cute_tail",
        "pose": pose,
        "bg": "night club, neon signs",
        "lazy": True,
        "actions_ciblees": actions,
        "casting": passe_un(P1_STUB, pose, NOOBAI if pair else ILLU),
    })

dossier = Path("sortie") / "casting"
dossier.mkdir(parents=True, exist_ok=True)
(dossier / "argv.json").write_text(json.dumps(args, ensure_ascii=False))
(dossier / "catalogue.json").write_text(json.dumps(shots, ensure_ascii=False, indent=2) + "\n")
print(f"CASTING {len(shots)} OC « {famille or 'tirage normal'} », actions ciblées « {','.join(actions)} », rien envoyé", flush=True)
print(f"CATALOGUE {dossier / 'catalogue.json'}", flush=True)

#!/usr/bin/env python3
"""Stub engine for the U1/U2/V1 casting smokes: same CLI contract as test_solo.py
--casting N [--famille X] [--pose-fixe <action>] [--poses <ID>] [--persos ID1,ID2,…]
[--exclude a,b,c] --graine G
(PYTHONHASHSEED=0 via os.execvpe, catalogue written to sortie/casting/catalogue.json,
CASTING/CATALOGUE messages on stdout) but with no real draw: the real engine is proven
by tests/tests_casting.py, tests/tests_casting_persos.py, tests/tests_rotations.py,
tests/casting.rs and tests/casting_persos.rs.
U2 rotations: --poses + --casting → Mode A (N copies of the SAME character, identical prompts,
stem -rotNN); --pose-fixe → Mode B (N OCs with THE SAME pose, action = the given name).
V1 targeting: --persos a,b,… + --casting N (N = number of ids, otherwise error — like the real
engine) → EXACTLY these ids, one shot each, in the requested order (stem hum--NNNN-stub-cible).
Pass 3: --exclude a,b,c is recognised (the app refuses the exclusion if the engine ignores the
flag) and EVERY invocation is logged in sortie/casting/argv_journal.jsonl —
end-to-end proof of what the backend actually passed to the engine.
"""
import json
import os
import sys
from pathlib import Path


# STUB_OC_NAMES="Aiko,Mira,…" gives the OCs readable names (screenshots); default "Stub OC <n>".
def oc_name(index):
    names = [n.strip() for n in os.environ.get("STUB_OC_NAMES", "").split(",") if n.strip()]
    return names[index % len(names)] if names else f"Stub OC {index + 1}"


args = sys.argv[1:]
if os.environ.get("PYTHONHASHSEED") != "0":
    os.execvpe(sys.executable, [sys.executable, *sys.argv], {**os.environ, "PYTHONHASHSEED": "0"})


def valeur(flag, defaut=None):
    return args[args.index(flag) + 1] if flag in args else defaut


nombre = int(valeur("--casting", "0"))
famille = valeur("--famille")
perso = valeur("--poses")
fixe = valeur("--pose-fixe")
cibles = valeur("--persos")
exclues = valeur("--exclude")
if famille not in (None, "bny"):
    raise RuntimeError(f"famille de casting inconnue : {famille} (attendues : hum bny)")
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
if cibles is not None:
    # Targeting (V1): EXACTLY these ids, one shot each, in the requested order.
    # Same refusals as the real engine (test_solo.py wires --persos).
    if "--famille" in args or "--pose-fixe" in args or "--poses" in args:
        raise RuntimeError("--persos est exclusif : ni --famille, ni --pose-fixe, ni --poses")
    ids = [x.strip() for x in cibles.split(",") if x.strip()]
    if len(ids) != nombre:
        raise RuntimeError(f"casting : --casting {nombre} et --persos ({len(ids)} ids) divergent")
    if not ids:
        raise RuntimeError("casting persos : aucun id demandé")
    for i, pid in enumerate(ids, 1):
        numero = pid.replace("CH--", "").replace("CH-", "") or str(i)
        shots.append({
            "id": pid,
            "stem": f"hum--{numero}-stub-cible-noobai",
            "name": f"Stub Ciblé {pid}",
            "famille": "hum",
            "model": "noobai",
            "seed": graine + i,
            "note": "negatif",
            "negatif_fige": True,
            "p1": P1_STUB,
            "anatomy": "(small wings:1.25), cute_tail",
            "pose": "standing, one leg up",
            "bg": "night club, neon signs",
            "lazy": True,
            "casting": passe_un(P1_STUB, "standing, one leg up", NOOBAI),
        })
elif perso is not None:
    # Mode A (U2): ONE character × N times the same pose — identical copies, stem -rotNN.
    if "--casting" not in args:
        raise RuntimeError("rotations Mode A : --casting N requis avec --poses <ID>")
    for i in range(1, nombre + 1):
        shots.append({
            "id": f"CH--9201-rot{i:02d}",
            "stem": f"hum--01695-stub-rot-{i:02d}-noobai",
            "name": f"Stub Prêtresse · rot {i}",
            "famille": "hum",
            "model": "noobai",
            "seed": graine + i,
            "note": "oc",
            "p1": P1_STUB,
            "anatomy": "(small wings:1.25), cute_tail",
            "pose": "standing, one leg up",
            "bg": "night club, neon signs",
            "lazy": True,
            "casting": passe_un(P1_STUB, "standing, one leg up", NOOBAI),
        })
elif fixe is not None:
    # Mode B (U2): 1 pose × N characters — the fixed pose IS the given action name.
    for index in range(nombre):
        pair = index % 2 == 0
        shots.append({
            "id": f"CH-9{index:04d}",
            "stem": f"bny-{index + 1:05d}-stub-oc-{'noobai' if pair else 'illustrious'}",
            "name": oc_name(index),
            "famille": "bny",
            "model": "noobai" if pair else "illustrious",
            "seed": graine + index,
            "note": "oc",
            "p1": P1_STUB,
            "anatomy": "(small wings:1.25), cute_tail",
            "pose": fixe,
            "bg": "night club, neon signs",
            "lazy": True,
            "casting": passe_un(P1_STUB, fixe, NOOBAI if pair else ILLU),
        })
else:
    # V2 targeting: the targeted actions' text becomes the pose, read from the local
    # bank like the real engine (unknown or non-validated action = refusal).
    poses = []
    for nom in (valeur("--actions") or "").split(","):
        if not nom:
            continue
        found = None
        for bank in Path("data/actions").glob("*.actions.json"):
            action = json.loads(bank.read_text())["actions"].get(nom)
            if action is not None:
                found = action
        if found is None or not found.get("valide"):
            raise RuntimeError(f"action de casting refusée : {nom} (action validée et solo attendue)")
        poses.append(found["add"])
    for index in range(nombre):
        pair = index % 2 == 0
        pose = poses[index % len(poses)] if poses else "standing, one leg up"
        shots.append({
            "id": f"CH-9{index:04d}",
            "stem": f"bny-{index + 1:05d}-stub-oc-{'noobai' if pair else 'illustrious'}",
            "name": oc_name(index),
            "famille": "bny",
            "model": "noobai" if pair else "illustrious",
            "seed": graine + index,
            "note": "oc",
            "p1": P1_STUB,
            "anatomy": "(small wings:1.25), cute_tail",
            "pose": pose,
            "bg": "night club, neon signs",
            "lazy": True,
            "casting": passe_un(P1_STUB, pose, NOOBAI if pair else ILLU),
        })

if cibles is not None:
    entete = f"CASTING PERSOS sortie/casting — {len(shots)} persos ciblés ({' '.join(x['id'] for x in shots)})"
elif perso is not None:
    entete = f"CASTING ROTATIONS {perso} × {len(shots)} fois la même pose"
elif fixe is not None:
    entete = f"CASTING {len(shots)} OC « {famille or 'tirage normal'} », pose « {fixe} »"
else:
    entete = f"CASTING {len(shots)} OC « {famille or 'tirage normal'} »"
dossier = Path("sortie") / "casting"
dossier.mkdir(parents=True, exist_ok=True)
with (dossier / "argv_journal.jsonl").open("a") as journal:
    journal.write(json.dumps(sys.argv, ensure_ascii=False) + "\n")
(dossier / "catalogue.json").write_text(json.dumps(shots, ensure_ascii=False, indent=2) + "\n")
entete_exclues = f" · exclues {exclues}" if exclues else ""
print(f"{entete}{entete_exclues}, rien envoyé", flush=True)
print(f"CATALOGUE {dossier / 'catalogue.json'}", flush=True)

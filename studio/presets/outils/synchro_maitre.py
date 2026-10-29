#!/usr/bin/env python3
"""Recopie les réglages du workflow maître dans les 6 passes (mêmes ids de nodes, même type).

Seuls les widgets_values changent : modèles, VAE, routings, seed, résolution, prompts, LoRAs,
couple/solo, upscale. La structure des passes n'est jamais touchée.
Usage : synchro_maitre.py [DOSSIER]   (défaut : dossier ILUSTCOUPLE_PASSES de ComfyUI)
"""
import json, sys
from pathlib import Path

DEFAUT = Path.home() / "AI/ComfyUI/user/default/workflows/ILUSTCOUPLE_PASSES"


def main(dossier):
    dossier = Path(dossier)
    maitre = json.loads((dossier / "ILUSTCOUPLE_00_MAITRE_CONFIG.json").read_text())
    reglages = {n["id"]: (n["type"], n.get("widgets_values")) for n in maitre["nodes"]}
    for f in sorted(dossier.glob("ILUSTCOUPLE_0[1-6]_PASSE_*.json")):
        wf = json.loads(f.read_text())
        changes = 0
        for n in wf["nodes"]:
            ref = reglages.get(n["id"])
            if ref and ref[0] == n["type"] and n.get("widgets_values") != ref[1]:
                n["widgets_values"] = ref[1]
                changes += 1
        if changes:
            f.write_text(json.dumps(wf, ensure_ascii=False, indent=1))
        print(f"{f.name} : {changes} réglage(s) recopié(s)")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else DEFAUT)

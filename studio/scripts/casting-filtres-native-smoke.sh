#!/usr/bin/env bash
# V3 native smoke: casting sorts/filters (V1/V2 windows + grid). Test Tauri window
# (feature smoke-test), isolated database (test-results/casting-filtres-v3/<run>),
# stub engine (scripts/casting-filtres-stub-engine.py) plugged in via SAYA_ENGINE_DIR:
# fixture negatifs_figes.json registry (4 validated bny/kmn × noobai/illustrious) +
# fixture action bank data/actions/ (file families x/y, scoped LoRAs);
# fake ComfyUI accepting /prompt. No GPU, never the installed app, never the real
# engine. Prior build — BOTH features are mandatory: without `custom-protocol`
# the app does not embed the UI ("Could not connect"):
#   npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
base="$root/test-results/casting-filtres-v3"
stub="$base/engine-stub"
rm -rf "$stub"
mkdir -p "$stub" "$base"
cp "$root/scripts/casting-filtres-stub-engine.py" "$stub/test_solo.py"
python3 - "$stub" <<'PY'
import json, sys
from pathlib import Path
stub = Path(sys.argv[1])
rows = [
    {"neg_id": "CH--9101", "nom": "Delta Test", "slug": "delta-test", "model": "noobai", "famille": "bny", "source": "oc", "bank_id": ""},
    {"neg_id": "CH--9102", "nom": "Echo Test", "slug": "echo-test", "model": "illustrious", "famille": "bny", "source": "banque", "bank_id": "CH-00002"},
    {"neg_id": "CH--9103", "nom": "Fox Test", "slug": "fox-test", "model": "noobai", "famille": "kmn", "source": "banque", "bank_id": "CH-00003"},
    {"neg_id": "CH--9104", "nom": "Golf Test", "slug": "golf-test", "model": "illustrious", "famille": "kmn", "source": "banque", "bank_id": "CH-00004"},
]
(stub / "negatifs_figes.json").write_text(json.dumps({"valides": rows}, ensure_ascii=False, indent=2) + "\n")
actions = stub / "data" / "actions"
actions.mkdir(parents=True)
(actions / "x.actions.json").write_text(json.dumps({
    "famille": "x",
    "actions": {
        "x_a": {"add": "(action a:1.1)", "valide": True, "lora": {"name": "pole dancing_illustrious_V2.0", "strength": 0.6, "clip_strength": 0.6}},
        "x_duo": {"add": "(duo pose:1.3)", "valide": True, "pas_solo": True},
    },
}, ensure_ascii=False, indent=2) + "\n")
(actions / "y.actions.json").write_text(json.dumps({
    "famille": "y",
    "actions": {
        "y_b": {"add": "(action b:1.1)", "valide": True, "lora": {"name": "cute_v1.5", "strength": 0.65, "clip_strength": 0.65}},
        "y_c": {"add": "(action c:1.1)", "valide": True},
        "y_vieux": {"add": "(old:1.0)", "valide": False},
    },
}, ensure_ascii=False, indent=2) + "\n")
# `_ordre.json`: actions_lister marks disponible:false any action absent from
# the bank's historical order (pass 3 rule, backend side).
(actions / "_ordre.json").write_text(json.dumps(["x_a", "x_duo", "y_b", "y_c", "y_vieux"]) + "\n")
PY
export SAYA_STUDIO_PRESETS="$root/presets"
export SAYA_ENGINE_DIR="$stub"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/casting-filtres-native-smoke.js" \
  --root "$base" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --timeout 420

# Stub-engine-side proof: the stub RECEIVED exactly the 4 targeted ids.
python3 - "$stub" <<'PY'
import json, sys
from pathlib import Path
stub = Path(sys.argv[1])
argv = json.loads((stub / "sortie" / "casting" / "argv.json").read_text())
assert "--persos" in argv and "--actions" not in argv and "--pose-fixe" not in argv, f"modes étrangers : {argv}"
ids = argv[argv.index("--persos") + 1].split(",")
assert ids == ["CH--9101", "CH--9102", "CH--9103", "CH--9104"], f"ids reçus : {ids}"
shots = json.loads((stub / "sortie" / "casting" / "catalogue.json").read_text())
assert len(shots) == 4, f"{len(shots)} shots"
familles = sorted({s["famille"] for s in shots})
modeles = sorted({s["model"] for s in shots})
assert familles == ["bny", "kmn"] and modeles == ["illustrious", "noobai"], (familles, modeles)
nyalia = sum(1 for s in shots for l in s["casting"]["lora_loaders"].get("662", {}).get("loras", []) if l.get("name") == "nyalia")
assert nyalia == 2, f"nyalia attendue sur 2 shots, got {nyalia}"
print("POST-RUN OK : --persos CH--9101..9104 reçus, 2 familles × 2 modèles, LoRA nyalia sur 2 shots")
PY

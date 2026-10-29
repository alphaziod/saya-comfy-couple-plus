#!/usr/bin/env bash
# V4 native smoke: anti-oversight finalisation: COMPLETE FLOW of the casting work.
# Test Tauri window (feature smoke-test), isolated database
# (test-results/casting-parcours-v4/<run>), stub engine
# (scripts/casting-stub-engine.py) plugged in via SAYA_ENGINE_DIR with a fixture
# negatifs_figes.json registry (2 validated CH--9201/9202, outside the real registry),
# EMPTY diverted ComfyUI output (SAYA_COMFY_OUTPUT_DIR) to isolate the
# browser, fake ComfyUI accepting /prompt: no GPU, never the installed app,
# never the real engine. Prior build — BOTH features are mandatory:
# without `custom-protocol` the app does not embed the UI ("Could not connect"):
#   npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
base="$root/test-results/casting-parcours-v4"
stub="$base/engine-stub"
comfyout="$base/comfy-output"
rm -rf "$stub"
mkdir -p "$stub" "$base" "$comfyout"
cp "$root/scripts/casting-stub-engine.py" "$stub/test_solo.py"
python3 - "$stub/negatifs_figes.json" <<'PY'
import json, sys
from pathlib import Path
rows = [
    {"neg_id": "CH--9201", "nom": "Delta Parcours", "slug": "delta-parcours", "model": "noobai", "famille": "hum", "source": "oc", "bank_id": ""},
    {"neg_id": "CH--9202", "nom": "Echo Parcours", "slug": "echo-parcours", "model": "illustrious", "famille": "bny", "source": "banque", "bank_id": "CH-00002"},
]
Path(sys.argv[1]).write_text(json.dumps({"valides": rows}, ensure_ascii=False, indent=2) + "\n")
PY
export SAYA_STUDIO_PRESETS="$root/presets"
export SAYA_ENGINE_DIR="$stub"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
# The Rotations action picker lists the engine action bank (no text field).
mkdir -p "$stub/data/actions"
printf '%s\n' '{"famille":"x","actions":{"x_pinces_cheveux":{"add":"(arms raised:1.1)","valide":true}}}' > "$stub/data/actions/smoke.actions.json"
printf '%s\n' '["x_pinces_cheveux"]' > "$stub/data/actions/_ordre.json"
export SAYA_COMFY_OUTPUT_DIR="$comfyout"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/casting-parcours-v4-native-smoke.js" \
  --root "$base" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --restart --timeout 420

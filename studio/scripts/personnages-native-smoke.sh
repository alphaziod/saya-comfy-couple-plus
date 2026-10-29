#!/usr/bin/env bash
# V1 native smoke: character targeting (large choice window). Test Tauri window
# (feature smoke-test), isolated database (test-results/casting-v1/<run>), stub
# engine (scripts/casting-stub-engine.py) plugged in via SAYA_ENGINE_DIR with a fake registry
# negatifs_figes.json (3 validated CH--9001..9003, outside the real registry), fake ComfyUI
# accepting /prompt: no GPU, never the installed app, never the real engine (it is
# proven by tests/tests_casting_persos.py on the engine side and tests/casting_persos.rs on the
# app side). Hermetic base preset: SAYA_STUDIO_PRESETS points to the repo's presets/
# (the preset imports the 96 validated ones into the isolated database at startup — the window,
# lists the stub engine's REGISTRY: 3 tiles, joined to the sheets by name). The 3
# test sheets come from the v11 fixture sidecar (Alpha/Beta/Gamma Test), indexed at
# runtime by the public `index` command — portraits linked without depending on gallery
# timing. Prior build — BOTH features are
# mandatory: without `custom-protocol` the app does not embed the UI (blank page, IPC
# dead):
#   npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
stub="$root/test-results/casting-v1/engine-stub"
mkdir -p "$stub" "$root/test-results/casting-v1"
cp "$root/scripts/casting-stub-engine.py" "$stub/test_solo.py"
python3 - "$stub/negatifs_figes.json" <<'PY'
import json, sys
from pathlib import Path
rows = [
    {"neg_id": "CH--9001", "nom": "Alpha Test", "slug": "alpha-test", "model": "noobai", "source": "oc", "bank_id": ""},
    {"neg_id": "CH--9002", "nom": "Beta Test", "slug": "beta-test", "model": "illustrious", "source": "banque", "bank_id": "CH-00002"},
    {"neg_id": "CH--9003", "nom": "Gamma Test", "slug": "gamma-test", "model": "noobai", "source": "banque", "bank_id": "CH-00003"},
]
Path(sys.argv[1]).write_text(json.dumps({"valides": rows}, ensure_ascii=False, indent=2) + "\n")
PY
export SAYA_STUDIO_PRESETS="$root/presets"
export SAYA_ENGINE_DIR="$stub"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/personnages-native-smoke.js" \
  --root "$root/test-results/casting-v1" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --restart --timeout 420

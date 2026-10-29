#!/usr/bin/env bash
# V2 native smoke: action targeting in casting. Test Tauri window (feature
# smoke-test), isolated database (test-results/casting-actions-v2/<run>), stub engine
# (scripts/actions-stub-engine.py) plugged in via SAYA_ENGINE_DIR with a fixture
# action bank in <stub>/data/actions/ (read-only via actions_lister),
# fake ComfyUI accepting /prompt: no GPU, never the installed app, never the real
# engine (proven by tests/tests_actions.py and crates/studio-core/tests/actions_bank.rs).
# Prior build — BOTH features are mandatory: without `custom-protocol` the app
# does not embed the UI and the webview shows "Could not connect to localhost":
#   npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
base="$root/test-results/casting-actions-v2"
stub="$base/engine-stub"
mkdir -p "$stub" "$base"
cp "$root/scripts/actions-stub-engine.py" "$stub/test_solo.py"
mkdir -p "$stub/data/actions"
cat > "$stub/data/actions/smoke.actions.json" <<'JSON'
{
  "famille": "x",
  "actions": {
    "x_pinces_cheveux": {"add": "(hair clips:1.3), hands clasped", "valide": true},
    "x_bridon": {"add": "(arm binder:1.2), arms strapped behind", "valide": true},
    "x_duo": {"add": "(duo pose:1.3)", "valide": true, "pas_solo": true},
    "x_vieux": {"add": "(old action:1.0)", "valide": false}
  }
}
JSON
# `_ordre.json`: CONVERGED A/B rule (slice D, 2026-10-26) — the window shows the
# EXACT engine draw pool (port of noyau.eligible_names): x_vieux (valide:false)
# and x_duo (pas_solo) left out, x_pinces_cheveux/x_bridon + their _flaccide sisters cloned.
cat > "$stub/data/actions/_ordre.json" <<'JSON'
["x_pinces_cheveux", "x_bridon", "x_duo", "x_vieux"]
JSON
export SAYA_ENGINE_DIR="$stub"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/casting-actions-native-smoke.js" \
  --root "$base" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --timeout 420

# Engine-side proof (stub): the stub RECEIVED the exact selection from the UI and the
# catalogue reflects the targeted-action cycle.
python3 - "$stub" <<'PY'
import json, sys
from pathlib import Path
stub = Path(sys.argv[1])
sortie = stub / "sortie" / "casting"
argv = json.loads((sortie / "argv.json").read_text())
def valeur(flag):
    return argv[argv.index(flag) + 1] if flag in argv else None
assert valeur("--actions") == "x_pinces_cheveux,x_bridon", f"--actions reçu = {valeur('--actions')!r}"
assert valeur("--casting") == "2", f"--casting reçu = {valeur('--casting')!r}"
assert "--persos" not in argv and "--pose-fixe" not in argv and "--poses" not in argv, f"modes étrangers : {argv}"
assert valeur("--graine") and valeur("--graine").isdigit(), "graine absente"
shots = json.loads((sortie / "catalogue.json").read_text())
assert len(shots) == 2, f"{len(shots)} shots"
for i, shot in enumerate(shots):
    attendue = ("x_pinces_cheveux", "x_bridon")[i % 2]
    assert shot["pose"] == attendue, f"{shot['stem']} : pose {shot['pose']!r} != ciblée {attendue!r}"
    assert shot.get("actions_ciblees") == ["x_pinces_cheveux", "x_bridon"], shot.get("actions_ciblees")
    assert shot["casting"]["action"] == attendue, "override passe 1 ≠ pose"
    assert shot["casting"]["seed"] == -1
print("POST-RUN OK : --actions x_pinces_cheveux,x_bridon reçu du moteur, catalogue = cycle [x_pinces_cheveux, x_bridon], passe 1 cohérente")
PY

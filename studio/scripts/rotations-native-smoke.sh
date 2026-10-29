#!/usr/bin/env bash
# U2 native smoke: casting rotations. Test Tauri window (feature smoke-test), isolated
# database (test-results/rotations-u2/<run>), stub engine (scripts/casting-stub-engine.py)
# plugged in via SAYA_ENGINE_DIR, fake ComfyUI accepting /prompt: no GPU, never
# the installed app, never the real engine (proven by tests/tests_rotations.py,
# tests/tests_casting.py and tests/casting.rs). Prior build — BOTH features are
# mandatory: without `custom-protocol` the app does not embed the UI and the webview shows
# "Could not connect to localhost" (blank page, all IPC hang):
#   npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
stub="$root/test-results/rotations-u2/engine-stub"
mkdir -p "$stub" "$root/test-results/rotations-u2"
cp "$root/scripts/casting-stub-engine.py" "$stub/test_solo.py"
# The "workflow complet" flow waits for the 6 passes: the repo's hermetic preset
# is required (split of passes 2 to 6 by split_phase_source on the app side).
export SAYA_STUDIO_PRESETS="$root/presets"
export SAYA_ENGINE_DIR="$stub"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
# The Rotations pickers list the engine registry and action bank (no text field):
# fixture character CH--9201 and action x_pinces_cheveux, outside the real data.
printf '%s\n' '{"valides":[{"neg_id":"CH--9201","nom":"Rotation Fixture","slug":"rotation-fixture","model":"noobai","famille":"hum","source":"oc","bank_id":""}]}' > "$stub/negatifs_figes.json"
mkdir -p "$stub/data/actions"
printf '%s\n' '{"famille":"x","actions":{"x_pinces_cheveux":{"add":"(arms raised:1.1)","valide":true}}}' > "$stub/data/actions/smoke.actions.json"
printf '%s\n' '["x_pinces_cheveux"]' > "$stub/data/actions/_ordre.json"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/rotations-native-smoke.js" \
  --root "$root/test-results/rotations-u2" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --restart --timeout 420

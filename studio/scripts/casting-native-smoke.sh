#!/usr/bin/env bash
# U1 native smoke: 1-to-1 casting. Test Tauri window (feature smoke-test), isolated
# database (test-results/casting-u1/<run>), stub engine (scripts/casting-stub-engine.py)
# plugged in via SAYA_ENGINE_DIR, fake ComfyUI accepting /prompt: no GPU, never
# the installed app, never the real engine (proven by tests/tests_casting.py
# and tests/casting.rs). Prior build — BOTH features are mandatory: without
# `custom-protocol` the app does not embed the UI (blank page, dead IPC):
#   npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
stub="$root/test-results/casting-u1/engine-stub"
mkdir -p "$stub" "$root/test-results/casting-u1"
cp "$root/scripts/casting-stub-engine.py" "$stub/test_solo.py"
# The "workflow complet" flow waits for the 6 passes: the repo's hermetic preset
# is required (split of passes 2 to 6 by split_phase_source on the app side).
export SAYA_STUDIO_PRESETS="$root/presets"
export SAYA_ENGINE_DIR="$stub"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/casting-native-smoke.js" \
  --root "$root/test-results/casting-u1" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --restart --timeout 420

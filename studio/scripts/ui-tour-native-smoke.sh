#!/usr/bin/env bash
# UI tour: synthetic casting batch then one capture per page (isolated, stub engine).
# Tauri window, isolated database, stub engine (SAYA_ENGINE_DIR), fake ComfyUI:
# no GPU, never the installed app. Prior build (BOTH features mandatory):
#   npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
stub="$root/test-results/ui-tour/engine-stub"
mkdir -p "$stub" "$root/test-results/ui-tour"
cp "$root/scripts/casting-stub-engine.py" "$stub/test_solo.py"
export SAYA_STUDIO_PRESETS="$root/presets"
export SAYA_ENGINE_DIR="$stub"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/ui-tour-native-smoke.js" \
  --root "$root/test-results/ui-tour" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --timeout 600

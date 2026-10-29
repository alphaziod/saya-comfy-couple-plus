#!/usr/bin/env bash
# Native smoke: incomplete batches and resume (Codex 27-10, verified by ZCode). Test
# Tauri window, isolated database, stub engine (SAYA_ENGINE_DIR), fake ComfyUI:
# no GPU, never the installed app. Prior build (BOTH features mandatory):
#   npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
stub="$root/test-results/casting-lot-resume/engine-stub"
mkdir -p "$stub" "$root/test-results/casting-lot-resume"
cp "$root/scripts/casting-stub-engine.py" "$stub/test_solo.py"
export SAYA_STUDIO_PRESETS="$root/presets"
export SAYA_ENGINE_DIR="$stub"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/casting-lot-resume-native-smoke.js" \
  --root "$root/test-results/casting-lot-resume" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --restart --timeout 420

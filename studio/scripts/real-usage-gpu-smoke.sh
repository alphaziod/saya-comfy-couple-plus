#!/usr/bin/env bash
# Real-usage GPU smoke: REAL ComfyUI (127.0.0.1:8188, started by the user's `comfy` command),
# REAL engine code from an isolated sandbox copy whose pass-1 prompts are forced to SFW,
# SFW copy of the presets. Real user data, presets and engine are never written.
# Prepare first (see docs/TESTING.md "Real-usage GPU check"):
#   test-results/real-usage/sandbox/generez  (moteur/sandbox.py copy + SFW patch)
#   test-results/real-usage/presets          (SFW copy of presets/)
# Other scenario: REAL_USAGE_SCRIPT=real-usage-pull20-smoke.js REAL_USAGE_RUNS=pull20-runs REAL_USAGE_RESTART= bash $0
# Build: npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
bench="$root/test-results/real-usage"
[ -f "$bench/sandbox/generez/test_solo.py" ] || { echo "missing SFW engine sandbox"; exit 1; }
grep -q "REAL-USAGE TEST BENCH" "$bench/sandbox/generez/moteur/comfy.py" || { echo "engine sandbox is not SFW-patched"; exit 1; }
[ -d "$bench/presets/passes" ] || { echo "missing SFW presets"; exit 1; }
curl -sf -m 5 http://127.0.0.1:8188/system_stats > /dev/null || { echo "ComfyUI is not running on 8188"; exit 1; }
mkdir -p "$bench/comfy-workflows" "$bench/comfy-output-view"
export SAYA_STUDIO_PRESETS="$bench/presets"
export SAYA_ENGINE_DIR="$bench/sandbox/generez"
export SAYA_COMFY_WORKFLOWS="$bench/comfy-workflows"
export SAYA_COMFY_OUTPUT_DIR="$bench/comfy-output-view"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/${REAL_USAGE_SCRIPT:-real-usage-gpu-smoke.js}" \
  --root "$bench/${REAL_USAGE_RUNS:-runs}" \
  --ui-workflow "$bench/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  ${REAL_USAGE_RESTART---restart} --timeout 7200

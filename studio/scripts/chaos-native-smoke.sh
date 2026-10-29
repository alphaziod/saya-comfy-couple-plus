#!/usr/bin/env bash
# Chaos smoke: spam + seeded random monkey on the debug smoke build, fake ComfyUI (no GPU),
# stub engine, SFW presets copy, HOME and every external folder isolated so random clicks
# (LoRA organize, cleanup, settings paths…) can never reach real data.
# Optional: CHAOS_LIBRARY=<folder of real test images> CHAOS_SEED=<n> CHAOS_STEPS=<n>
# Build: npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
bench="$root/test-results/chaos"
home="$bench/home"
mkdir -p "$bench" "$home/AI/ComfyUI/models/loras" "$home/Images" "$home/Downloads" "$bench/engine-stub"
cp "$root/scripts/casting-stub-engine.py" "$bench/engine-stub/test_solo.py"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$bench/engine-stub"
presets="$root/test-results/real-usage/presets"
[ -d "$presets" ] || presets="$root/presets"
script="$bench/chaos-injected.js"
{
  printf 'window.__CHAOS_LIBRARY__=%s;\n' "$(python3 -c 'import json,os,sys;print(json.dumps(os.environ.get("CHAOS_LIBRARY") or None))')"
  printf 'window.__CHAOS_SEED__=%s;window.__CHAOS_STEPS__=%s;\n' "${CHAOS_SEED:-20261029}" "${CHAOS_STEPS:-2000}"
  cat "$root/scripts/chaos-native-smoke.js"
} > "$script"
export HOME="$home"
export SAYA_STUDIO_PRESETS="$presets"
export SAYA_ENGINE_DIR="$bench/engine-stub"
export SAYA_COMFYUI_DIR="$home/AI/ComfyUI"
export SAYA_COMFY_WORKFLOWS="$bench/comfy-workflows"
export SAYA_COMFY_OUTPUT_DIR="$bench/comfy-output-view"
export XDG_PICTURES_DIR="$home/Images"
mkdir -p "$SAYA_COMFY_WORKFLOWS" "$SAYA_COMFY_OUTPUT_DIR"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$script" \
  --root "$bench/runs" \
  --ui-workflow "$presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --timeout 3600

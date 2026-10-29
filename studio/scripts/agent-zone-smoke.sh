#!/usr/bin/env bash
# Sandbox runner for exploratory testing agents. Runs the smoke build with an injected test
# script inside test-results/agent-zone: isolated HOME, data, config, cache, pictures,
# ComfyUI folders, stub engine and FAKE ComfyUI (no GPU). The zone's image library (copies of
# real SFW test renders) is exposed to the script as window.__ZONE_LIBRARY__, and a writable
# scratch folder as window.__ZONE_SCRATCH__.
# Usage: bash scripts/agent-zone-smoke.sh <script.js> [run-name]
set -euo pipefail
root="$(pwd)"
zone="$root/test-results/agent-zone"
script="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
name="${2:-$(basename "$1" .js)}"
[ -d "$zone/library" ] || { echo "zone not prepared: $zone/library missing"; exit 1; }
case "$script" in "$zone"/*|"$root"/scripts/*) ;; *) echo "script must live in $zone or scripts/"; exit 1;; esac
home="$zone/home"; scratch="$zone/scratch/$name"
mkdir -p "$home/AI/ComfyUI/models/loras" "$home/Images" "$home/Downloads" "$zone/engine-stub" "$scratch" "$zone/runs"
cp "$root/scripts/casting-stub-engine.py" "$zone/engine-stub/test_solo.py"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$zone/engine-stub" > /dev/null
presets="$root/test-results/real-usage/presets"; [ -d "$presets" ] || presets="$root/presets"
injected="$scratch/injected.js"
{
  printf 'window.__ZONE_LIBRARY__=%s;window.__ZONE_SCRATCH__=%s;\n' \
    "$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$zone/library")" \
    "$(python3 -c 'import json,sys;print(json.dumps(sys.argv[1]))' "$scratch")"
  cat "$script"
} > "$injected"
export HOME="$home" SAYA_STUDIO_PRESETS="$presets" SAYA_ENGINE_DIR="$zone/engine-stub"
export SAYA_COMFYUI_DIR="$home/AI/ComfyUI" SAYA_COMFY_WORKFLOWS="$zone/comfy-workflows"
export SAYA_COMFY_OUTPUT_DIR="$zone/comfy-output-view" XDG_PICTURES_DIR="$home/Images"
mkdir -p "$SAYA_COMFY_WORKFLOWS" "$SAYA_COMFY_OUTPUT_DIR"
python3 "$root/scripts/v12-native-smoke.py" \
  --binary "$root/target/debug/saya-studio" \
  --script "$injected" \
  --root "$zone/runs/$name" \
  --ui-workflow "$presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  ${ZONE_RESTART:+--restart} --timeout "${ZONE_TIMEOUT:-1800}"

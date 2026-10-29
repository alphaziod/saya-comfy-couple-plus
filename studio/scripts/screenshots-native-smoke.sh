#!/usr/bin/env bash
# README screenshots (Generation, Casting, Gallery) in an isolated profile: stub engine, mock ComfyUI
# that returns the PNGs of SCREENSHOT_IMAGES (SFW renders you choose) instead of synthetic outputs.
# Prior build: npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
: "${SCREENSHOT_IMAGES:?set SCREENSHOT_IMAGES to a folder of SFW PNG renders}"
stub="$root/test-results/screenshots/engine-stub"
mkdir -p "$stub"
cp "$root/scripts/casting-stub-engine.py" "$stub/test_solo.py"
export SAYA_STUDIO_PRESETS="$root/presets" SAYA_ENGINE_DIR="$stub" MOCK_COMFY_IMAGES="$SCREENSHOT_IMAGES"
export STUB_OC_NAMES="${STUB_OC_NAMES:-Aiko,Mira,Lena,Yuki,Sora,Hana,Rin,Noa}"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
extra=()
# Optional: a captured /object_info of your ComfyUI, so the Saya preset maps its real nodes.
[ -n "${OBJECT_INFO:-}" ] && extra=(--mock-object-info "$OBJECT_INFO")
python3 scripts/v12-native-smoke.py "${extra[@]}" \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/screenshots-native-smoke.js" \
  --root "$root/test-results/screenshots" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept --restart --timeout 900

#!/usr/bin/env bash
# i18n native smoke (Saya, 26-10): first start (popup 14 languages → French),
# relaunch (popup absent, language kept), Paramètres → Japanese/Arabic(RTL)/English(LTR).
# Test Tauri window, isolated database, zero GPU, never the installed app.
# Prior build — BOTH features are mandatory:
#   npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
mkdir -p "$root/test-results/v12-i18n"
# Captures and database exclusively in the test folder.
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/v12-i18n-native-smoke.js" \
  --root "$root/test-results/v12-i18n" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --capture-root --mock-comfy \
  --restart --timeout 90

#!/usr/bin/env bash
# Native smoke: bypass of a node from Generation. Test Tauri window, isolated
# database, fake ComfyUI that logs every /prompt: no GPU, never the
# installed app. Prior build:
#   npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
mkdir -p "$root/test-results/bypass-native"
export SAYA_STUDIO_PRESETS="$root/presets"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/bypass-native-smoke.js" \
  --root "$root/test-results/bypass-native" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --timeout 600
# Engine-side proof: the bypassed node appears in no submitted /prompt.
run="$(ls -td "$root"/test-results/bypass-native/*/ | head -1)"
python3 - "$run" <<'PY'
import json, sys, pathlib
run = pathlib.Path(sys.argv[1])
target = json.loads((run / 'report.json').read_text())['phases'][0]['native']['target']
prompts = [json.loads(l)['body']['prompt'] for l in (run / 'mock-journal.jsonl').read_text().splitlines() if json.loads(l)['path'] == '/prompt' and json.loads(l)['method'] == 'POST']
assert prompts, 'aucun /prompt soumis'
assert all(target not in p for p in prompts), f'node {target} soumis malgré le bypass'
print(f'PASS bypass : node {target} absent des {len(prompts)} /prompt soumis')
PY

#!/usr/bin/env bash
# Native smoke of the Casting "Brouillons" panel: a fake engine (casting stub + fake solo CLI
# + a tiny action bank) stands for GROK BUILD, the fake ComfyUI accepts the pass-1 prompts.
# Checks: draft saved in the test copy only, tested in the casting grid, promoted to the
# "real" (fake) engine on YES, deleted draft never reaches it. No GPU, no real engine.
set -euo pipefail
root="$(pwd)"
work="$root/test-results/action-drafts"
engine="$work/engine-stub"
rm -rf "$engine"
mkdir -p "$engine/data/actions"
cp "$root/scripts/casting-stub-engine.py" "$engine/test_solo.py"
cp "$root/scripts/action-drafts-fake-solo.py" "$engine/solo_cli.py"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$engine" > /dev/null
printf '%s' '{"requis":["add"],"champs":{"add":"str","note":"str","negative_add":"str","valide":"bool","pas_solo":"bool"}}' > "$engine/data/actions/schema.json"
printf '%s' '{"actions":{"debout_base":{"add":"standing","valide":true}}}' > "$engine/data/actions/debout.actions.json"
printf '%s' '{"actions":{}}' > "$engine/data/actions/assise.actions.json"
printf '%s' '["debout_base"]' > "$engine/data/actions/_ordre.json"
export SAYA_STUDIO_PRESETS="$root/presets" SAYA_ENGINE_DIR="$engine"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/action-drafts-native-smoke.js" \
  --root "$work" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --timeout 400
python3 - "$engine/data/actions" <<'PY'
import json, sys
from pathlib import Path
bank = Path(sys.argv[1])
debout = json.loads((bank / "debout.actions.json").read_text())["actions"]
assise = json.loads((bank / "assise.actions.json").read_text())["actions"]
assert debout.get("bras_leves_test", {}).get("add") == "arms up, waving", debout
assert debout["bras_leves_test"]["valide"] is True
assert "assise_jetee" not in assise, "rejected draft reached the real engine"
print("PASS action drafts: promoted draft in the real bank, rejected one absent")
PY

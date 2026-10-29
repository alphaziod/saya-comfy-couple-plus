#!/usr/bin/env bash
# Pass 3 native smoke — new Casting flow (Saya's feedback 2026-10-26). Test
# Tauri window (feature smoke-test), isolated database (test-results/casting-passe3/<run>),
# stub engine (scripts/casting-stub-engine.py) plugged in via SAYA_ENGINE_DIR with a
# fixture negatifs_figes.json registry (2 validated CH--9301/9302, outside the real registry)
# and a fixture action bank (2 available, 1 pas_solo, 1 not validated), EMPTY diverted
# ComfyUI output (SAYA_COMFY_OUTPUT_DIR), fake ComfyUI --mock-live (each
# prompt stays ~6 s in the queue: real window for the global bar and stop/pause).
# No GPU, never the installed app, never the real engine. Prior build — BOTH
# features are mandatory: without `custom-protocol` the app does not embed the UI:
#   npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
set -euo pipefail
root="$(pwd)"
base="$root/test-results/casting-passe3"
stub="$base/engine-stub"
comfyout="$base/comfy-output"
rm -rf "$stub"
mkdir -p "$stub" "$base" "$comfyout"
cp "$root/scripts/casting-stub-engine.py" "$stub/test_solo.py"
python3 - "$stub" <<'PY'
import json, sys
from pathlib import Path
stub = Path(sys.argv[1])
rows = [
    {"neg_id": "CH--9301", "nom": "Delta Passe3", "slug": "delta-passe3", "model": "noobai", "famille": "hum", "source": "oc", "bank_id": ""},
    {"neg_id": "CH--9302", "nom": "Echo Passe3", "slug": "echo-passe3", "model": "illustrious", "famille": "bny", "source": "banque", "bank_id": "CH-00002"},
]
stub.joinpath("negatifs_figes.json").write_text(json.dumps({"valides": rows}, ensure_ascii=False, indent=2) + "\n")
actions = stub / "data" / "actions"
actions.mkdir(parents=True, exist_ok=True)
(actions / "x.actions.json").write_text(json.dumps({
    "famille": "x",
    "actions": {
        "x_pinces": {"add": "(hair clips:1.3)", "valide": True},
        "x_bridon": {"add": "(arm binder:1.2)", "valide": True},
        "x_duo": {"add": "(duo pose:1.3)", "valide": True, "pas_solo": True},
        "x_vieux": {"add": "(old action:1.0)", "valide": False},
    },
}, ensure_ascii=False, indent=2) + "\n")
# `_ordre.json`: CONVERGED A/B rule (slice D, 2026-10-26) — the window shows the
# EXACT engine draw pool (port of noyau.eligible_names): x_vieux (valide:false)
# and x_duo (pas_solo) disappear, x_pinces/x_bridon + _flaccide sisters cloned.
(actions / "_ordre.json").write_text(json.dumps(["x_pinces", "x_bridon", "x_duo", "x_vieux"]) + "\n")
PY
export SAYA_STUDIO_PRESETS="$root/presets"
export SAYA_ENGINE_DIR="$stub"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
export SAYA_COMFY_OUTPUT_DIR="$comfyout"
python3 scripts/v12-native-smoke.py \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/casting-passe3-native-smoke.js" \
  --root "$base" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept --mock-live \
  --mock-object-info "$root/test-results/gpu-real/object_info.json" \
  --restart --timeout 420

# Stub-engine-side proof: the backend passed to the engine EXACTLY what the UI
# sent — the free batch with exclusion carries --exclude x_pinces, the targeting carries
# --persos CH--9301 without --exclude, every invocation carries a seed.
python3 - "$stub" <<'PY'
import json, sys
from pathlib import Path
lignes = [json.loads(l) for l in (Path(sys.argv[1]) / "sortie" / "casting" / "argv_journal.jsonl").read_text().splitlines() if l.strip()]
def valeur(argv, flag):
    return argv[argv.index(flag) + 1] if flag in argv else None
exclusion = [a for a in lignes if "--exclude" in a]
assert exclusion, f"aucune invocation avec --exclude : {lignes}"
assert all(valeur(a, "--exclude") == "x_pinces" for a in exclusion), exclusion
cible = [a for a in lignes if "--persos" in a]
assert cible and all(valeur(a, "--persos") == "CH--9301" for a in cible), cible
assert all("--exclude" not in a for a in cible), "le ciblage ne doit pas porter --exclude"
assert all(valeur(a, "--graine") and valeur(a, "--graine").isdigit() for a in lignes), "graine absente"
assert all(valeur(a, "--casting") for a in lignes), "--casting absent"
print(f"POST-RUN OK : {len(exclusion)} invocation(s) --exclude x_pinces, ciblage --persos CH--9301 sans exclude, graines présentes")
PY

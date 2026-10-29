#!/usr/bin/env bash
# Reopen the smoke build on an EXISTING isolated run folder (e.g. a real-usage pull) to check
# what the app does at start: preview cleanup, P1/P6 lineage, gallery content.
# Usage: bash scripts/reopen-check-smoke.sh <run dir> [comfy output dir]
# The comfy output dir defaults to the bench's isolated view; pass the real ComfyUI output only
# when the run manifests point at bench-created roots (cleanup stays inside each run's root).
set -euo pipefail
root="$(pwd)"
run="$(cd "$1" && pwd)"
[ -f "$run/data/studio.saya.desktop/studio.sqlite3" ] || { echo "no isolated database in $run"; exit 1; }
bench="$root/test-results/real-usage"
export XDG_DATA_HOME="$run/data" XDG_CONFIG_HOME="$run/config" XDG_CACHE_HOME="$run/cache"
export GDK_BACKEND=wayland SAYA_STUDIO_SMOKE_FULL_HD=1
export SAYA_STUDIO_PRESETS="$bench/presets" SAYA_ENGINE_DIR="$bench/sandbox/generez"
export SAYA_COMFY_WORKFLOWS="$bench/comfy-workflows" SAYA_COMFY_OUTPUT_DIR="${2:-$bench/comfy-output-view}"
export SAYA_STUDIO_SMOKE_SCRIPT="$root/scripts/reopen-check-smoke.js"
report="$run/data/studio.saya.desktop/smoke-report.json"
rm -f "$report"
"$root/target/debug/saya-studio" > "$run/reopen.log" 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 240); do [ -f "$report" ] && break; sleep 0.5; done
[ -f "$report" ] || { echo "no report"; tail -20 "$run/reopen.log"; exit 1; }
cat "$report"; echo
grep -h "Preview cleanup" "$run/reopen.log" || true

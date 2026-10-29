#!/usr/bin/env bash
# Smoke test of the INSTALLED package (/usr/bin/saya-studio): fully isolated XDG dirs,
# ComfyUI dir and pictures dir, so the real user data is never opened. Checks that the
# window opens, a fresh database is migrated to the current schema and the process stays
# up; takes one screenshot of its own window. Requires a niri Wayland session.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
binary="${SAYA_STUDIO_BINARY:-/usr/bin/saya-studio}"
run="$root/test-results/installed-app/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$run"/{data,config,cache,pictures,comfyui,engine}
export XDG_DATA_HOME="$run/data" XDG_CONFIG_HOME="$run/config" XDG_CACHE_HOME="$run/cache"
export XDG_PICTURES_DIR="$run/pictures" SAYA_COMFYUI_DIR="$run/comfyui" SAYA_ENGINE_DIR="$run/engine"
"$binary" > "$run/app.log" 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true' EXIT
window=""
for _ in $(seq 1 150); do
  window="$(niri msg --json windows | python3 -c "import json,sys; print(next((w['id'] for w in json.load(sys.stdin) if w.get('pid')==$pid),''))")"
  [ -n "$window" ] && break
  sleep 0.1
done
[ -n "$window" ] || { echo "FAIL: no window for pid $pid"; exit 1; }
niri msg action focus-window --id "$window"
niri msg action fullscreen-window --id "$window"
sleep 8
kill -0 "$pid" || { echo "FAIL: process exited"; cat "$run/app.log"; exit 1; }
grim "$run/installed-window.png"
db="$run/data/studio.saya.desktop/studio.sqlite3"
[ -f "$db" ] || { echo "FAIL: database not created at $db"; exit 1; }
version="$(sqlite3 "file:$db?mode=ro" 'PRAGMA user_version')"
integrity="$(sqlite3 "file:$db?mode=ro" 'PRAGMA integrity_check')"
echo "binary=$binary pid=$pid window=$window schema=$version integrity=$integrity"
echo "screenshot=$run/installed-window.png"
[ "$version" = "31" ] && [ "$integrity" = "ok" ] && echo "PASS installed-app smoke"

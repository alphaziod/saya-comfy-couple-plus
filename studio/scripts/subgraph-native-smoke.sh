#!/usr/bin/env bash
set -euo pipefail
root="$(pwd)"
run="$(mktemp -d "$root/test-results/subgraph-native-XXXXXX")"
export XDG_DATA_HOME="$run/data" XDG_CONFIG_HOME="$run/config" XDG_CACHE_HOME="$run/cache"
export SAYA_SUBGRAPH_ENGINE_ROOT="${1:-$root/../saya-prompt-engine}"
export SAYA_STUDIO_SMOKE_SCRIPT="$run/inspect.js" SAYA_STUDIO_SMOKE_FULL_HD=1 GDK_BACKEND=x11
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
python3 - <<'PY'
import json, os
from pathlib import Path
target = Path(os.environ['SAYA_STUDIO_SMOKE_SCRIPT'])
target.write_text('window.__SUBGRAPH_ENGINE_ROOT__=' + json.dumps(str(Path(os.environ['SAYA_SUBGRAPH_ENGINE_ROOT']).resolve())) + ';\n' + Path('scripts/subgraph-native-smoke.js').read_text())
PY
status=0
timeout 90 xvfb-run -a -s '-screen 0 1920x1080x24' dbus-run-session -- "$root/target/debug/saya-studio" || status=$?
cp "$XDG_DATA_HOME/studio.saya.desktop/smoke-report.json" "$root/test-results/subgraph-native-report.json"
exit "$status"

#!/usr/bin/env bash
# Linux only. Run from the repository root. Uses an isolated display and database.
set -euo pipefail
if [[ "${1:-}" != --inside ]]; then
    exec xvfb-run -a -s '-screen 0 1280x900x24' dbus-run-session -- bash "$0" --inside
fi
root="$(pwd)"
mkdir -p "$root/test-results/native-data" "$root/test-results/native-config" "$root/test-results/native-cache"
export XDG_DATA_HOME="$root/test-results/native-data"
export XDG_CONFIG_HOME="$root/test-results/native-config"
export XDG_CACHE_HOME="$root/test-results/native-cache"
export GDK_BACKEND=x11
"$root/target/debug/saya-studio" > "$root/test-results/native-runtime.log" 2>&1 &
app_pid=$!
trap 'kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true' EXIT
sleep 8
kill -0 "$app_pid"
import -window root "$root/test-results/interface-native.png"
python3 - <<'PY'
from pathlib import Path
import sqlite3
path = Path('test-results/native-data/studio.saya.desktop/studio.sqlite3').resolve()
with sqlite3.connect(path.as_uri() + '?mode=ro', uri=True) as db:
    assert db.execute('PRAGMA user_version').fetchone() == (9,)
    assert db.execute('PRAGMA integrity_check').fetchone() == ('ok',)
    assert db.execute('SELECT count(*) FROM characters').fetchone() == (0,)
print('PASS: native process alive, schema v5, database integrity. Inspect screenshot for the WebView/IPC result.')
PY

#!/usr/bin/env bash
set -euo pipefail
root="$(pwd)"
# Reuses only the named synthetic fixture, never the user's application data.
python3 scripts/provenance-native-fixture.py
export XDG_DATA_HOME="$root/test-results/provenance-native/data"
export XDG_CONFIG_HOME="$root/test-results/provenance-native/config"
export XDG_CACHE_HOME="$root/test-results/provenance-native/cache"
export SAYA_STUDIO_SMOKE_SCRIPT="$root/scripts/references-native-smoke.js"
export SAYA_STUDIO_SMOKE_FULL_HD=1
export GDK_BACKEND=x11
status=0
timeout 90 xvfb-run -a -s '-screen 0 1920x1080x24' dbus-run-session -- "$root/target/debug/saya-studio" || status=$?
cp "$XDG_DATA_HOME/studio.saya.desktop/smoke-report.json" "$root/test-results/references-native-report.json"
exit "$status"

#!/usr/bin/env bash
set -euo pipefail
root="$(pwd)"
python3 scripts/lora-native-fixture.py --root test-results/navigation-native
export XDG_DATA_HOME="$root/test-results/navigation-native/data"
export XDG_CONFIG_HOME="$root/test-results/navigation-native/config"
export XDG_CACHE_HOME="$root/test-results/navigation-native/cache"
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
export SAYA_STUDIO_SMOKE_SCRIPT="$root/scripts/navigation-native-smoke.js"
export SAYA_STUDIO_SMOKE_FULL_HD=1
export GDK_BACKEND=x11
export SAYA_STUDIO_SMOKE_SLOW_PREFERENCES=1
status=0
timeout 120 xvfb-run -a -s '-screen 0 1920x1080x24' dbus-run-session -- "$root/target/debug/saya-studio" || status=$?
cp "$XDG_DATA_HOME/studio.saya.desktop/smoke-report.json" "$root/test-results/navigation-native-report.json"
exit "$status"

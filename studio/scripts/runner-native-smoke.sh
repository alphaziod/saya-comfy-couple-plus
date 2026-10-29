#!/usr/bin/env bash
set -euo pipefail
root="$(pwd)"
export XDG_DATA_HOME="$root/test-results/runner-native/data"
export XDG_CONFIG_HOME="$root/test-results/runner-native/config"
export XDG_CACHE_HOME="$root/test-results/runner-native/cache"
mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
export SAYA_STUDIO_SMOKE_SCRIPT="$root/scripts/runner-native-smoke.js"
export SAYA_STUDIO_SMOKE_FULL_HD=1
export GDK_BACKEND=x11
status=0
timeout 120 xvfb-run -a -s '-screen 0 1920x1080x24' dbus-run-session -- "$root/target/debug/saya-studio" || status=$?
cp "$XDG_DATA_HOME/studio.saya.desktop/smoke-report.json" "$root/test-results/runner-native-report.json"
exit "$status"

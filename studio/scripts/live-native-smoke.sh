#!/usr/bin/env bash
set -euo pipefail
root="$(pwd)"
mkdir -p test-results/live-native/{data,config,cache}
export XDG_DATA_HOME="$root/test-results/live-native/data"
export XDG_CONFIG_HOME="$root/test-results/live-native/config"
export XDG_CACHE_HOME="$root/test-results/live-native/cache"
export SAYA_STUDIO_SMOKE_SCRIPT="$root/scripts/live-native-smoke.js"
export GDK_BACKEND=x11
exec timeout 90 xvfb-run -a -s '-screen 0 1280x900x24' dbus-run-session -- "$root/target/debug/saya-studio"

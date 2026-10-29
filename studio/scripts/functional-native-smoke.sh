#!/usr/bin/env bash
# Requires the smoke-test build. Own database; never submits a prompt.
set -euo pipefail
root="$(pwd)"
mkdir -p test-results/functional-native/{data,config,cache}
export XDG_DATA_HOME="$root/test-results/functional-native/data"
export XDG_CONFIG_HOME="$root/test-results/functional-native/config"
export XDG_CACHE_HOME="$root/test-results/functional-native/cache"
export SAYA_STUDIO_SMOKE_SCRIPT="$root/scripts/functional-native-smoke.js"
export GDK_BACKEND=x11
exec timeout 180 xvfb-run -a -s '-screen 0 1280x900x24' dbus-run-session -- "$root/target/debug/saya-studio"

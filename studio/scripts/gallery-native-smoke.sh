#!/usr/bin/env bash
# Build first: npm run tauri -- build --debug --no-bundle --features smoke-test -- --locked -j 2
# The smoke-test binary runs with an isolated database; private originals are read only.
set -euo pipefail
root="$(pwd)"
mkdir -p test-results/gallery-native/{data,config,cache}
export XDG_DATA_HOME="$root/test-results/gallery-native/data"
export XDG_CONFIG_HOME="$root/test-results/gallery-native/config"
export XDG_CACHE_HOME="$root/test-results/gallery-native/cache"
export SAYA_STUDIO_SMOKE_SCRIPT="$root/scripts/gallery-native-smoke.js"
export GDK_BACKEND=x11
exec timeout 300 xvfb-run -a -s '-screen 0 1280x900x24' dbus-run-session -- "$root/target/debug/saya-studio"

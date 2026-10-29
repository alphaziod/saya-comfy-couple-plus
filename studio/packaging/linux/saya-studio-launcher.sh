#!/bin/sh
# Launcher for formats that cannot install under the hardcoded /usr/share/saya-studio
# (Flatpak, Snap, Nix, AppImage). Points the app at the data directory of the package
# through the environment variables the app already honours, then execs the real binary.
# SAYA_STUDIO_DATA: directory holding presets/, civitai_bridge.py, saya_native_host.py,
# sidecars/ (default: ../share/saya-studio relative to this script).
here="$(cd "$(dirname "$0")" && pwd)"
data="${SAYA_STUDIO_DATA:-$here/../share/saya-studio}"
export SAYA_STUDIO_PRESETS="${SAYA_STUDIO_PRESETS:-$data/presets}"
export SAYA_PASSES_DIR="${SAYA_PASSES_DIR:-$data/presets/passes}"
export SAYA_CIVITAI_BRIDGE="${SAYA_CIVITAI_BRIDGE:-$data/civitai_bridge.py}"
export SAYA_NATIVE_HOST="${SAYA_NATIVE_HOST:-$data/saya_native_host.py}"
export SAYA_SEMANTIC_SCRIPT="${SAYA_SEMANTIC_SCRIPT:-$data/sidecars/f09-semantic-embed.py}"
exec "$here/saya-studio-bin" "$@"

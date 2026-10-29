#!/usr/bin/env bash
set -euo pipefail
root="$(pwd)"
base="$(mktemp -d "$root/test-results/casting-connected-XXXXXX")"
mkdir -p "$base/engine-stub/sortie/reprise-fixture" "$base/comfy-output"
cp scripts/casting-stub-engine.py "$base/engine-stub/test_solo.py"
python3 - "$base/engine-stub/test_solo.py" <<'PYDELAY'
from pathlib import Path
import sys
p=Path(sys.argv[1]);p.write_text("import time; time.sleep(2)\n"+p.read_text())
PYDELAY
python3 - "$base/engine-stub/sortie/reprise-fixture/reprise.png" <<'PY'
import struct,zlib,json,sys
from pathlib import Path
def chunk(k,d):return struct.pack('>I',len(d))+k+d+struct.pack('>I',zlib.crc32(k+d)&0xffffffff)
w=Path('scripts/fixtures/v15.2-subgraph-ui.json').read_bytes()
Path(sys.argv[1]).write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',2,2,8,2,0,0,0))+chunk(b'tEXt',b'workflow\0'+w)+chunk(b'IDAT',zlib.compress((b'\0'+b'\x88\xaa\xbb'*2)*2))+chunk(b'IEND',b''))
PY
export SAYA_ENGINE_DIR="$base/engine-stub" SAYA_COMFY_OUTPUT_DIR="$base/comfy-output" SAYA_STUDIO_PRESETS="$root/presets"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
python3 scripts/v12-native-smoke.py --binary "$root/target/debug/saya-studio" --script "$root/scripts/casting-connected-native-smoke.js" --root "$base/runs" --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" --mock-comfy --mock-accept --mock-object-info "$root/test-results/gpu-real/object_info.json" --restart --timeout 300

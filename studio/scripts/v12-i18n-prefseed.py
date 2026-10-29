#!/usr/bin/env python3
"""Legacy fixture helper: keep captureRoot inside the isolated test directory.
The native runner now supports --capture-root directly.
"""
import json, sqlite3, sys
from pathlib import Path
base = Path(sys.argv[1])
db = base / "data" / "studio.saya.desktop" / "studio.sqlite3"
if not db.exists():
    print("pas encore de base (le harnais la crée) — rien à pré-seeder"); sys.exit(0)
con = sqlite3.connect(db)
con.execute(
    "INSERT INTO preferences(key,value_json) VALUES('captureRoot',?) "
    "ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",
    (json.dumps(str(base / "images")),),
)
con.commit(); con.close()
print("captureRoot pré-seedé vers", base / "images")

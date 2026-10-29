#!/usr/bin/env bash
set -euo pipefail
root="$(pwd)"
export XDG_DATA_HOME="$root/test-results/jobs-native/data"
export XDG_CONFIG_HOME="$root/test-results/jobs-native/config"
export XDG_CACHE_HOME="$root/test-results/jobs-native/cache"
mkdir -p "$XDG_DATA_HOME/studio.saya.desktop" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
python3 - <<'PY'
import os,sqlite3,pathlib,json,uuid
path=pathlib.Path(os.environ['XDG_DATA_HOME'])/'studio.saya.desktop/studio.sqlite3'
with sqlite3.connect(path) as db:
 if not db.execute('pragma user_version').fetchone()[0]:
  for migration in sorted(pathlib.Path('migrations').glob('*.sql')): db.executescript(migration.read_text())
  db.execute('pragma user_version=9')
 db.execute("INSERT OR IGNORE INTO workflow_revisions(id,name,format,document_json,source_sha256) VALUES('native-jobs-workflow','Fixture jobs','comfy_api','{}','synthetic')")
 db.execute("INSERT OR IGNORE INTO generation_jobs(id,workflow_revision_id,state,seed,request_snapshot_json,comfy_prompt_id) VALUES('native-job','native-jobs-workflow','queued','18446744073709551615',?,?)",(json.dumps({'seed':'18446744073709551615','synthetic':True}),str(uuid.uuid4())))
PY
export SAYA_STUDIO_SMOKE_SCRIPT="$root/scripts/jobs-native-smoke.js"
export GDK_BACKEND=x11
exec timeout 90 xvfb-run -a -s '-screen 0 1280x900x24' dbus-run-session -- "$root/target/debug/saya-studio"

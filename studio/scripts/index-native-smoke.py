#!/usr/bin/env python3
"""Synthetic interrupted-state UI fixture; process interruption separately qualified on a real copy."""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import uuid
import time

root=Path.cwd();run=root/'test-results'/('index-native-'+uuid.uuid4().hex[:10]);images=run/'images';images.mkdir(parents=True)
# Use the protocol fixture's valid PNG, no private data or engine access.
from importlib.util import spec_from_file_location,module_from_spec
spec=spec_from_file_location('generation_fixture',root/'scripts/generation-native-server.py');fixture=module_from_spec(spec);spec.loader.exec_module(fixture)
data=fixture.png()
for n in range(1000):(images/f'{n:04}.png').write_bytes(data)
(images/'broken.png').write_bytes(b'broken')
env={**os.environ,'SAYA_STUDIO_SMOKE_COMFY_PORT':'9','SAYA_STUDIO_SMOKE_FULL_HD':'1','SAYA_STUDIO_SMOKE_SCRIPT':str(root/'scripts/index-native-smoke.js'),'GDK_BACKEND':'wayland'}
for name in ['DATA','CONFIG','CACHE']:
 path=run/name.lower();path.mkdir();env['XDG_'+name+'_HOME']=str(path)
database=run/'data/studio.saya.desktop/studio.sqlite3';database.parent.mkdir()
subprocess.run([str(root/'target/debug/studio-maintenance'),'status',str(database)],check=True,stdout=subprocess.DEVNULL)
with sqlite3.connect(database) as db:
 db.execute("INSERT INTO index_runs(root_path,state) VALUES(?,'interrupted')",(str(images),))
with (run/'native.log').open('w') as log:
 process=subprocess.Popen([str(root/'target/debug/saya-studio')],env=env,stdout=log,stderr=subprocess.STDOUT)
 try:
  window=None;deadline=time.monotonic()+15
  while process.poll() is None and time.monotonic()<deadline:
   windows=json.loads(subprocess.check_output(['niri','msg','--json','windows']))
   window=next((w for w in windows if w.get('pid')==process.pid),None)
   if window:break
   time.sleep(0.1)
  assert window,'Own test window missing'
  subprocess.run(['niri','msg','action','fullscreen-window','--id',str(window['id'])],check=True)
  code=process.wait(timeout=90)
 finally:
  if process.poll() is None:process.terminate();process.wait(timeout=10)
report=json.loads((database.parent/'smoke-report.json').read_text());report['directory']=str(run)
assert all(p.read_bytes()==data for p in images.glob('[0-9]*.png'))
with sqlite3.connect(database) as db:assert db.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
(root/'test-results/index-native-report.json').write_text(json.dumps(report,indent=2))
print(json.dumps(report,indent=2));raise SystemExit(0 if code==0 and report['ok'] else 1)

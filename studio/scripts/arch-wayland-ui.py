#!/usr/bin/env python3
"""Native WebView checks on the copy already qualified by arch-wayland-smoke.py.

Requires the smoke-test build. The distributed release is tested separately.
"""
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import urllib.request

root = Path.cwd()
release = json.loads((root / 'test-results/arch-wayland-report.json').read_text())
assert release['ok'] and os.environ.get('WAYLAND_DISPLAY')
run = Path(release['directory'])
database = run / 'data/studio.saya.desktop/studio.sqlite3'
with sqlite3.connect(database) as db:
    expected = dict(zip(['assets', 'characters', 'jobs'], [db.execute(sql).fetchone()[0] for sql in [
        'select count(*) from assets', 'select count(*) from characters where archived=0', 'select count(*) from generation_jobs']]))
    assert db.execute("select value_json from preferences where key='selectedTab'").fetchone()[0] == '"data"'
with urllib.request.urlopen('http://127.0.0.1:8188/system_stats', timeout=5) as response:
    version = json.load(response)['system']['comfyui_version']
reports = []
for phase in [1, 2]:
    script = run / f'ui-{phase}.js'
    config = {'counts': expected, 'database': str(database), 'version': version, 'phase': phase}
    script.write_text('const expected=' + json.dumps(config) + ';\n' + r'''
(async()=>{
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}}),report={ok:false,errors:[]};
 window.addEventListener('error',e=>report.errors.push(e.message));
 window.addEventListener('unhandledrejection',e=>report.errors.push(String(e.reason)));
 const wait=async fn=>{const end=Date.now()+20000;while(!fn()){if(Date.now()>end)throw Error('UI condition timed out: '+fn);await new Promise(r=>setTimeout(r,80));}};
 const button=text=>[...document.querySelectorAll('nav button')].find(b=>b.textContent.trim()===text);
 try{
  const bootstrap=await invoke('get_bootstrap');
  if(bootstrap.schemaVersion!==9||Object.keys(expected.counts).some(k=>bootstrap.counts[k]!==expected.counts[k]))throw Error('Copied catalog mismatch');
  if(expected.phase===1){
   await wait(()=>document.querySelector('h1')?.textContent==='Données et sauvegardes'&&document.body.textContent.includes(expected.database));
   button('ComfyUI').click();
  }
  await wait(()=>document.querySelector('h1')?.textContent==='ComfyUI'&&document.body.textContent.includes('Connecté · version '+expected.version));
  await wait(()=>button('ComfyUI')?.getAttribute('aria-current')==='page');
  const status=await api('connection');if(!status.connected||status.version!==expected.version)throw Error('Live engine mismatch');
  if((await api('preferences')).selectedTab!=='engine')throw Error('Navigation not persisted');
  const actions=await invoke('smoke_actions');if(actions.some(a=>['submit','index','save_bindings','prepare','prepare_couple'].includes(a)))throw Error('Unexpected mutation');
  report.counts=bootstrap.counts;report.engineVersion=status.version;report.phase=expected.phase;report.domVerified=true;report.gpuSubmitted=false;report.ok=!report.errors.length;
 }catch(e){report.failure=String(e?.message??e);}
 await invoke('finish_smoke',{report});
})();
''')
    env = {**os.environ, 'XDG_DATA_HOME': str(run / 'data'), 'XDG_CONFIG_HOME': str(run / 'config'), 'XDG_CACHE_HOME': str(run / 'cache'), 'GDK_BACKEND': 'wayland', 'WAYLAND_DEBUG': '1', 'SAYA_STUDIO_SMOKE_SCRIPT': str(script)}
    with (run / f'ui-{phase}.log').open('w') as log:
        subprocess.run([str(root / 'target/debug/saya-studio')], env=env, stdout=log, stderr=subprocess.STDOUT, timeout=60, check=True)
    report = json.loads((database.parent / 'smoke-report.json').read_text())
    reports.append(report)
    assert report['ok'] and report['phase'] == phase, report
    assert 'xdg_toplevel' in (run / f'ui-{phase}.log').read_text()
result = {'ok': True, 'directory': str(run), 'releaseTestSeparate': True, 'runs': reports}
(root / 'test-results/arch-wayland-ui-report.json').write_text(json.dumps(result, indent=2))
print(json.dumps(result, indent=2))

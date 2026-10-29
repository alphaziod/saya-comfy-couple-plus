#!/usr/bin/env python3
"""Launch an extracted release on Niri using a verified copy of a user database.

Only the two windows created by this test are closed. No package installation,
GPU submission, original database mutation, or screenshot/clipboard access.
"""
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import time

extracted, source = map(lambda value: Path(value).resolve(), sys.argv[1:3])
assert os.environ.get('WAYLAND_DISPLAY') and os.environ.get('NIRI_SOCKET'), 'Niri/Wayland session required'
root = Path.cwd()
executable = Path(sys.argv[3]).resolve() if len(sys.argv) > 3 else extracted / 'usr/bin/saya-studio'
report_name = 'appimage-wayland-report.json' if len(sys.argv) > 3 else 'arch-wayland-report.json'
run = Path(tempfile.mkdtemp(prefix='arch-wayland-', dir=root / 'test-results'))
appdir = run / 'data/studio.saya.desktop'
appdir.parent.mkdir()


def hashes():
    return {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in [source, Path(str(source) + '-wal')] if p.exists()}


def summary(path):
    with sqlite3.connect('file:' + str(path) + '?mode=ro', uri=True) as db:
        return {'schema': db.execute('pragma user_version').fetchone()[0], 'integrity': db.execute('pragma integrity_check').fetchone()[0], 'counts': {t: db.execute('select count(*) from ' + t).fetchone()[0] for t in ['assets', 'characters', 'workflow_revisions', 'generation_jobs']}}


def windows():
    return json.loads(subprocess.check_output(['niri', 'msg', '--json', 'windows']))


before_hash = hashes()
before = summary(source)
report = {'ok': False, 'directory': str(run), 'before': before, 'runs': [], 'installed': False, 'gpuSubmitted': False, 'executable': str(executable)}
try:
    subprocess.run([str(extracted / 'usr/bin/saya-studio-maintenance'), 'backup', str(source), str(appdir)], check=True, stdout=(run / 'backup-manifest.json').open('w'))
    with sqlite3.connect(appdir / 'studio.sqlite3') as db:
        db.execute("INSERT INTO preferences(key,value_json) VALUES('selectedTab','\"data\"') ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json")
    env = {**os.environ, 'XDG_DATA_HOME': str(run / 'data'), 'XDG_CONFIG_HOME': str(run / 'config'), 'XDG_CACHE_HOME': str(run / 'cache'), 'GDK_BACKEND': 'wayland', 'WAYLAND_DEBUG': '1'}
    for phase in [1, 2]:
        log_path = run / f'launch-{phase}.log'
        with log_path.open('w') as log:
            process = subprocess.Popen([str(executable)], env=env, stdout=log, stderr=subprocess.STDOUT)
            window = None
            try:
                end = time.monotonic() + 30
                while time.monotonic() < end:
                    assert process.poll() is None, f'Release exited: {process.returncode}'
                    window = next((w for w in windows() if w.get('pid') == process.pid), None)
                    if window:
                        break
                    time.sleep(0.2)
                assert window, 'No native Niri window for release process'
                time.sleep(6)
                assert process.poll() is None
                assert window.get('app_id') == 'saya-studio', window
                subprocess.run(['niri', 'msg', 'action', 'close-window', '--id', str(window['id'])], check=True, stdout=subprocess.DEVNULL)
                assert process.wait(timeout=15) == 0
            finally:
                if process.poll() is None:
                    process.terminate(); process.wait(timeout=10)
        observed = summary(appdir / 'studio.sqlite3')
        assert observed['schema'] == 8 and observed['integrity'] == 'ok'
        assert observed['counts'] == before['counts'], observed
        protocol = log_path.read_text(errors='replace')
        assert 'wl_registry' in protocol and 'xdg_toplevel' in protocol, 'Wayland protocol not observed'
        report['runs'].append({'pid': process.pid, 'windowId': window['id'], 'appId': window['app_id'], 'exit': process.returncode, 'database': observed, 'waylandProtocol': True})
    assert hashes() == before_hash, 'Original database or WAL changed during test'
    backups = list((appdir / 'backups').glob('*'))
    if before['schema'] < 8:
        assert backups and any(summary(p)['schema'] == before['schema'] for p in backups), 'Original schema backup missing'
    report.update(ok=True, originalDatabaseUnchanged=True, migrationBackupCount=len(backups), visualContentVerified=False)
except Exception as error:
    report['failure'] = str(error)
finally:
    (root / 'test-results' / report_name).write_text(json.dumps(report, indent=2))
print(json.dumps(report, indent=2))
raise SystemExit(0 if report['ok'] else 1)

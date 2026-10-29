#!/usr/bin/env python3
"""Capture the gallery image-detail panel in an isolated native smoke window.

Runs the same isolated fixture as the proof smoke (v11 gallery fixture + the
synthetic metadata PNG), holds the window open at two panel states and takes
grim screenshots. Never touches the installed app or the real profile.
"""
import argparse
import json
import os
import runpy
import sqlite3
import subprocess
import sys
import time
import uuid
from pathlib import Path


def main():
    project = Path(__file__).resolve().parent.parent

    def raise_window(window_id):
        # The test window must be on top of everything (a full-screen video
        # stream from another program would pollute the capture).
        subprocess.run(['niri', 'msg', 'action', 'focus-window', '--id', str(window_id)], check=True, timeout=5)
        subprocess.run(['niri', 'msg', 'action', 'fullscreen-window', '--id', str(window_id)], check=True, timeout=5)
        time.sleep(0.4)

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--script', type=Path, help='Smoke JS (default: capture hold script, or proof script with --proof)')
    parser.add_argument('--root', type=Path, default=project / 'test-results/gallery-detail')
    parser.add_argument('--timeout', type=int, default=120)
    parser.add_argument('--proof', action='store_true', help='Run the assertions script to completion instead of holding for captures')
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    script = (args.script or project / ('scripts/gallery-detail-metadata-smoke.js' if args.proof else 'scripts/gallery-detail-capture.js')).resolve(strict=True)
    run = args.root.resolve() / uuid.uuid4().hex[:12]
    run.mkdir(parents=True, exist_ok=False)
    report = {'ok': False, 'directory': str(run), 'captures': []}
    process = None
    try:
        fixture = subprocess.run(
            [sys.executable, str(project / 'scripts/v11-gallery-fixture.py'), '--root', str(run), '--extra', '0'],
            cwd=project, capture_output=True, text=True, check=True)
        (run / 'fixture.log').write_text(fixture.stdout + fixture.stderr)
        runpy.run_path(str(project / 'scripts/gallery-detail-fixture.py'))['write_fixture'](run / 'images/metadata.png')
        env = {**os.environ, 'GDK_BACKEND': 'wayland'}
        for name in ('DATA', 'CONFIG', 'CACHE'):
            path = run / name.lower()
            path.mkdir(exist_ok=True)
            env['XDG_' + name + '_HOME'] = str(path)
        env['SAYA_STUDIO_SMOKE_SCRIPT'] = str(script)
        log = (run / 'capture.log').open('w')
        process = subprocess.Popen([str(binary)], cwd=project, env=env, stdout=log, stderr=subprocess.STDOUT)
        end = time.monotonic() + args.timeout
        window = None
        while process.poll() is None and time.monotonic() < end:
            windows = json.loads(subprocess.check_output(['niri', 'msg', '--json', 'windows'], timeout=5))
            window = next((w for w in windows if w.get('pid') == process.pid), None)
            if window:
                break
            time.sleep(0.1)
        assert window, 'Smoke window not found'
        subprocess.run(['niri', 'msg', 'action', 'fullscreen-window', '--id', str(window['id'])], check=True, timeout=5)

        if args.proof:
            code = process.wait(timeout=max(1.0, end - time.monotonic()))
            native_path = run / 'data/studio.saya.desktop/smoke-report.json'
            native = json.loads(native_path.read_text()) if native_path.exists() else {'ok': False, 'failure': 'No native report'}
            report['native'] = native
            report['exitCode'] = code
            report['ok'] = code == 0 and native.get('ok') is True
        else:
            db = run / 'data/studio.saya.desktop/studio.sqlite3'

            def filters_json():
                try:
                    # WAL database held open by the application: concurrent
                    # read connection (mode=ro fails for lack of write access to the shm).
                    with sqlite3.connect(db, timeout=2) as connection:
                        row = connection.execute("SELECT value_json FROM preferences WHERE key='galleryFilters'").fetchone()
                        return str(row[0]) if row else ''
                except Exception as error:
                    return 'READER-ERROR: ' + str(error)

            while 'metadata.png ready' not in filters_json() and time.monotonic() < end:
                time.sleep(0.3)
            assert 'metadata.png ready' in filters_json(), 'Folded-state sentinel missing: ' + filters_json()[:120]
            time.sleep(1.5)
            shot1 = run / 'detail-top.png'
            raise_window(window['id'])
            subprocess.run(['grim', '-t', 'png', str(shot1)], check=True, timeout=20)
            report['captures'].append(str(shot1))
            # Signals to the script that the top state is captured (direct line,
            # outside the application's set_preference whitelist).
            with sqlite3.connect(db, timeout=2) as connection:
                connection.execute("INSERT OR REPLACE INTO preferences(key,value_json) VALUES('galleryDetailShot','\"1-done\"')")
                connection.commit()
            stage2 = time.monotonic()
            while 'metadata.png done' not in filters_json() and time.monotonic() - stage2 < 45:
                time.sleep(0.3)
            assert 'metadata.png done' in filters_json(), 'Bottom-state sentinel missing: ' + filters_json()[:120]
            time.sleep(1.5)
            shot2 = run / 'detail-annotations.png'
            raise_window(window['id'])
            subprocess.run(['grim', '-t', 'png', str(shot2)], check=True, timeout=20)
            report['captures'].append(str(shot2))
            report['ok'] = True
    except Exception as error:
        report['failure'] = f'{type(error).__name__}: {error}'
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        (run / 'report.json').write_text(json.dumps(report, indent=2))
        print(json.dumps(report, indent=2))
    return 0 if report['ok'] else 1


if __name__ == '__main__':
    sys.exit(main())

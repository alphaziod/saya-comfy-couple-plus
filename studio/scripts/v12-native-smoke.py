#!/usr/bin/env python3
"""Run one injected smoke (optionally restart) in an isolated native Niri window.

Example: python3 scripts/v12-native-smoke.py --binary /tmp/private-target/debug/saya-studio --restart
Build separately with custom-protocol,smoke-test. Never chooses a cached binary implicitly.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import time
import uuid


def write_large_png(path, width, height):
    import struct, zlib
    def chunk(kind, body):
        return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body) & 0xffffffff)
    # Hard-edged stripes and checks so a blur measurably changes pixels.
    rows = []
    for y in range(height):
        band = (y // 32) % 2
        row = bytearray(b'\0')
        line = bytearray(width * 3)
        for x0 in range(0, width, 16):
            value = 255 if ((x0 // 16) + band) % 2 else 0
            line[x0 * 3:(x0 + 16) * 3] = bytes((value, 255 - value, 128)) * min(16, width - x0)
        row += line
        rows.append(bytes(row))
    raw = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
    raw += chunk(b'IDAT', zlib.compress(b''.join(rows), 6)) + chunk(b'IEND', b'')
    path.write_bytes(raw)


SEED_JOB_PROMPT = 'P1 exact,  double espace\nligne é 🌸'


def seed_job(database):
    import sqlite3
    graph = {'1': {'class_type': 'CLIPTextEncode', 'inputs': {'text': SEED_JOB_PROMPT}},
             '2': {'class_type': 'KSampler', 'inputs': {'seed': 18446744073709551615, 'steps': 28, 'cfg': 5.5}},
             '3': {'class_type': 'CheckpointLoaderSimple', 'inputs': {'ckpt_name': 'fixture-ckpt_name.safetensors'}},
             '4': {'class_type': 'LoraLoader', 'inputs': {'lora_name': 'fixture-lora.safetensors', 'strength_model': 0.8, 'strength_clip': 0.9}}}
    snapshot = json.dumps({'workflowId': 'smoke-wf', 'prompt': graph}, ensure_ascii=False)
    with sqlite3.connect(database) as db:
        assert db.execute('pragma user_version').fetchone()[0] == 9, 'seed-job expects the v9 fixture'
        db.execute("INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES('smoke-wf','Smoke workflow',?,'comfy_api',?)",
                   (hashlib.sha256(b'smoke-wf').hexdigest(), json.dumps(graph, ensure_ascii=False)))
        db.execute("INSERT INTO generation_jobs(id,workflow_revision_id,seed,state,request_snapshot_json) VALUES('smoke-job','smoke-wf','18446744073709551615','succeeded',?)", (snapshot,))


def main():
    project = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--script', type=Path, default=project / 'scripts/v12-characters-native-smoke.js')
    parser.add_argument('--root', type=Path, default=project / 'test-results/v12-native')
    parser.add_argument('--restart', action='store_true')
    parser.add_argument('--timeout', type=int, default=150)
    parser.add_argument('--mock-live', action='store_true', help='Fake identified WebSocket sampling and preview events')
    parser.add_argument('--mock-object-info', type=Path, help='Read-only object_info capture for real workflow mappings')
    parser.add_argument('--mock-comfy', action='store_true', help='Point the app at scripts/mock-comfy.py (loopback, journaled)')
    parser.add_argument('--comfy-offline', action='store_true', help='Point the app at a closed loopback port (engine offline)')
    parser.add_argument('--mock-accept', action='store_true', help='Let the mock accept /prompt (synthetic outputs, no GPU)')
    parser.add_argument('--gallery-filter-fixture', action='store_true', help='Add four synthetic checkpoint/LoRA combinations')
    parser.add_argument('--lora-fixture', action='store_true', help='Create six tiny synthetic model files and previews in isolated storage')
    parser.add_argument('--saya-run-fixture', action='store_true', help='Persist a synthetic six-phase run with one succeeded job; never submits a phase')
    parser.add_argument('--seed-job', action='store_true', help='Insert one succeeded job (u64 max seed, exact prompt) into the v9 fixture')
    parser.add_argument('--capture-root', action='store_true', help='G1: isolated capture root + complete workflow for organized delivery')
    parser.add_argument('--presets', type=Path, help='Isolate SAYA_STUDIO_PRESETS (default: leave the system presets visible)')
    parser.add_argument('--grok-fixture', action='store_true', help='Add synthetic observed Grok JSON exports')
    parser.add_argument('--missing-portrait', action='store_true', help='Move own alpha000 fixture between initial and restart phases')
    parser.add_argument('--generation-metadata', action='store_true', help='Add a synthetic Comfy PNG with exact seed and SOLO branches')
    parser.add_argument('--large', help='Add images/large.png of WIDTHxHEIGHT (editor performance)')
    parser.add_argument('--gallery-extra', type=int, default=0, help='Add N synthetic gallery images for scale tests')
    parser.add_argument('--gallery-seed', type=int, default=0, help='Insert N metadata-only synthetic assets after the fixture')
    parser.add_argument('--gallery-seed-images', action='store_true', help='Create tiny PNG files for the seeded assets')
    parser.add_argument('--semantic-python')
    parser.add_argument('--semantic-model')
    parser.add_argument('--semantic-script')
    parser.add_argument('--semantic-idle', type=float)
    parser.add_argument('--ui-workflow', type=Path, help='UI workflow opened by the smoke (read-only, hash-checked)')
    parser.add_argument('--ui-workflow-blocked', type=Path, help='incomplete UI workflow expected to be refused (read-only, hash-checked)')
    args = parser.parse_args()
    assert args.binary.is_absolute(), '--binary must be absolute'
    binary = args.binary.resolve(strict=True)
    assert binary.is_file() and os.access(binary, os.X_OK), 'Binary is not executable'
    script = args.script.resolve(strict=True)
    assert script.is_file(), 'Smoke script missing'
    assert args.timeout > 0, 'Timeout must be positive'
    assert os.environ.get('WAYLAND_DISPLAY') and os.environ.get('NIRI_SOCKET'), 'Niri/Wayland session required'
    ui_workflow = args.ui_workflow.resolve(strict=True) if args.ui_workflow else None
    if ui_workflow:
        assert ui_workflow.is_file(), 'UI workflow fixture missing'
    ui_workflow_blocked = args.ui_workflow_blocked.resolve(strict=True) if args.ui_workflow_blocked else None
    if ui_workflow_blocked:
        assert ui_workflow_blocked.is_file(), 'blocked UI workflow fixture missing'
    assert shutil.which('niri'), 'niri executable missing'
    run = args.root.resolve() / uuid.uuid4().hex[:12]
    run.mkdir(parents=True, exist_ok=False)
    report = {'ok': False, 'directory': str(run), 'binary': str(binary),
              'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'script': str(script), 'backend': 'niri', 'gpuGeneration': False, 'phases': []}
    if ui_workflow:
        report['uiWorkflow'] = str(ui_workflow)
        report['uiWorkflowSha256'] = hashlib.sha256(ui_workflow.read_bytes()).hexdigest()
    if ui_workflow_blocked:
        report['uiWorkflowBlocked'] = str(ui_workflow_blocked)
        report['uiWorkflowBlockedSha256'] = hashlib.sha256(ui_workflow_blocked.read_bytes()).hexdigest()
    try:
        fixture = subprocess.run([sys.executable, str(project / 'scripts/v11-gallery-fixture.py'), '--root', str(run), '--extra', str(max(0, args.gallery_extra))],
                                 cwd=project, capture_output=True, text=True, check=True)
        (run / 'fixture.log').write_text(fixture.stdout + fixture.stderr)
        if args.gallery_seed > 0:
            db_path = run / 'data/studio.saya.desktop/studio.sqlite3'
            connection = sqlite3.connect(db_path)
            found = connection.execute('SELECT id FROM libraries LIMIT 1').fetchone()
            if found:
                library = found[0]
            else:
                library = 'scale-library'
                connection.execute(
                    'INSERT INTO libraries(id,label,root_path,platform) VALUES(?,?,?,?)',
                    (library, 'Scale', str(run / 'images'), 'linux'),
                )
            connection.executemany(
                'INSERT INTO assets(id,library_id,relative_path,byte_size,availability,metadata_json,metadata_state,width,height) VALUES(?,?,?,?,?,?,?,?,?)',
                [(f'scale-{i:06}', library, f'scale/{i:06}.png', 0, 'present', '{}', 'absent', 512, 512) for i in range(args.gallery_seed)],
            )
            connection.commit()
            connection.close()
            if args.gallery_seed_images:
                import struct, zlib
                images = run / 'images' / 'scale'
                images.mkdir(parents=True, exist_ok=True)
                def chunk(kind, body):
                    return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body) & 0xffffffff)
                raw = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 320, 320, 8, 2, 0, 0, 0))
                raw += chunk(b'IDAT', zlib.compress((b'\0' + bytes((80, 120, 160)) * 320) * 320)) + chunk(b'IEND', b'')
                for i in range(args.gallery_seed):
                    (images / f'{i:06}.png').write_bytes(raw)
                report['gallerySeedImages'] = args.gallery_seed
            report['gallerySeed'] = args.gallery_seed
        grok_files = []
        grok_scene_file = None
        if args.grok_fixture:
            records = [{'characters': [{'id': 'CH-TEST', 'name': 'Grok Test', 'identity_prompt': ' exact  \nété 🌸', 'anatomy_prompt': ' anatomy ', 'bg_options': [{'bg_id': 'A', 'bg_name': 'Garden', 'scene_bg': ' garden  '}, {'bg_id': 'B', 'bg_name': 'Night', 'scene_bg': ' night\n'}]}]}, [{'id': 'variant-test', 'name': 'Grok Test', 'bank_id': 'CH-TEST', 'p1': 'variant exact\n', 'anatomy': 'variant anatomy', 'bg': 'variant bg'}]]
            scene_path = run / 'grok-scenes.py'
            scene_path.write_text("raise RuntimeError('must never execute')\nPOLE_PERSO = {'CH-TEST': {'scene': ' garden  ', 'intime': ' room  \\nété 🌸'}}\n")
            grok_scene_file = str(scene_path)
            for index, record in enumerate(records):
                path = run / f'grok-{index}.json'
                path.write_text(json.dumps(record, ensure_ascii=False))
                grok_files.append(str(path))
        if args.generation_metadata:
            import runpy
            runpy.run_path(str(project / 'scripts/v15-metadata-fixture.py'))['write_fixture'](run / 'images/metadata.png')
        if args.seed_job or args.saya_run_fixture:
            seed_job(run / 'data/studio.saya.desktop/studio.sqlite3')
            report['seedJob'] = 'smoke-job'
        if True:  # Every native smoke must publish inside its isolated run, never the user gallery.
            # G1: isolated captures root + full workflow (SaveImage)
            # for the organised delivery <day>/final/ trial.
            captures = run / 'captures'
            captures.mkdir(parents=True, exist_ok=True)
            report['captureRoot'] = str(captures)
            graph = {'1': {'class_type': 'CheckpointLoaderSimple', 'inputs': {'ckpt_name': 'fixture-ckpt_name.safetensors'}},
                     '2': {'class_type': 'CLIPTextEncode', 'inputs': {'clip': ['1', 1], 'text': 'capture finale exacte'}},
                     '3': {'class_type': 'CLIPTextEncode', 'inputs': {'clip': ['1', 1], 'text': 'neg'}},
                     '4': {'class_type': 'EmptyLatentImage', 'inputs': {'width': 64, 'height': 64, 'batch_size': 1}},
                     '5': {'class_type': 'KSampler', 'inputs': {'model': ['1', 0], 'positive': ['2', 0], 'negative': ['3', 0], 'latent_image': ['4', 0], 'seed': 18446744073709551615, 'steps': 4, 'cfg': 6.0, 'sampler_name': 'euler', 'scheduler': 'normal', 'denoise': 1.0}},
                     '6': {'class_type': 'VAEDecode', 'inputs': {'samples': ['5', 0], 'vae': ['1', 2]}},
                     '7': {'class_type': 'SaveImage', 'inputs': {'images': ['6', 0], 'filename_prefix': 'capture'}}}
            with sqlite3.connect(run / 'data/studio.saya.desktop/studio.sqlite3') as db:
                db.execute("INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES('smoke-capture-wf','Capture smoke',?,'comfy_api',?)",
                           (hashlib.sha256(b'smoke-capture-wf').hexdigest(), json.dumps(graph)))
                db.execute("INSERT OR REPLACE INTO preferences VALUES('captureRoot',?)", (json.dumps(str(captures)),))
            report['captureWorkflow'] = 'smoke-capture-wf'
        if args.saya_run_fixture:
            manifests = run / 'data/studio.saya.desktop/pass-states/saya-runs'
            manifests.mkdir(parents=True)
            manifest = {'id':'fixture-run-0001','sourcePath':'isolated-fixture.json','source':{},'masterSeed':'9007199254740993','isolatedRoot':'fixture-only','state':'running','createdAt':'2026-10-20T00:00:00Z','phases':{'1':{'job':'smoke-job','state':'submitted'}}}
            (manifests / 'fixture-run-0001.json').write_text(json.dumps(manifest))
            report['sayaRunFixture'] = 'synthetic manifest, phase 1 synchronizes with succeeded SQLite job'

        if args.gallery_filter_fixture:
            import struct, zlib
            for name, checkpoint, names in [('abc','Fixture-A',['A','B','C']),('ab','Fixture-B',['A','B']),('ac','Fixture-A',['A','C']),('a','Fixture-B',['A'])]:
                def chunk(kind, body):
                    return struct.pack('>I',len(body))+kind+body+struct.pack('>I',zlib.crc32(kind+body)&0xffffffff)
                graph={'1':{'class_type':'CheckpointLoaderSimple','inputs':{'ckpt_name':checkpoint}},'2':{'class_type':'Lora Loader (LoraManager)','inputs':{'loras':[{'name':n,'active':True} for n in names]}}}
                raw=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',16,16,8,2,0,0,0))+chunk(b'tEXt',b'prompt\0'+json.dumps(graph).encode())+chunk(b'IDAT',zlib.compress((b'\0'+bytes([90,120,180])*16)*16))+chunk(b'IEND',b'')
                (run / 'images' / f'filter-{name}.png').write_bytes(raw)
        if args.lora_fixture:
            loras = run / 'loras'
            for category in ['Portraits', 'Styles']:
                (loras / category).mkdir(parents=True)
                for index in range(3):
                    model = loras / category / f'Fixture-{category}-{index}.safetensors'
                    model.write_bytes(b'fixture only, not model weights')
                    write_large_png(model.with_suffix('.png'), 96, 128)
            with sqlite3.connect(run / 'data/studio.saya.desktop/studio.sqlite3') as db:
                db.execute("INSERT OR REPLACE INTO preferences VALUES('loraRoot',?)", (json.dumps(str(loras)),))
        if args.large:
            width, height = (int(v) for v in args.large.lower().split('x'))
            assert 0 < width <= 16384 and 0 < height <= 16384, '--large out of editor limits'
            write_large_png(run / 'images/large.png', width, height)
            report['large'] = [width, height]
        env = {**os.environ, 'GDK_BACKEND': 'wayland', 'SAYA_STUDIO_SMOKE_FULL_HD': '1'}
        if args.presets:
            # Isolation: the installed package's base preset (/usr/share)
            # must not import into the fixture database unless requested.
            args.presets.mkdir(parents=True, exist_ok=True)
            env['SAYA_STUDIO_PRESETS'] = str(args.presets)
            report['presets'] = str(args.presets)
        if args.semantic_idle:
            env['SAYA_SEMANTIC_IDLE_SECONDS'] = str(args.semantic_idle)
        for key, value in (('SAYA_SEMANTIC_PYTHON', args.semantic_python), ('SAYA_SEMANTIC_MODEL', args.semantic_model), ('SAYA_SEMANTIC_SCRIPT', args.semantic_script)):
            if value:
                # Do not dereference the venv Python symlink: site-packages live
                # beside the symlink, not at the resolved interpreter.
                env[key] = str(Path(value).absolute() if key == 'SAYA_SEMANTIC_PYTHON' else Path(value).resolve())
        if args.comfy_offline:
            import socket
            with socket.socket() as probe:
                probe.bind(('127.0.0.1', 0))
                env['SAYA_STUDIO_SMOKE_COMFY_PORT'] = str(probe.getsockname()[1])
            report['comfyOffline'] = True
        if args.mock_comfy:
            ready = run / 'mock-port'
            command = [sys.executable, str(project / 'scripts/mock-comfy.py'), '--journal', str(run / 'mock-journal.jsonl'), '--ready-file', str(ready)]
            if args.mock_live:
                command += ['--live']
            if args.mock_object_info:
                command += ['--object-info', str(args.mock_object_info.resolve())]
            mock = subprocess.Popen(command + (['--accept-prompts'] if args.mock_accept else []), stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)
            started = time.monotonic()
            while not ready.exists() and time.monotonic() - started < 10:
                time.sleep(0.05)
            assert ready.exists(), 'mock-comfy did not start'
            env['SAYA_STUDIO_SMOKE_COMFY_PORT'] = ready.read_text().strip()
            report['mockComfy'] = {'port': int(env['SAYA_STUDIO_SMOKE_COMFY_PORT']), 'acceptPrompts': args.mock_accept}
        for name in ('DATA', 'CONFIG', 'CACHE'):
            path = run / name.lower()
            path.mkdir(exist_ok=True)
            env['XDG_' + name + '_HOME'] = str(path)
        native_report = run / 'data/studio.saya.desktop/smoke-report.json'
        for phase in ('initial', 'restart') if args.restart else ('initial',):
            if phase == 'restart' and args.missing_portrait:
                (run / 'images/alpha000.png').rename(run / 'parked-portrait.png')
            previous = report['phases'][0]['native'] if report['phases'] else None
            injected = run / (phase + '.js')
            injected.write_text('window.__V15_UI_WORKFLOW__=' + json.dumps(str(ui_workflow) if ui_workflow else None) + ';\n' + 'window.__V15_UI_WORKFLOW_BLOCKED__=' + json.dumps(str(ui_workflow_blocked) if ui_workflow_blocked else None) + ';\n' + 'window.__V15_GROK_SCENE_FILE__=' + json.dumps(grok_scene_file) + ';\n' + 'window.__V15_GROK_FILES__=' + json.dumps(grok_files) + ';\n' + 'window.__V12_CAPTURE_ROOT__=' + json.dumps(report.get('captureRoot')) + ';\n' + 'window.__V12_SMOKE_PHASE__=' + json.dumps(phase) + ';\n'
                                'window.__V12_SMOKE_PREVIOUS__=' + json.dumps(previous) + ';\n' + script.read_text())
            native_report.unlink(missing_ok=True)
            env['SAYA_STUDIO_SMOKE_SCRIPT'] = str(injected)
            deadline = time.monotonic() + args.timeout
            with (run / (phase + '.log')).open('w') as log:
                process = subprocess.Popen([str(binary)], cwd=project, env=env, stdout=log, stderr=subprocess.STDOUT)
                phase_result = {'phase': phase, 'pid': process.pid}
                report['phases'].append(phase_result)
                try:
                    window = None
                    window_deadline = min(deadline, time.monotonic() + 15)
                    while process.poll() is None and time.monotonic() < window_deadline:
                        windows = json.loads(subprocess.check_output(['niri', 'msg', '--json', 'windows'], timeout=5))
                        window = next((w for w in windows if w.get('pid') == process.pid), None)
                        if window:
                            break
                        time.sleep(0.1)
                    assert window, 'Own native test window not found'
                    phase_result['window'] = window['id']
                    subprocess.run(['niri', 'msg', 'action', 'fullscreen-window', '--id', str(window['id'])], check=True, timeout=5)
                    code = process.wait(timeout=max(0.1, deadline - time.monotonic()))
                finally:
                    if process.poll() is None:
                        process.terminate()
                        try:
                            process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait(timeout=5)
            native = json.loads(native_report.read_text()) if native_report.exists() else {'ok': False, 'failure': 'No native report'}
            (run / (phase + '-report.json')).write_text(json.dumps(native, indent=2))
            report['phases'][-1].update({'exitCode': code, 'native': native})
            assert code == 0 and native.get('ok') is True, f'{phase} failed: exit {code}: {native.get("failure", native)}'
        if args.mock_comfy:
            journal = run / 'mock-journal.jsonl'
            lines = [json.loads(line) for line in journal.read_text().splitlines()] if journal.exists() else []
            prompts = [line for line in lines if line['method'] == 'POST' and line['path'] == '/prompt']
            report['mockComfy'].update({'requests': len(lines), 'promptSubmissions': len(prompts)})
            assert args.mock_accept or not prompts, f'{len(prompts)} /prompt submission(s) without --mock-accept'
        if ui_workflow:
            after = hashlib.sha256(ui_workflow.read_bytes()).hexdigest()
            assert after == report['uiWorkflowSha256'], 'UI workflow source was modified'
            report['uiWorkflowUnchanged'] = True
        if ui_workflow_blocked:
            after = hashlib.sha256(ui_workflow_blocked.read_bytes()).hexdigest()
            assert after == report['uiWorkflowBlockedSha256'], 'blocked UI workflow source was modified'
            report['uiWorkflowBlockedUnchanged'] = True
        report['ok'] = True
    except Exception as error:
        if report['phases'] and 'native_report' in locals() and native_report.exists():
            try:
                report['phases'][-1]['native'] = json.loads(native_report.read_text())
            except (OSError, ValueError):
                pass
        report['failure'] = f'{type(error).__name__}: {error}'
    finally:
        if 'mock' in locals() and mock.poll() is None:
            mock.terminate()
            mock.wait(timeout=5)
        (run / 'report.json').write_text(json.dumps(report, indent=2))
        print(json.dumps(report, indent=2))
    return 0 if report['ok'] else 1


if __name__ == '__main__':
    sys.exit(main())

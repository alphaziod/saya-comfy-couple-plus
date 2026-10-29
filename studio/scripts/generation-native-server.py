#!/usr/bin/env python3
"""Synthetic Comfy protocol fixture. No engine imports, GPU, or private inputs.

Run from the repository root after building with --features smoke-test.
Uses a fresh XDG directory and an ephemeral loopback port; never port 8188.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import socket
import sqlite3
import struct
import sys
import subprocess
import threading
import time
import uuid
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse


def schema():
    def node(inputs, outputs):
        return {'input': {'required': inputs}, 'output': outputs}
    return {
        'CheckpointLoaderSimple': node({'ckpt_name': [['synthetic.safetensors']]}, ['MODEL', 'CLIP', 'VAE']),
        'Lora Loader (LoraManager)': node({'model': ['MODEL'], 'text': ['AUTOCOMPLETE_TEXT_LORAS'], 'loras': ['LORAS']}, ['MODEL', 'CLIP', 'STRING', 'STRING']),
        'EmptyLatentImage': node({k: ['INT', {'min': 1, 'max': 8192}] for k in ['width', 'height', 'batch_size']}, ['LATENT']),
        'CLIPTextEncode': node({'text': ['STRING'], 'clip': ['CLIP']}, ['CONDITIONING']),
        'ConditioningCombine': node({'conditioning_1': ['CONDITIONING'], 'conditioning_2': ['CONDITIONING']}, ['CONDITIONING']),
        'KSampler': node({'model': ['MODEL'], 'positive': ['CONDITIONING'], 'negative': ['CONDITIONING'], 'latent_image': ['LATENT'], 'seed': ['INT', {'min': 0, 'max': 18446744073709551615}], 'steps': ['INT', {'min': 1, 'max': 10000}], 'cfg': ['FLOAT', {'min': 0, 'max': 100}], 'sampler_name': [['euler']], 'scheduler': [['normal']], 'denoise': ['FLOAT', {'min': 0, 'max': 1}]}, ['LATENT']),
        'VAEDecode': node({'samples': ['LATENT'], 'vae': ['VAE']}, ['IMAGE']),
        'SaveImage': node({'images': ['IMAGE'], 'filename_prefix': ['STRING']}, []),
    }


def png():
    def chunk(kind, data):
        return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('!2I5B', 2, 2, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(b'\0' + b'\x32\x66\x99' * 2 + b'\0' + b'\xaa\xbb\xcc' * 2)) + chunk(b'IEND', b'')


class Fixture:
    def __init__(self):
        self.lock = threading.RLock()
        self.jobs, self.history, self.requests, self.sockets = {}, {}, [], []
        self.ws_enabled = True
        self.progress_sent = 0
        self.reconnects = 0

    def event(self, kind, prompt, **data):
        raw = json.dumps({'type': kind, 'data': {'prompt_id': prompt, **data}}).encode()
        frame = b'\x81' + (bytes([len(raw)]) if len(raw) < 126 else b'\x7e' + struct.pack('!H', len(raw))) + raw
        with self.lock:
            for connection in self.sockets[:]:
                try:
                    connection.sendall(frame)
                except OSError:
                    self.sockets.remove(connection)
            if kind == 'progress':
                self.progress_sent += 1

    def disconnect(self):
        with self.lock:
            self.ws_enabled = False
            for connection in self.sockets[:]:
                try:
                    connection.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
            self.sockets.clear()

    def complete(self, prompt, failure=False):
        with self.lock:
            job = self.jobs[prompt]
            job['state'] = 'failed' if failure else 'succeeded'
            status = {'completed': True, 'status_str': 'error' if failure else 'success', 'messages': [['execution_error', {'exception_message': 'Synthetic node failure'}]] if failure else []}
            self.history[prompt] = {'prompt': job['row'], 'status': status, 'outputs': {} if failure else {'out': {'images': [{'filename': prompt + '.png', 'subfolder': '', 'type': 'output'}]}}}

    def run_job(self, prompt, scenario):
        time.sleep(1)
        with self.lock:
            self.jobs[prompt]['state'] = 'running'
        self.event('execution_start', prompt)
        # Repeated progress leaves enough time for WebView's 1 s polling.
        for _ in range(7):
            self.event('progress', prompt, node='sample', value=2, max=4)
            time.sleep(0.5)
        if scenario == 'COUPLE':
            self.disconnect()  # The completed output must be recovered without WS.
        self.complete(prompt, failure=scenario == 'ERROR')
        if scenario != 'COUPLE':
            self.event('execution_error' if scenario == 'ERROR' else 'execution_success', prompt,
                       node_id='sample', exception_type='SyntheticError', exception_message='Synthetic node failure' if scenario == 'ERROR' else '')


FIXTURE = Fixture()


class Handler(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    def handle(self):
        try:
            super().handle()
        except ConnectionResetError:
            pass  # Native process closes its keep-alive clients on exit.

    def log_message(self, *_):
        pass

    def reply(self, data, status=200, mime='application/json'):
        raw = data if isinstance(data, bytes) else json.dumps(data).encode()
        self.send_response(status)
        self.send_header('Content-Type', mime)
        self.send_header('Content-Length', str(len(raw)))
        self.end_headers()
        try:
            self.wfile.write(raw)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_GET(self):
        path = urlparse(self.path).path
        with FIXTURE.lock:
            FIXTURE.requests.append({'method': 'GET', 'path': self.path})
            if path == '/system_stats':
                return self.reply({'system': {'comfyui_version': '0.34.0'}, 'devices': []})
            if path == '/object_info':
                return self.reply(schema())
            if path == '/queue':
                return self.reply({key: [job['row'] for job in FIXTURE.jobs.values() if job['state'] == state] for key, state in [('queue_running', 'running'), ('queue_pending', 'queued')]})
            if path.startswith('/history'):
                prompt = path.removeprefix('/history/');return self.reply({prompt: FIXTURE.history[prompt]} if prompt in FIXTURE.history else FIXTURE.history if path == '/history' else {})
            if path == '/view':
                return self.reply(png(), mime='image/png')
            if path != '/ws':
                return self.reply({'error': 'unexpected path'}, 404)
            if not FIXTURE.ws_enabled:
                return self.reply({'error': 'deliberate WebSocket outage'}, 503)
            key = self.headers['Sec-WebSocket-Key']
            self.send_response(101)
            self.send_header('Upgrade', 'websocket');self.send_header('Connection', 'Upgrade')
            self.send_header('Sec-WebSocket-Accept', base64.b64encode(hashlib.sha1((key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode())
            self.end_headers();self.wfile.flush()
            FIXTURE.sockets.append(self.connection)
            FIXTURE.reconnects += 1
        try:
            while True:
                header = self.rfile.read(2)
                if len(header) != 2:
                    break
                opcode, size = header[0] & 15, header[1] & 127
                if size == 126:
                    size = struct.unpack('!H', self.rfile.read(2))[0]
                elif size == 127:
                    size = struct.unpack('!Q', self.rfile.read(8))[0]
                if size > 2_000_000 or not header[1] & 128:
                    break
                mask = self.rfile.read(4)
                payload = self.rfile.read(size)
                if len(mask) != 4 or len(payload) != size:
                    break
                payload = bytes(v ^ mask[i % 4] for i, v in enumerate(payload))
                if opcode == 8:
                    break
                if opcode == 9:
                    with FIXTURE.lock:
                        self.connection.sendall(b'\x8a' + bytes([size]) + payload)
        except OSError:
            pass
        finally:
            self.close_connection = True
            with FIXTURE.lock:
                if self.connection in FIXTURE.sockets:
                    FIXTURE.sockets.remove(self.connection)

    def do_POST(self):
        payload = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        with FIXTURE.lock:
            FIXTURE.requests.append({'method': 'POST', 'path': self.path, 'payload': payload})
            if self.path == '/queue':
                assert list(payload) == ['delete'] and len(payload['delete']) == 1
                job = FIXTURE.jobs[payload['delete'][0]]
                assert job['state'] == 'queued'
                job['state'] = 'cancelled'
                return self.reply({})
            if self.path != '/prompt':
                return self.reply({'error': 'unexpected mutation'}, 400)
            scenario = payload['prompt']['out']['inputs']['filename_prefix']
            assert scenario in ['SOLO', 'COUPLE', 'ERROR', 'CANCEL', 'LOST']
            prompt = str(uuid.uuid4())
            row = [len(FIXTURE.jobs), prompt, payload['prompt'], {**payload['extra_data'], 'client_id': payload['client_id']}, ['out']]
            FIXTURE.jobs[prompt] = {'state': 'queued', 'row': row, 'scenario': scenario}
            if scenario == 'LOST':
                # Accepted by engine but response lost. Recovery requires exact provenance.
                FIXTURE.jobs[prompt]['state'] = 'unobserved'
                threading.Timer(3, FIXTURE.complete, args=(prompt,)).start()
                self.close_connection = True
                self.connection.shutdown(socket.SHUT_RDWR)
                return
            self.reply({'prompt_id': prompt, 'number': row[0], 'node_errors': {}})
            if scenario != 'CANCEL':
                threading.Thread(target=FIXTURE.run_job, args=(prompt, scenario), daemon=True).start()


def main():
    wayland = sys.argv[1:] == ["--wayland"]
    assert not sys.argv[1:] or wayland, "Usage: generation-native-server.py [--wayland]"
    root = Path.cwd()
    run = root / 'test-results' / ('generation-native-' + uuid.uuid4().hex[:10])
    run.mkdir(parents=True)
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    assert server.server_port not in (0, 8188)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    env = {**os.environ, 'SAYA_STUDIO_SMOKE_COMFY_PORT': str(server.server_port), 'SAYA_STUDIO_SMOKE_FULL_HD': '1', 'GDK_BACKEND': 'wayland' if wayland else 'x11'}
    for name in ['DATA', 'CONFIG', 'CACHE']:
        path = run / name.lower();path.mkdir();env['XDG_' + name + '_HOME'] = str(path)
    report = {'ok': False, 'directory': str(run), 'port': server.server_port, 'gpu': False, 'backend': 'niri' if wayland else 'xvfb'}
    try:
        for phase in ['generation', 'generation-restart']:
            script = root / 'scripts' / (phase + '-native-smoke.js')
            if phase == 'generation-restart':
                source = script.read_text()
                script = run / 'restart.js'
                script.write_text('window.__GENERATION_EXPECTED__=' + json.dumps(report['generation']['saved']) + ';\n' + source)
            env['SAYA_STUDIO_SMOKE_SCRIPT'] = str(script)
            with (run / (phase + '.log')).open('w') as log:
                if wayland:
                    process = subprocess.Popen([str(root / 'target/debug/saya-studio')], env=env, stdout=log, stderr=subprocess.STDOUT)
                    try:
                        deadline = time.monotonic() + 15
                        window = None
                        while process.poll() is None and time.monotonic() < deadline:
                            windows = json.loads(subprocess.check_output(['niri', 'msg', '--json', 'windows']))
                            window = next((w for w in windows if w.get('pid') == process.pid), None)
                            if window:
                                break
                            time.sleep(0.1)
                        assert window, 'Own native test window not found'
                        subprocess.run(['niri', 'msg', 'action', 'fullscreen-window', '--id', str(window['id'])], check=True)
                        result_code = process.wait(timeout=150)
                    finally:
                        if process.poll() is None:
                            process.terminate()
                            process.wait(timeout=10)
                else:
                    result_code = subprocess.run(['timeout', '150', 'xvfb-run', '-a', '-s', '-screen 0 1920x1080x24', 'dbus-run-session', '--', str(root / 'target/debug/saya-studio')], env=env, stdout=log, stderr=subprocess.STDOUT).returncode
            native_path = run / 'data/studio.saya.desktop/smoke-report.json'
            native = json.loads(native_path.read_text()) if native_path.exists() else {'ok': False, 'failure': 'No native report'}
            (run / (phase + '-report.json')).write_text(json.dumps(native, indent=2))
            report[phase] = native
            assert result_code == 0 and native['ok'], (result_code, native)
        with sqlite3.connect('file:' + str(run / 'data/studio.saya.desktop/studio.sqlite3') + '?mode=ro', uri=True) as db:
            assert db.execute('pragma integrity_check').fetchone()[0] == 'ok'
            states = dict(db.execute('select state,count(*) from generation_jobs group by state'))
            assert states == {'succeeded': 3, 'failed': 1, 'cancelled': 1}, states
            assert db.execute('select count(*) from job_outputs').fetchone()[0] == 3
            assert db.execute('select count(*) from asset_characters where role="depicted"').fetchone()[0] == 4
            assert db.execute("select count(*) from job_events where event_type='submission_recovered'").fetchone()[0] == 1
            for expected in report['generation']['saved']:
                original, digest = db.execute('select document_json,source_sha256 from workflow_revisions where id=?', (expected['workflow'],)).fetchone()
                assert json.loads(original) == json.loads(expected['original'])
                assert digest == hashlib.sha256(expected['original'].encode()).hexdigest()
            report['database'] = {'states': states, 'integrity': 'ok', 'outputs': 3, 'associations': 4}
        posts = [r for r in FIXTURE.requests if r['method'] == 'POST']
        submitted = [r['payload'] for r in posts if r['path'] == '/prompt']
        assert len(submitted) == 5 and len({p['extra_data']['saya_studio_job_id'] for p in submitted}) == 5
        assert len(posts) == 6 and posts[-2]['path'] == '/queue', posts
        assert all(p['prompt']['sample']['inputs']['seed'] == 18446744073709551615 for p in submitted)
        assert FIXTURE.progress_sent >= 21
        assert any(r['path'].startswith('/history?') for r in FIXTURE.requests)
        report.update(ok=True, singleSubmissions=5, targetedCancellation=True, exactU64=True, websocketProgressEvents=FIXTURE.progress_sent)
    except Exception as error:
        report['failure'] = str(error)
    finally:
        FIXTURE.disconnect();server.shutdown();server.server_close()
        (run / 'requests.json').write_text(json.dumps(FIXTURE.requests, indent=2))
        (root / 'test-results/generation-native-report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps({**report, 'generation': {k: v for k, v in report.get('generation', {}).items() if k != 'saved'}}, indent=2))
    raise SystemExit(0 if report['ok'] else 1)


if __name__ == '__main__':
    main()

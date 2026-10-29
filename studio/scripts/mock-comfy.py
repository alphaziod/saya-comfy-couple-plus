#!/usr/bin/env python3
"""Loopback ComfyUI stand-in for native smokes and orchestration tests.

Serves a scrubbed object_info subset (scripts/fixtures/object_info-subset.json),
an idle system/queue/history, and journals every request as JSON lines.
POST /prompt is refused (HTTP 403) unless --accept-prompts is given; accepted
prompts never run anything, they only produce a synthetic history entry with
one PNG output per SaveImage node, served by /view.
Never a GPU, never a network beyond 127.0.0.1.
"""
import argparse
import json
import struct
import threading
import uuid
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

HERE = Path(__file__).resolve().parent


def png(width, height, rgb):
    def chunk(kind, body):
        return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body) & 0xffffffff)
    raw = b''.join(b'\0' + bytes(rgb) * width for _ in range(height))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b''))


def latent_bytes(seed):
    import hashlib, json
    header = json.dumps({"__metadata__": {"format": "pt"},
                         "latent_tensor": {"dtype": "F32", "shape": [1, 4, 8, 8], "data_offsets": [0, 1024]},
                         "latent_format_version_0": {"dtype": "F32", "shape": [0], "data_offsets": [1024, 1024]}},
                        separators=(',', ':')).encode()
    data = bytearray(1024)
    digest = hashlib.sha256(str(seed).encode()).digest()
    for i in range(1024):
        data[i] = digest[i % len(digest)]
    return len(header).to_bytes(8, 'little') + header + bytes(data)


class State:
    def __init__(self, journal, accept, drop_response=False, live=False):
        self.journal = journal
        self.accept = accept
        self.drop_response = drop_response
        self.live = live
        self.sockets = {}
        self.lock = threading.Lock()
        self.history = {}
        self.pending = {}  # prompt_id -> (ready_at, history entry): 'mock-slow' jobs
        self.files = {}
        self.inputs = {}  # uploaded input files (LoadImage)

    def log(self, entry):
        with self.lock, open(self.journal, 'a') as handle:
            handle.write(json.dumps(entry) + '\n')


def websocket_frame(opcode, data):
    size = len(data)
    header = bytes([0x80 | opcode, size if size < 126 else 126 if size < 65536 else 127])
    if size >= 126:
        header += struct.pack('>H' if size < 65536 else '>Q', size)
    return header + data


def stream_progress(state, prompt_id, client_id, graph):
    import time
    node = next((key for key, value in graph.items() if 'Sampler' in value.get('class_type', '')), next(iter(graph)))
    def send(kind, data):
        body = json.dumps({'type': kind, 'data': {'prompt_id': prompt_id, **data}}).encode()
        with state.lock:
            sock = state.sockets.get(client_id)
            if sock:
                try: sock.sendall(websocket_frame(1, body))
                except OSError: pass
    time.sleep(0.4)
    send('execution_start', {})
    send('executing', {'node': node})
    for step in range(1, 6):
        send('progress', {'node': node, 'value': step, 'max': 5})
        send('progress_state', {'nodes': {node: {'prompt_id': prompt_id, 'state': 'finished' if step == 5 else 'running', 'value': step, 'max': 5}}})
        meta = json.dumps({'prompt_id': prompt_id, 'node_id': node, 'image_type': 'image/png'}).encode()
        preview = struct.pack('>II', 4, len(meta)) + meta + png(64, 64, (step * 40, 90, 150))
        with state.lock:
            sock = state.sockets.get(client_id)
            if sock:
                try: sock.sendall(websocket_frame(2, preview))
                except OSError: pass
        time.sleep(1)
    send('execution_success', {})


def handler(state, object_info):
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"
        def log_message(self, *args):
            pass

        def reply(self, code, body, kind='application/json'):
            data = body if isinstance(body, bytes) else json.dumps(body).encode()
            self.send_response(code)
            self.send_header('Content-Type', kind)
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):
            url = urlparse(self.path)
            state.log({'method': 'GET', 'path': url.path})
            if url.path == '/ws' and state.live:
                import base64, hashlib
                client_id = parse_qs(url.query).get('clientId', [''])[0]
                accept = base64.b64encode(hashlib.sha1((self.headers.get('Sec-WebSocket-Key', '') + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
                self.send_response(101)
                self.send_header('Upgrade', 'websocket')
                self.send_header('Connection', 'Upgrade')
                self.send_header('Sec-WebSocket-Accept', accept)
                self.end_headers()
                with state.lock: state.sockets[client_id] = self.connection
                try:
                    while self.connection.recv(4096): pass
                except OSError: pass
                finally:
                    with state.lock:
                        if state.sockets.get(client_id) is self.connection: state.sockets.pop(client_id, None)
                return
            if url.path == '/object_info':
                info = json.loads(json.dumps(object_info))
                with state.lock:
                    uploaded = sorted(state.inputs)
                load = info.get('LoadImage', {}).get('input', {}).get('required', {}).get('image')
                if load and isinstance(load[0], list):
                    load[0] = load[0] + uploaded
                latents = info.get('LoadLatent', {}).get('input', {}).get('required', {}).get('latent')
                if latents and isinstance(latents[0], list):
                    latents[0] = latents[0] + [name for name in uploaded if name.endswith('.latent')]
                return self.reply(200, info)
            if url.path == '/system_stats':
                return self.reply(200, {'system': {'comfyui_version': 'mock-0', 'python_version': 'mock', 'os': 'posix'},
                                        'devices': [{'name': 'mock-cpu', 'type': 'cpu', 'vram_total': 0, 'vram_free': 0}]})
            if url.path in ('/queue', '/history') or url.path.startswith('/history/'):
                import time
                with state.lock:
                    for pid, (ready, entry) in list(state.pending.items()):
                        if time.time() >= ready:
                            state.history[pid] = entry
                            del state.pending[pid]
            if url.path == '/queue':
                with state.lock:
                    pending = [[i, pid, entry['prompt'][2], entry['prompt'][3], entry['prompt'][4]] for i, (pid, (_, entry)) in enumerate(state.pending.items())]
                return self.reply(200, {'queue_running': [], 'queue_pending': pending})
            if url.path.startswith('/history'):
                prompt = url.path[len('/history/'):] if url.path.startswith('/history/') else None
                with state.lock:
                    data = {prompt: state.history[prompt]} if prompt in state.history else ({} if prompt else dict(state.history))
                return self.reply(200, data)
            if url.path == '/view':
                name = parse_qs(url.query).get('filename', [''])[0]
                with state.lock:
                    body = state.files.get(name)
                return self.reply(200, body, 'image/png') if body else self.reply(404, {'error': 'not found'})
            return self.reply(404, {'error': 'mock: unknown endpoint ' + url.path})

        def do_POST(self):
            url = urlparse(self.path)
            length = int(self.headers.get('Content-Length') or 0)
            body = self.rfile.read(length) if length else b''
            if url.path == '/upload/image':
                # Minimal multipart parsing: field "image" with a filename.
                boundary = self.headers.get('Content-Type', '').split('boundary=')[-1].encode()
                name, data = None, None
                for part in body.split(b'--' + boundary):
                    head, _, content = part.partition(b'\r\n\r\n')
                    if b'name="image"' in head and b'filename="' in head:
                        name = head.split(b'filename="')[1].split(b'"')[0].decode()
                        data = content[:-2] if content.endswith(b'\r\n') else content
                state.log({'method': 'POST', 'path': url.path, 'file': name, 'bytes': len(data or b'')})
                if not name or data is None or '/' in name or '..' in name:
                    return self.reply(400, {'error': 'mock: bad upload'})
                with state.lock:
                    state.inputs[name] = data
                return self.reply(200, {'name': name, 'subfolder': '', 'type': 'input'})
            try:
                payload = json.loads(body or b'{}')
            except ValueError:
                payload = None
            state.log({'method': 'POST', 'path': url.path, 'body': payload})
            if url.path == '/prompt':
                if not state.accept:
                    return self.reply(403, {'error': 'mock-comfy: submission refused (smoke without GPU)'})
                prompt_id = str(uuid.uuid4())
                outputs = {}
                import hashlib
                digest = hashlib.sha256(json.dumps((payload or {}).get('prompt', {}), sort_keys=True).encode()).digest()
                for node_id, node in (payload or {}).get('prompt', {}).items():
                    if node.get('class_type') == 'LoadImage':
                        with state.lock:
                            if node.get('inputs', {}).get('image') not in state.inputs:
                                return self.reply(400, {'error': 'mock: LoadImage input not uploaded', 'node_errors': {node_id: 'missing'}})
                    if node.get('class_type') == 'LoadLatent':
                        with state.lock:
                            if node.get('inputs', {}).get('latent') not in state.inputs:
                                return self.reply(400, {'error': 'mock: LoadLatent input not uploaded', 'node_errors': {node_id: 'missing'}})
                    if node.get('class_type') == 'SaveLatent':
                        name = f'mock_{prompt_id[:8]}_{node_id}.latent'
                        with state.lock:
                            state.files[name] = latent_bytes(digest)
                        outputs[node_id] = {'latents': [{'filename': name, 'subfolder': '', 'type': 'output'}]}
                    if node.get('class_type') in ('SaveImage', 'DaSiWa_MetadataImageSaver'):
                        name = f'mock_{prompt_id[:8]}_{node_id}.png'
                        with state.lock:
                            state.files[name] = png(8, 8, (digest[0], digest[1], digest[2]))
                        outputs[node_id] = {'images': [{'filename': name, 'subfolder': '', 'type': 'output'}]}
                prefixes = [n.get('inputs', {}).get('filename_prefix', '') for n in (payload or {}).get('prompt', {}).values() if n.get('class_type') in ('SaveImage', 'SaveLatent', 'DaSiWa_MetadataImageSaver')]
                failed = any(str(p).startswith('mock-fail') for p in prefixes)
                entry = {'prompt': [0, prompt_id, payload.get('prompt'), {**payload.get('extra_data', {}), 'client_id': payload.get('client_id')}, list(outputs)],
                         'outputs': {} if failed else outputs,
                         'status': {'status_str': 'error' if failed else 'success', 'completed': not failed,
                                    'messages': [['execution_error', {'exception_message': 'mock failure'}]] if failed else []}}
                import time
                with state.lock:
                    slow = [str(p) for p in prefixes if str(p).startswith('mock-slow')]
                    if state.live:
                        state.pending[prompt_id] = (time.time() + 6, entry)
                    elif slow:
                        # 'mock-slow' = 3 s ; 'mock-slow-90' = 90 s in the queue.
                        tail = slow[0][len('mock-slow-'):]
                        state.pending[prompt_id] = (time.time() + (float(tail) if tail.isdigit() else 3.0), entry)
                    else:
                        state.history[prompt_id] = entry
                if state.live:
                    threading.Thread(target=stream_progress, args=(state, prompt_id, payload.get('client_id'), payload.get('prompt', {})), daemon=True).start()
                if state.drop_response:
                    self.close_connection = True
                    return
                return self.reply(200, {'prompt_id': prompt_id, 'number': 0, 'node_errors': {}})
            if url.path == '/queue' and isinstance(payload, dict) and 'delete' in payload:
                with state.lock:
                    for pid in payload.get('delete') or []:
                        state.pending.pop(pid, None)
                return self.reply(200, {})
            if url.path in ('/interrupt', '/free'):
                return self.reply(200, {})
            return self.reply(404, {'error': 'mock: unknown endpoint ' + url.path})
    return Handler


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--object-info', type=Path, default=HERE / 'fixtures/object_info-subset.json')
    parser.add_argument('--live', action='store_true', help='Emit identified fake WebSocket progress and previews')
    parser.add_argument('--port', type=int, default=0)
    parser.add_argument('--journal', type=Path, required=True)
    parser.add_argument('--accept-prompts', action='store_true')
    parser.add_argument('--drop-prompt-response', action='store_true', help='Accept the prompt but close before acknowledgement')
    parser.add_argument('--ready-file', type=Path, help='Write the bound port here once listening')
    args = parser.parse_args()
    object_info = json.loads(args.object_info.read_text())
    state = State(args.journal, args.accept_prompts, args.drop_prompt_response, args.live)
    server = ThreadingHTTPServer(('127.0.0.1', args.port), handler(state, object_info))
    if args.ready_file:
        args.ready_file.write_text(str(server.server_address[1]))
    server.serve_forever()


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""ComfyUI offline smoke: closed engine, short note, bounded /queue polling.

Runs v12-native-smoke.py with a counting stub bound on an isolated loopback
port (SAYA_STUDIO_SMOKE_COMFY_PORT). The stub answers every HTTP request with
a plain 500 — one unambiguous failure per poll cycle, no client retry
ambiguity — and drops WebSocket upgrades without a response (true refusal
path for the monitor's reconnect). After the run it asserts the offline
backoff cadence: the fixed 5 s polling this replaces would send ~window/5
/queue requests; the backoff (4, 8, 16, 32, 60 s) must stay far below.

No GPU, no ComfyUI start, no real user database: the harness isolates
XDG_DATA_HOME in its run directory.

Example: python3 scripts/comfy-offline-native-smoke.py --binary /tmp/private-target/debug/saya-studio
"""
import argparse
import json
import os
import socket
import subprocess
import sys
import threading
import time
from pathlib import Path

PROJECT = Path(__file__).resolve().parent.parent


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--root', type=Path, default=PROJECT / 'test-results/comfy-offline-native')
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)

    listener = socket.socket()
    listener.bind(('127.0.0.1', 0))
    listener.listen(16)
    listener.settimeout(0.2)
    port = listener.getsockname()[1]
    counts = {'queue': 0, 'ws': 0, 'other': 0}
    stop = threading.Event()

    def serve() -> None:
        while not stop.is_set():
            try:
                conn, _ = listener.accept()
            except (socket.timeout, OSError):
                continue
            with conn:
                conn.settimeout(2)
                try:
                    data = b''
                    while b'\r\n\r\n' not in data:
                        chunk = conn.recv(4096)
                        if not chunk:
                            break
                        data += chunk
                    first = data.split(b'\r\n', 1)[0].decode('latin1', 'replace')
                    if first.startswith('GET /ws'):
                        counts['ws'] += 1
                        # No handshake response: the monitor sees a dead peer.
                        continue
                    if first.startswith('GET /queue'):
                        counts['queue'] += 1
                    else:
                        counts['other'] += 1
                    conn.sendall(b'HTTP/1.1 500 Internal Server Error\r\n'
                                 b'Content-Length: 0\r\nConnection: close\r\n\r\n')
                except OSError:
                    pass

    server = threading.Thread(target=serve, daemon=True)
    server.start()

    env = {**os.environ,
           'SAYA_STUDIO_SMOKE_COMFY_PORT': str(port),
           'PYTHONHASHSEED': '0'}
    started = time.monotonic()
    completed = subprocess.run(
        [sys.executable, str(PROJECT / 'scripts/v12-native-smoke.py'),
         '--binary', str(binary),
         '--script', str(PROJECT / 'scripts/comfy-offline-smoke.js'),
         '--root', str(args.root),
         '--timeout', '150'],
        cwd=PROJECT, env=env, capture_output=True, text=True)
    elapsed = time.monotonic() - started
    stop.set()
    listener.close()

    payload = completed.stdout.strip()
    try:
        report = json.loads(payload[payload.index('{'):])
    except (ValueError, IndexError):
        report = {'ok': False, 'failure': f'harness produced no report: {completed.stderr[-1500:]}'}
    # Bound for the offline cadence: the replaced fixed 5 s poll would have
    # sent ~elapsed/5 requests; the backoff must stay far under that.
    bound = max(5, int(elapsed / 7) + 2)
    report['offlineWindowSeconds'] = round(elapsed, 1)
    report['offlineStubRequests'] = counts
    report['offlineQueueBound'] = bound
    report['offlineFixedCadenceBaseline'] = round(elapsed / 5, 1)
    report['stubPort'] = port
    if report.get('ok'):
        assert counts['queue'] >= 2, f'offline polling stopped retrying: {counts}'
        assert counts['queue'] <= bound, (
            f'queue hammered while offline: {counts["queue"]} /queue requests over {elapsed:.0f}s '
            f'(bound {bound}, fixed 5 s cadence ~{elapsed / 5:.0f})')
        report['offlineQueueBounded'] = True
    print(json.dumps(report, indent=2))
    return 0 if report.get('ok') else 1


if __name__ == '__main__':
    sys.exit(main())

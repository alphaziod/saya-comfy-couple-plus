#!/usr/bin/env python3
"""Removes from the Saya Studio catalogue the images written by the fake test engines.

Proof required for each file (never the size alone):
  * PNG whose bytes are EXACTLY those produced by the
    scripts/mock-comfy.py (`png(w, h, rgb)`, solid colour) encoder;
  * file in Saya's storage (results/, casting/, capture root);
  * no provenance or character row cites the asset.
Without --apply: report only. With --apply: JSON journal of the deleted
rows, then deletion of the rows and files.

Usage: purge-mock-artifacts.py <studio.sqlite3> [--capture-root DIR] [--apply]
"""
import argparse, json, sqlite3, struct, time, zlib
from pathlib import Path

REFERENCES = ('job_outputs', 'asset_characters', 'character_references', 'ab_cell_images', 'asset_models', 'pass_artifacts')


def mock_png(width, height, rgb):  # exact copy of scripts/mock-comfy.py
    def chunk(kind, body):
        return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body) & 0xffffffff)
    raw = b''.join(b'\0' + bytes(rgb) * width for _ in range(height))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b''))


def is_mock(data):
    if len(data) > 4096 or not data.startswith(b'\x89PNG\r\n\x1a\n') or data[12:16] != b'IHDR':
        return False
    width, height = struct.unpack('>II', data[16:24])
    try:
        at = data.index(b'IDAT')
        size = struct.unpack('>I', data[at - 4:at])[0]
        raw = zlib.decompress(data[at + 4:at + 4 + size])
    except (ValueError, zlib.error):
        return False
    return mock_png(width, height, raw[1:4]) == data


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('db', type=Path)
    parser.add_argument('--capture-root', type=Path)
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    data_dir = args.db.resolve().parent
    owned = [p.resolve() for p in (data_dir / 'results', data_dir / 'casting', args.capture_root) if p and p.exists()]
    db = sqlite3.connect(args.db)
    db.execute('PRAGMA foreign_keys=ON')
    db.row_factory = sqlite3.Row
    found, skipped = [], []
    for row in db.execute("SELECT * FROM assets WHERE availability='present' AND byte_size<=4096"):
        path = Path(row['absolute_path'])
        try:
            real = path.resolve(strict=True)
            data = real.read_bytes()
        except OSError:
            continue
        if not is_mock(data):
            continue
        if not any(real.is_relative_to(root) for root in owned):
            skipped.append({'path': str(path), 'reason': 'hors stockage Saya'})
            continue
        cited = [t for t in REFERENCES if db.execute(f'SELECT 1 FROM {t} WHERE asset_id=?', (row['id'],)).fetchone()]
        if cited:
            skipped.append({'path': str(path), 'reason': 'cité par ' + ','.join(cited)})
            continue
        found.append(dict(row))
    report = {'proven': len(found), 'skipped': skipped, 'applied': args.apply}
    if args.apply and found:
        journal = data_dir / 'pass-states' / 'mock-artifacts-cleanup'
        journal.mkdir(parents=True, exist_ok=True)
        log = journal / f'removed-{time.strftime("%Y%m%d-%H%M%S")}.json'
        log.write_text(json.dumps(found, ensure_ascii=False, indent=1))
        with db:
            for row in found:
                db.execute('DELETE FROM assets WHERE id=?', (row['id'],))
        for row in found:
            Path(row['absolute_path']).unlink()
        report['journal'] = str(log)
    report['paths'] = [r['absolute_path'] for r in found]
    print(json.dumps(report, ensure_ascii=False, indent=1))


if __name__ == '__main__':
    main()

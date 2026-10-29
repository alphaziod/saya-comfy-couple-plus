"""Synthetic V11 gallery fixture: images, sidecar catalogue and isolated preferences.

Creates 130 tiny PNGs assigned to three characters through the sidecar
catalogue path (characters_ready.json + journal.jsonl), then prepares an
isolated XDG data directory whose database already points at the images.
Indexing itself is performed by Studio; nothing here writes the user database.
"""
import argparse, json, pathlib, sqlite3, struct, zlib

parser = argparse.ArgumentParser()
parser.add_argument('--root', default='test-results/v11-gallery')
parser.add_argument('--extra', type=int, default=0)
args = parser.parse_args()
root = pathlib.Path(args.root).resolve()
data = root / 'data/studio.saya.desktop'
images = root / 'images'
data.mkdir(parents=True, exist_ok=True)
images.mkdir(exist_ok=True)

def chunk(kind, body):
    return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body) & 0xffffffff)

def png(path, tint):
    raw = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 16, 16, 8, 2, 0, 0, 0))
    graph = {'1': {'class_type': 'CheckpointLoaderSimple', 'inputs': {'ckpt_name': 'model'}}}
    raw += chunk(b'tEXt', b'prompt\0' + json.dumps(graph).encode())
    raw += chunk(b'IDAT', zlib.compress((b'\0' + bytes(tint) * 16) * 16)) + chunk(b'IEND', b'')
    path.write_bytes(raw)

CHARACTERS = [
    {'id': 'alpha', 'name': 'Alpha Test', 'identity_prompt': 'alpha full P1 identity', 'anatomy_prompt': 'alpha full P1 anatomy'},
    {'id': 'beta', 'name': 'Beta Test', 'identity_prompt': 'beta full P1 identity'},
    {'id': 'gamma', 'name': 'Gamma Test', 'identity_prompt': 'gamma full P1 identity'},
]
PLAN = [('alpha', 60, (150, 90, 90)), ('beta', 50, (90, 150, 90)), ('gamma', 20, (90, 90, 150))]

journal = []
for key, count, tint in PLAN:
    for i in range(count):
        path = images / f'{key}{i:03d}.png'
        if not path.exists():
            png(path, tint)
        journal.append({'event': 'saved', 'image': str(path), 'id': key})
for i in range(max(0, args.extra)):
    path = images / f'extra{i:05d}.png'
    if not path.exists():
        png(path, (40 + i % 160, 80 + (i * 3) % 160, 120 + (i * 7) % 120))
    journal.append({'event': 'saved', 'image': str(path), 'id': None})

(images / 'characters_ready.json').write_text(json.dumps({'characters': CHARACTERS}, indent=1))
with open(images / 'journal.jsonl', 'w') as handle:
    for line in journal:
        handle.write(json.dumps(line) + '\n')

db_path = data / 'studio.sqlite3'
if not db_path.exists():
    with sqlite3.connect(db_path) as db:
        version = db.execute('pragma user_version').fetchone()[0]
        for migration in sorted(pathlib.Path('migrations').glob('*.sql')):
            # Genuine v9 database: later migrations are applied by the app
            # itself on open (real 9 -> current upgrade under test).
            if version < int(migration.name[:4]) <= 9:
                db.executescript(migration.read_text())
        db.execute('pragma user_version=9')
        db.execute('INSERT OR IGNORE INTO preferences VALUES(?,?)', ('libraryRoot', json.dumps(str(images))))
        db.execute("INSERT INTO preferences VALUES('selectedTab','\"gallery\"') ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json")
print(json.dumps({'images': len(journal), 'characters': len(CHARACTERS), 'database': str(db_path), 'imagesDir': str(images)}))

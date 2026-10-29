"""Synthetic schemas from installed Comfy/Saya nodes; no private image or prompt."""
import json
import struct
import zlib

PROMPT = '  exact, (tag:1.2)\nété 🌸\n'

def write_fixture(path):
    graph = {
        'p1': {'class_type': 'PrimitiveStringMultiline', 'inputs': {'value': PROMPT}, '_meta': {'title': 'P1'}},
        'p2': {'class_type': 'PrimitiveStringMultiline', 'inputs': {'value': 'inactive'}, '_meta': {'title': 'P2'}},
        'c1': {'class_type': 'CLIPTextEncode', 'inputs': {'text': ['p1', 0]}, '_meta': {'title': 'P1 encoder'}},
        'c2': {'class_type': 'CLIPTextEncode', 'inputs': {'text': ['p2', 0]}, '_meta': {'title': 'P2 encoder'}},
        'neg': {'class_type': 'CLIPTextEncode', 'inputs': {'text': ''}, '_meta': {'title': 'NEG'}},
        'couple': {'class_type': 'SayaMultiCouple', 'inputs': {'solo': True, 'pos_1': ['c1', 0], 'pos_2': ['c2', 0], 'neg_1': ['neg', 0]}},
        'sample': {'class_type': 'KSampler', 'inputs': {'positive': ['couple', 2], 'seed': 18446744073709551615, 'steps': 27, 'cfg': 5.5}},
        'save': {'class_type': 'SaveImage', 'inputs': {'images': ['sample', 0]}},
    }
    def chunk(kind, body):
        return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body) & 0xffffffff)
    raw = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 2, 8, 2, 0, 0, 0))
    raw += chunk(b'iTXt', b'prompt\0\0\0\0\0' + json.dumps(graph, ensure_ascii=False).encode())
    raw += chunk(b'IDAT', zlib.compress(b'\0' * 14)) + chunk(b'IEND', b'')
    path.write_bytes(raw)

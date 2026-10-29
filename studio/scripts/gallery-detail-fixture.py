"""Synthetic Saya-like solo pass-1 PNG for the gallery detail panel smoke.

Prompt sources titled like the real Saya workflow (Prompt · Main / Background
with prefix, Prompt · Action, Prompt · Person 1, Prompt · Person 1 · Anatomy,
Prompt · Negative) and their CLIP encoders; the negative is the embedding
placeholder that the panel must hide. No private image or prompt.
"""
import json
import struct
import zlib

PREFIXE = 'masterpiece, best quality, very aesthetic, absurdres'
BG = 'candlelit library at night, brass lamps, rain on the tall windows'
ACTION = 'standing between the shelves, light smile, closed mouth'
P1 = 'young adult woman, (aged up:1.3), pink loose waves'
P1_ANATOMY = '(small wings:1.25), cute_tail, long tail'
NEG = 'embedding:lazyneg'


def write_fixture(path):
    graph = {
        'main': {'class_type': 'SayaMainPromptFR', 'inputs': {'prefixe': PREFIXE, 'decor_libre': BG}, '_meta': {'title': 'Prompt · Main / Background'}},
        'c_main': {'class_type': 'CLIPTextEncode', 'inputs': {'text': ['main', 0]}, '_meta': {'title': 'CLIP · Main / Background'}},
        'act': {'class_type': 'PrimitiveStringMultiline', 'inputs': {'value': ACTION}, '_meta': {'title': 'Prompt · Action'}},
        'c_act': {'class_type': 'CLIPTextEncode', 'inputs': {'text': ['act', 0]}, '_meta': {'title': 'CLIP · Action'}},
        'p1': {'class_type': 'PrimitiveStringMultiline', 'inputs': {'value': P1}, '_meta': {'title': 'Prompt · Person 1'}},
        'c_p1': {'class_type': 'CLIPTextEncode', 'inputs': {'text': ['p1', 0]}, '_meta': {'title': 'CLIP · Person 1'}},
        'p1a': {'class_type': 'PrimitiveStringMultiline', 'inputs': {'value': P1_ANATOMY}, '_meta': {'title': 'Prompt · Person 1 · Anatomy'}},
        'c_p1a': {'class_type': 'CLIPTextEncode', 'inputs': {'text': ['p1a', 0]}, '_meta': {'title': 'CLIP · Person 1 · Anatomy'}},
        'neg': {'class_type': 'PrimitiveStringMultiline', 'inputs': {'value': NEG}, '_meta': {'title': 'Prompt · Negative'}},
        'c_neg': {'class_type': 'CLIPTextEncode', 'inputs': {'text': ['neg', 0]}, '_meta': {'title': 'CLIP · Negative'}},
        'couple': {'class_type': 'SayaMultiCouple', 'inputs': {'solo': True, 'pos_1': ['c_p1', 0], 'neg_1': ['c_neg', 0], 'decor_1': ['c_main', 0], 'action_1': ['c_act', 0], 'anatomy_1': ['c_p1a', 0]}, '_meta': {'title': 'Saya Couple'}},
        'sample': {'class_type': 'KSampler', 'inputs': {'positive': ['couple', 2], 'seed': 18446744073709551615, 'steps': 27, 'cfg': 5.5}},
        'save': {'class_type': 'SaveImage', 'inputs': {'images': ['sample', 0]}},
    }

    def chunk(kind, body):
        return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body) & 0xffffffff)

    raw = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 2, 8, 2, 0, 0, 0))
    raw += chunk(b'iTXt', b'prompt\0\0\0\0\0' + json.dumps(graph, ensure_ascii=False).encode())
    raw += chunk(b'IDAT', zlib.compress(b'\0' * 14)) + chunk(b'IEND', b'')
    path.write_bytes(raw)


if __name__ == '__main__':
    import sys
    from pathlib import Path
    write_fixture(Path(sys.argv[1]) if len(sys.argv) > 1 else Path('metadata.png'))

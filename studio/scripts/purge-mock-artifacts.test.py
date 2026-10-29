"""The purge of test artifacts requires the fake engine's encoding proof:
an ordinary 8x8 image (same solid colour) is never retained."""
import importlib.util, struct, unittest, zlib
from pathlib import Path

spec = importlib.util.spec_from_file_location('purge', Path(__file__).with_name('purge-mock-artifacts.py'))
purge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(purge)


def png(width, height, rgb, level=9, text=b''):
    def chunk(kind, body):
        return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body) & 0xffffffff)
    raw = b''.join(b'\0' + bytes(rgb) * width for _ in range(height))
    extra = chunk(b'tEXt', text) if text else b''
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)) + extra
            + chunk(b'IDAT', zlib.compress(raw, level)) + chunk(b'IEND', b''))


class ProofTests(unittest.TestCase):
    def test_mock_engine_output_is_proven(self):
        self.assertTrue(purge.is_mock(purge.mock_png(8, 8, (12, 34, 56))))
        self.assertTrue(purge.is_mock(purge.mock_png(64, 64, (80, 90, 150))))

    def test_ordinary_8x8_images_are_never_taken(self):
        self.assertFalse(purge.is_mock(png(8, 8, (12, 34, 56), level=1)))
        self.assertFalse(purge.is_mock(png(8, 8, (12, 34, 56), text=b'parameters\0seed: 7')))
        self.assertFalse(purge.is_mock(b'not a png'))


if __name__ == '__main__':
    unittest.main()

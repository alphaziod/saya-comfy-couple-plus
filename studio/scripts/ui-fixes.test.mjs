import test from 'node:test';
import assert from 'node:assert/strict';
import { extensionFor, extensionMismatch, parseLoras } from '../src/lib/ui-helpers.ts';

test('extension de destination suit le format', () => {
  assert.equal(extensionFor('/a/b - retouche.png', 'jpeg'), '/a/b - retouche.jpg');
  assert.equal(extensionFor('/a/b.JPG', 'png'), '/a/b.png');
  assert.equal(extensionFor('/a/b.jpeg', 'jpeg'), '/a/b.jpeg');
  assert.equal(extensionFor('/a/b.png', 'png'), '/a/b.png');
  assert.equal(extensionFor('/a/b.webp', 'jpeg'), '/a/b.webp');
  assert.equal(extensionFor('/a.png.d/b', 'jpeg'), '/a.png.d/b');
  assert.equal(extensionMismatch('/a/b.png', 'jpeg'), true);
  assert.equal(extensionMismatch('/a/b.jpg', 'png'), true);
  assert.equal(extensionMismatch('/a/b.jpg', 'jpeg'), false);
  assert.equal(extensionMismatch('/a/b.png', 'png'), false);
});

test('LoRA nom:poids vers la forme Rust', () => {
  assert.deepEqual(parseLoras(''), []);
  assert.deepEqual(parseLoras('detail_tweaker:0.6\r\n\n dossier/x.safetensors : 1 '), [
    { name: 'detail_tweaker', weightModel: 0.6, weightClip: 0.6 },
    { name: 'dossier/x.safetensors', weightModel: 1, weightClip: 1 }
  ]);
  assert.throws(() => parseLoras('sans_poids'), /ligne 1/);
  assert.throws(() => parseLoras('ok:1\nx:abc'), /ligne 2/);
  assert.throws(() => parseLoras(':0.5'), /ligne 1/);
});

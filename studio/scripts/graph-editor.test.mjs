import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { missingRequiredInputs, parseGraphDocument, typesCompatible } from '../src/lib/graph/editor.ts';

async function fixture() {
  return JSON.parse(
    await readFile(new URL('../contracts/graph-editor-v1.json', import.meta.url), 'utf8'),
  );
}

test('Format v1 du noyau Rust accepté par la frontière TypeScript', async () => {
  const document = parseGraphDocument(await fixture());
  assert.equal(document.formatVersion, 1);
  assert.equal(document.nodes.length, 7);
  assert.equal(document.connections.length, 9);
  assert.equal(document.groups.length, 1);
  assert.equal(document.subgraphs.length, 1);
  const ksampler = document.nodes.find(n => n.classType === 'KSampler');
  assert.equal(ksampler.params.seed, 1234567890);
  const samplerName = ksampler.inputs.find(p => p.name === 'sampler_name');
  assert.equal(samplerName.type, 'COMBO');
  const checkpoint = document.nodes.find(n => n.classType === 'CheckpointLoaderSimple');
  assert.deepEqual(checkpoint.outputs.map(p => p.type), ['MODEL', 'CLIP', 'VAE']);
  assert.deepEqual(missingRequiredInputs(document), []);
  assert.equal(typesCompatible('MODEL', 'MODEL'), true);
  assert.equal(typesCompatible('MODEL,CLIP', 'CLIP'), true);
  assert.equal(typesCompatible('*', 'LATENT'), true);
  assert.equal(typesCompatible('CLIP', 'MODEL'), false);
});

test('Documents non conformes refusés par la frontière TypeScript', async () => {
  const base = await fixture();
  assert.throws(() => parseGraphDocument({ ...base, formatVersion: 2 }));
  assert.throws(() => parseGraphDocument({ ...base, futureField: true }));
  assert.throws(() => parseGraphDocument({ ...base, nodes: [...base.nodes, base.nodes[0]] }));
  // Connexion pendante : node cible absent.
  const dangling = structuredClone(base);
  dangling.connections[0].to.node = 'node-absent';
  assert.throws(() => parseGraphDocument(dangling));
  // Auto-connexion.
  const selfConnected = structuredClone(base);
  selfConnected.connections[0].from.node = selfConnected.connections[0].to.node;
  assert.throws(() => parseGraphDocument(selfConnected));
  // Incompatible types: MODEL to a CLIP input.
  const mismatch = structuredClone(base);
  mismatch.connections[0].from.output = 0;
  assert.throws(() => parseGraphDocument(mismatch));
  // Duplicate on the same target input.
  const duplicate = structuredClone(base);
  duplicate.connections.push({ ...duplicate.connections[0], id: 'copie' });
  assert.throws(() => parseGraphDocument(duplicate));
  // Parameter as an array (reserved link shape).
  const arrayParam = structuredClone(base);
  arrayParam.nodes[0].params.ckpt_name = ['model.safetensors'];
  assert.throws(() => parseGraphDocument(arrayParam));
});

test('Entrées requises manquantes signalées comme état intermédiaire', async () => {
  const base = await fixture();
  const broken = structuredClone(base);
  broken.connections = broken.connections.filter(
    c => !(c.to.input === 'model' && broken.nodes.some(n => n.id === c.to.node && n.classType === 'KSampler')),
  );
  const document = parseGraphDocument(broken);
  const missing = missingRequiredInputs(document);
  assert.deepEqual(missing, [['2f97c049-5ce5-4b5a-9631-11e5b6a4b1c9', 'model']]);
});

test('Les seeds u64 au-delà de 2^53 ne traversent jamais JSON.parse', async () => {
  // Boundary rule: document editing stays in the Rust core.
  // JavaScript loses precision beyond Number.MAX_SAFE_INTEGER; displayed
  // documents must never be re-serialised from JS with
  // such values.
  const raw = JSON.parse('{"seed":18446744073709551615}');
  assert.equal(raw.seed, 18446744073709552000);
  assert.ok(!Number.isSafeInteger(raw.seed));
  assert.notEqual(raw.seed, 18446744073709551615n);
});

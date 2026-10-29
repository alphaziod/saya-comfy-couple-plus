import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseModuleList, parseModuleDetail, parseModuleDiff } from '../src/lib/module-contracts.ts';

test('Rust module fixtures are accepted by the TypeScript boundary', async () => {
  const detail = JSON.parse(await readFile(new URL('../contracts/module-detail-v1.json', import.meta.url), 'utf8'));
  const parsed = parseModuleDetail(detail);
  assert.equal(parsed.manifest.manifestVersion, 1);
  assert.equal(parsed.manifest.inputs.length, 4);
  assert.equal(parsed.manifest.outputs[0].kind, 'IMAGE');
  assert.equal(parsed.manifest.dependencies.loras[0], 'saya-fix');
  assert.equal(parsed.nodeCount, parsed.nodes.length);
  const list = parseModuleList([{
    id: detail.id, name: detail.manifest.name, description: detail.manifest.description,
    currentVersion: 2, versionCount: 2,
    inputs: detail.manifest.inputs, outputs: detail.manifest.outputs, dependencies: detail.manifest.dependencies,
  }]);
  assert.equal(list[0].currentVersion, 2);
  const diff = parseModuleDiff({
    module: detail.id, a: 1, b: 2,
    nodes: { added: [], removed: [{ id: '7', classType: 'VAEDecode' }], changed: [] },
    ports: { inputs: { added: [], removed: [], changed: [] }, outputs: { added: [], removed: [], changed: [] } },
    parameters: { added: [], removed: [], changed: [] },
  });
  assert.equal(diff.nodes.removed.length, 1);
});

test('Non-conforming module responses are refused, never improvised', async () => {
  const detail = JSON.parse(await readFile(new URL('../contracts/module-detail-v1.json', import.meta.url), 'utf8'));
  assert.throws(() => parseModuleDetail({ ...detail, manifest: { ...detail.manifest, manifestVersion: 2 } }));
  assert.throws(() => parseModuleDetail({ ...detail, manifest: { ...detail.manifest, name: 42 } }));
  assert.throws(() => parseModuleDetail({ ...detail, manifest: { ...detail.manifest, inputs: [{ name: 7, kind: 'IMAGE', node: '7' }] } }));
  assert.throws(() => parseModuleDetail({ ...detail, versions: 'deux' }));
  assert.throws(() => parseModuleDetail({ ...detail, warnings: [null] }));
  assert.throws(() => parseModuleList('deux'));
  assert.throws(() => parseModuleList([{ id: 'x' }]));
  assert.throws(() => parseModuleDiff({ module: 'x', a: 'un', b: 2 }));
  assert.throws(() => parseModuleDetail(null));
});

import { kindsCompatible, composeModules, buildConnection, instanceId } from '../src/lib/module-contracts.ts';

test('composition contract lists typed links and unsatisfied inputs', () => {
  assert.ok(kindsCompatible('IMAGE', 'IMAGE') && kindsCompatible('*', 'LATENT') && kindsCompatible('IMAGE,MASK', 'MASK'));
  assert.ok(!kindsCompatible('LATENT', 'IMAGE'), 'an image is never a latent');
  const sampler = { outputs: [{ name: '7.0', kind: 'IMAGE', node: '7', outputIndex: 0 }] };
  const upscale = { inputs: [{ name: '2.image', kind: 'IMAGE', node: '2', input: 'image' }, { name: '1.model', kind: 'UPSCALE_MODEL', node: '1', input: 'model' }] };
  const result = composeModules(sampler, upscale);
  assert.deepEqual(result.links, [{ from: '7.0', to: '2.image', kind: 'IMAGE' }]);
  assert.deepEqual(result.unsatisfied.map(p => p.name), ['1.model']);
  assert.equal(result.composable, true);
  assert.equal(composeModules({ outputs: [{ name: 'x', kind: 'LATENT', node: '1' }] }, { inputs: upscale.inputs }).composable, false);
});

test('connection builder requires every input and sends overrides as an array', () => {
  const built = buildConnection(['5.model', '6.latent'], { '5.model': { port: '5.model', node: '1', outputIndex: 0 } }, [{ port: '7.0', node: '9', input: 'images' }]);
  assert.deepEqual(built.missing, ['6.latent']);
  assert.deepEqual(built.connection.inputs, [{ port: '5.model', node: '1', outputIndex: 0 }]);
  assert.deepEqual(built.connection.overrides, []);
  assert.equal(instanceId('Détail Visage #2'), 'detail-visage-2');
  assert.equal(instanceId('!!!'), 'module');
});

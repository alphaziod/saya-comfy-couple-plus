import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseBootstrap } from '../src/lib/contracts.ts';

test('Rust fixture is accepted by the TypeScript boundary', async () => {
  const fixture = JSON.parse(await readFile(new URL('../contracts/bootstrap-v1.json', import.meta.url), 'utf8'));
  // Parallel migrations raise the schema beyond 16: number never frozen.
  assert.ok(parseBootstrap(fixture).schemaVersion >= 16);
  assert.throws(() => parseBootstrap({ ...fixture, apiVersion: 2 }));
  assert.throws(() => parseBootstrap({ ...fixture, counts: { characters: -1, assets: 0, jobs: 0 } }));
  assert.throws(() => parseBootstrap({ ...fixture, implemented: [null] }));
  assert.throws(() => parseBootstrap(null));
});

test('semantic sidecar is declared as a bundled resource without hardcoded ComfyUI paths', async () => {
  const config = JSON.parse(await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
  assert.equal(config.bundle.resources['../sidecars/f09-semantic-embed.py'], 'sidecars/f09-semantic-embed.py');
  const script = await readFile(new URL('../sidecars/f09-semantic-embed.py', import.meta.url), 'utf8');
  assert.ok(!script.includes('/home/'));
  assert.ok(script.includes('--serve'));
  assert.ok(script.includes('--idle-seconds'));
});

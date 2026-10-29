import { readFile } from 'node:fs/promises';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildScene, canConnect, filterNodes, knownClasses, nodeKey, typesCompatible, nodesInRect, fitTransform, centerOn, NODE_W } from '../src/lib/graph/view.ts';

async function view() {
  const document = JSON.parse(await readFile(new URL('../contracts/graph-editor-v1.json', import.meta.url), 'utf8'));
  const originalIds = Object.fromEntries(document.nodes.map((n, i) => [n.id, String(100 + i)]));
  return { workflow: 'w', document, paramText: {}, originalIds, canUndo: false, canRedo: false, missing: [], exportable: true, exportError: null };
}

test('scène : un nœud, un lien et un cadre par entité du document', async () => {
  const v = await view();
  const s = buildScene(v);
  assert.equal(s.nodes.length, v.document.nodes.length);
  assert.equal(s.edges.length, v.document.connections.length);
  assert.equal(s.frames.filter(f => f.kind === 'group').length, v.document.groups.length);
  assert.ok(s.edges.every(e => e.d.startsWith('M')));
});

test('décalage de glisser local appliqué sans toucher au document', async () => {
  const v = await view();
  const id = v.document.nodes[0].id;
  const a = buildScene(v).nodes[0], b = buildScene(v, { [id]: { x: 10, y: -5 } }).nodes[0];
  assert.equal(b.x - a.x, 10); assert.equal(b.y - a.y, -5);
  assert.equal(b.outputs.length ? b.outputs[0].x : NODE_W + b.x, b.x + NODE_W);
});

test('compatibilité de ports et recherche', async () => {
  assert.ok(typesCompatible('*', 'MODEL')); assert.ok(typesCompatible('A,B', 'B')); assert.ok(!typesCompatible('A', 'B'));
  const v = await view();
  const k = v.document.nodes.find(n => n.classType === 'KSampler');
  assert.ok(filterNodes(v, 'ksampler').some(n => n.id === k.id));
  assert.ok(filterNodes(v, v.originalIds[k.id]).some(n => n.id === k.id));
  assert.equal(filterNodes(v, '  ').length, 0);
  assert.ok(knownClasses(v).includes('KSampler'));
  assert.equal(nodeKey(k.id, v.originalIds), v.originalIds[k.id]);
  assert.equal(nodeKey('zz', {}), 'zz');
});

test('canConnect refuse boucle, entrée déjà liée et type incompatible', async () => {
  const v = await view();
  const s = buildScene(v);
  const c = v.document.connections[0];
  assert.equal(canConnect(s, c.from, c.to), false); // input already linked
  assert.equal(canConnect(s, { node: c.from.node, output: 0 }, { node: c.from.node, input: 'x' }), false);
});

test('rectangle, ajustement et centrage', async () => {
  const s = buildScene(await view());
  assert.equal(nodesInRect(s, { x0: -1e6, y0: -1e6, x1: 1e6, y1: 1e6 }).length, s.nodes.length);
  const f = fitTransform(s, 800, 600);
  assert.ok(f.zoom > 0 && f.zoom <= 1);
  const c = centerOn({ x: 10, y: 20 }, 2, 100, 100);
  assert.equal(c.panX, 30); assert.equal(c.panY, 10);
});

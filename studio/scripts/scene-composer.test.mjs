import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  buildManifest,
  compositionForProfile,
  sceneText,
  validateManifest,
} from '../src/lib/scene-manifest.ts';

function coupleManifest() {
  return buildManifest({
    workflowId: 'wf-1',
    composition: 'couple',
    p1: { characterId: 'c1', variantId: 'v1' },
    p2: { characterId: 'c2', variantId: null },
    action: sceneText('preset', 'P1', ''),
    bg: sceneText('custom', null, 'night market,\n(wet cobblestones:0.9)'),
    quality: sceneText('workflow', null, ''),
    negative: sceneText('custom', null, 'lowres,  worst quality'),
    mask: { nodeId: 'mask', direction: 'vertical', split: 50, blur: 0, swap: false },
  });
}

test('composition follows only the saved mapping profile', () => {
  assert.equal(compositionForProfile('solo'), 'solo');
  assert.equal(compositionForProfile('couple'), 'couple');
  assert.equal(compositionForProfile('unconfigured'), null);
  assert.equal(compositionForProfile('couple-name'), null);
});

test('manifest assembly preserves exact bytes, weights and newlines', () => {
  const manifest = buildManifest({
    workflowId: 'wf-1',
    composition: 'solo',
    p1: { characterId: 'c1', variantId: null },
    p2: null,
    action: sceneText('custom', null, 'holds (a rose:1.1)\n  gently,  warm'),
    bg: sceneText('workflow', null, ''),
    quality: null,
    negative: null,
    mask: null,
  });
  assert.equal(manifest.action.text, 'holds (a rose:1.1)\n  gently,  warm');
  assert.equal(manifest.bg.source, 'workflow');
  // JSON round-trip keeps every byte of the exact prompt texts.
  const round = JSON.parse(JSON.stringify(manifest));
  assert.equal(round.action.text, manifest.action.text);
  assert.deepEqual(validateManifest(round), []);
  // Optional sections and P2 stay absent for SOLO.
  assert.equal('p2' in round, false);
  assert.equal('quality' in round, false);
  assert.equal('mask' in round, false);
});

test('SOLO refuses P2 and COUPLE requires it, matching the runner contract', () => {
  const couple = coupleManifest();
  assert.deepEqual(validateManifest(couple), []);
  const withoutP2 = buildManifest({ ...couple, p2: null, composition: 'couple' });
  assert.ok(validateManifest(withoutP2).some(p => p.includes('COUPLE')));
  const soloWithP2 = buildManifest({ ...couple, composition: 'solo' });
  assert.ok(validateManifest(soloWithP2).some(p => p.includes('SOLO') && p.includes('refusé')));
  const solo = buildManifest({ ...couple, composition: 'solo', p2: null });
  assert.deepEqual(validateManifest(solo), []);
  assert.ok(validateManifest({ ...couple, workflowId: '' }).some(p => p.includes('Workflow')));
  assert.ok(validateManifest({ ...couple, p1: { characterId: '', variantId: null } }).some(p => p.includes('P1')));
});

test('preset sources require a role and SOLO restricts it to P1', () => {
  const couple = coupleManifest();
  assert.deepEqual(validateManifest(couple), []);
  const noRole = buildManifest({ ...couple, action: sceneText('preset', null, '') });
  assert.ok(validateManifest(noRole).some(p => p.includes('ACTION') && p.includes('rôle')));
  const couple2 = JSON.parse(JSON.stringify(couple));
  couple2.action.from = 'P2';
  assert.deepEqual(validateManifest(couple2), []);
  const solo = buildManifest({ ...couple, composition: 'solo', p2: null });
  const fromP2 = buildManifest({ ...solo, action: sceneText('preset', 'P2', '') });
  assert.ok(validateManifest(fromP2).some(p => p.includes('SOLO') && p.includes('P1')));
});

test('mask parameters stay inside the SayaSplitMask adapter contract', () => {
  const base = coupleManifest();
  assert.deepEqual(validateManifest(base), []);
  assert.ok(validateManifest({ ...base, mask: { nodeId: 'm', direction: 'diagonal', split: 50, blur: 0, swap: false } }).length > 0);
  assert.ok(validateManifest({ ...base, mask: { nodeId: 'm', direction: 'vertical', split: 101, blur: 0, swap: false } }).some(p => p.includes('0 et 100')));
  assert.ok(validateManifest({ ...base, mask: { nodeId: 'm', direction: 'vertical', split: 49.5, blur: 0, swap: false } }).length > 0);
  assert.ok(validateManifest({ ...base, mask: { nodeId: 'm', direction: 'vertical', split: 50, blur: -1, swap: false } }).length > 0);
});

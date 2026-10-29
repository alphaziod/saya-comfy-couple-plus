import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  buildVisualManifest, configFieldType, configKeyError, configValueError, loadImageNodes, move,
  newDraftConfig, pruneBindings, pruneConfigLinks, removeStep, seedValueError, validateDraft,
  validateVisualDraft,
} from '../src/lib/pipeline-builder.ts';

const two = [
  { id: 'base', workflowId: 'w1', sourceStep: '', nodeId: '' },
  { id: 'up', workflowId: 'w2', sourceStep: 'base', nodeId: '12' },
];

test('manifeste conforme (v2 multiline : prompts de config globale)', () => {
  const m = buildVisualManifest(' Mon pipe ', two, [], [], 'id-1');
  assert.equal(m.format, 'saya-workflow-pipeline');
  assert.equal(m.version, 2, 'le lecteur noyau accepte 1 et 2 ; la v2 admet les \\n des prompts');
  assert.equal(m.name, 'Mon pipe');
  assert.deepEqual(m.workflows, [{ id: 'w1' }, { id: 'w2' }]);
  assert.deepEqual(m.steps[0], { id: 'base', workflowId: 'w1', kind: 'image', outputs: [{ name: 'image', portType: 'IMAGE' }] });
  assert.deepEqual(m.steps[1].bindings, [{ port: 'image', nodeId: '12', input: 'image', source: { step: 'base', port: 'image' } }]);
  assert.deepEqual(m.steps[1].inputs, [{ name: 'image', portType: 'IMAGE' }]);
  assert.ok(!('parameters' in m.steps[0]) && !('seed' in m.steps[0]), 'aucun paramètre sans lien de config');
});

test('config globale reliée : parameters par section, seed exacte', () => {
  const configs = [
    { id: 'config1', label: 'Config globale', fields: [
      { key: 'P1', value: 'elf, solo\nligne 2' },
      { key: 'NEG', value: '' },
    ] },
    { id: 'config2', label: 'Rendu', fields: [
      { key: 'seed', value: '9007199254740993' },
      { key: '1.ckpt_name', value: 'model.safetensors' },
    ] },
  ];
  const links = [
    { config: 'config1', step: 'base', field: 'P1' },
    { config: 'config2', step: 'up', field: 'seed' },
    { config: 'config2', step: 'up', field: '1.ckpt_name' },
  ];
  const m = buildVisualManifest('pipe', two, configs, links, 'id-2');
  assert.deepEqual(m.steps[0].parameters, { P1: 'elf, solo\nligne 2' });
  assert.equal(m.steps[1].seed, '9007199254740993', 'la seed voyage en u64 exact, pas en paramètre');
  assert.deepEqual(m.steps[1].parameters, { '1.ckpt_name': 'model.safetensors' });
  const sans = buildVisualManifest('pipe', two, configs, [links[0]], 'id-3');
  assert.ok(!('seed' in sans.steps[1]), 'seed absente sans lien');
});

test('validation locale des passes', () => {
  assert.deepEqual(validateDraft('x', two), []);
  assert.ok(validateDraft('', two).length);
  assert.ok(validateDraft('x', []).length);
  assert.ok(validateDraft('x', [{ ...two[0], id: '-bad' }]).length);
  assert.ok(validateDraft('x', [two[0], { ...two[1], id: 'base' }]).length);
  assert.ok(validateDraft('x', [{ ...two[0], workflowId: '' }]).length);
  assert.ok(validateDraft('x', [two[0], { ...two[1], nodeId: '' }]).length);
  assert.ok(validateDraft('x', [{ ...two[0], sourceStep: 'base', nodeId: '1' }]).length);
});

test('clés et valeurs de config : miroir des refus du noyau', () => {
  assert.equal(configKeyError('P1'), null);
  assert.equal(configKeyError('NEG'), null);
  assert.equal(configKeyError('seed'), null);
  assert.equal(configKeyError('1.ckpt_name'), null);
  assert.ok(configKeyError(''));
  assert.ok(configKeyError('.ckpt'));
  assert.ok(configKeyError('node.'));
  assert.ok(configKeyError('prompt\nmultiligne'));
  assert.equal(configFieldType('seed'), 'INT');
  assert.equal(configFieldType('P1'), 'STRING');
  assert.equal(seedValueError('18446744073709551615'), null);
  assert.ok(seedValueError('18446744073709551616'), 'hors u64');
  assert.ok(seedValueError('0123'), 'non canonique');
  assert.ok(seedValueError('-1'));
  assert.equal(configValueError('ligne\nsaut\ttab'), null, 'v2 : \\n \\r \\t admis');
  assert.ok(configValueError('ctrl\x00'));
  assert.ok(configValueError('x'.repeat(200_001)), 'longueur v2 bornée');
});

test('validation visuelle : doublons de champ sur une passe refusés', () => {
  const configs = [
    newDraftConfig([]),
    { id: 'config2', label: 'B', fields: [{ key: 'P1', value: 'x' }] },
  ];
  configs[0].label = 'A';
  configs[0].fields.push({ key: 'P1', value: 'y' }, { key: 'P1', value: 'z' });
  assert.ok(validateVisualDraft('pipe', two, configs, []).some(e => e.includes('défini deux fois')), 'doublon dans un bloc');
  assert.ok(
    validateVisualDraft('pipe', two, configs, [
      { config: 'config2', step: 'base', field: 'P1' },
      { config: 'config1', step: 'base', field: 'P1' },
    ]).some(e => e.includes('fourni deux fois')),
    'même champ depuis deux blocs vers la même passe',
  );
  assert.deepEqual(
    validateVisualDraft('pipe', two, configs, [{ config: 'config2', step: 'base', field: 'P1' }]).filter(e => e.includes('fourni')),
    [],
    'le même champ vers deux passes différentes reste licite',
  );
  assert.ok(validateVisualDraft('pipe', two, configs, [{ config: 'absent', step: 'base', field: 'P1' }]).length, 'bloc inconnu');
  assert.ok(validateVisualDraft('pipe', two, configs, [{ config: 'config2', step: 'base', field: 'absent' }]).length, 'champ inconnu');
  assert.ok(validateVisualDraft('pipe', two, [{ ...configs[0], label: '' }], []).some(e => e.includes('nom vide')));
});

test('retrait et élagage des liens de config', () => {
  const configs = [{ id: 'c1', label: 'A', fields: [{ key: 'P1', value: '' }] }];
  const steps3 = [...two, { id: 'trois', workflowId: 'w3', sourceStep: '', nodeId: '' }];
  const links = [
    { config: 'c1', step: 'base', field: 'P1' },
    { config: 'c1', step: 'fantôme', field: 'P1' },
    { config: 'c1', step: 'base', field: 'absent' },
  ];
  assert.deepEqual(pruneConfigLinks(configs, steps3, links), [links[0]]);
  const restants = removeStep(steps3, 0);
  assert.equal(restants[0].sourceStep, '', 'retirer une passe purge sa liaison image');
  assert.equal(loadImageNodes([{ id: '1', classType: 'LoadImage', title: null }, { id: '2', classType: 'SaveImage', title: null }]).length, 1);
  assert.deepEqual(move([1, 2, 3], 0, 1), [2, 1, 3]);
  assert.deepEqual(move([1, 2], 0, -1), [1, 2]);
  assert.equal(pruneBindings(move(two, 1, -1))[0].sourceStep, '');
});

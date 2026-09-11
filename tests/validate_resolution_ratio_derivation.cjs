const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '../web/saya_resolution_ratio_filter.js'), 'utf8')
    .replace(/^import .*;$/gm, '');
const context = { app: { registerExtension() {} }, console };
vm.createContext(context);
vm.runInContext(source, context);

// --- deriveAspectLabel: resolution_preset name -> aspect_preset_when_not_image option ---
// Every case the user asked to be covered: 16:9, 4:3, 1:1, 2:3, 3:4, 9:16,
// plus 7:9 and its 9:7 landscape counterpart (both now real ratio families
// in src/nodes/saya_resolution_scale.py's reorganized preset table).
const cases = [
    ['Landscape 16:9 · 896x512', '16:9 - Landscape'],
    ['Landscape 16:9 · 1344x768', '16:9 - Landscape'],
    ['Landscape 4:3 · 1024x768', '4:3 - Landscape'],
    ['Square 1:1 · 896x896', '1:1 - Square'],
    ['Portrait 2:3 · 832x1216', '2:3 - Portrait'],
    ['Portrait 3:4 · 768x1024', '3:4 - Portrait'],
    ['Portrait 9:16 · 512x896', '9:16 - Portrait'],
    ['Landscape 3:2 · 960x640', '3:2 - Landscape'],
    ['Portrait 7:9 · 896x1152', '7:9 - Portrait'],
    ['Landscape 9:7 · 1152x896', '9:7 - Landscape'],
];
for (const [presetName, expectedAspect] of cases) {
    const derived = context.deriveAspectLabel(presetName);
    assert.equal(derived, expectedAspect, `${presetName} -> expected "${expectedAspect}", got "${derived}"`);
}
console.log('PASS deriveAspectLabel matches ASPECT_PRESETS keys for 16:9, 4:3, 1:1, 2:3, 3:4, 3:2, 9:16, 7:9, 9:7');

// --- syncAspectFromPreset: one-way, only writes an option that really exists ---
function makeNode(presetValue, aspectValue, aspectOptions) {
    const presetWidget = { name: 'resolution_preset', value: presetValue, callback: null };
    const aspectWidget = { name: 'aspect_preset_when_not_image', value: aspectValue, options: { values: aspectOptions }, callback(v) { aspectWidget.value = v; } };
    return { widgets: [presetWidget, aspectWidget], presetWidget, aspectWidget, setDirtyCanvas() {} };
}

const allAspectOptions = [
    'All (no filter)', '1:1 - Square', '4:3 - Landscape', '3:4 - Portrait',
    '3:2 - Landscape', '2:3 - Portrait', '16:9 - Landscape', '9:16 - Portrait',
    '21:9 - Ultrawide', '9:21 - Ultrawide Portrait', 'CUSTOM',
];

// Exact reported bug state: resolution_preset=16:9, aspect stuck on 4:3.
{
    const node = makeNode('Landscape 16:9 · 896x512', '4:3 - Landscape', allAspectOptions);
    context.syncAspectFromPreset(node);
    assert.equal(node.aspectWidget.value, '16:9 - Landscape', 'must correct a stale aspect to match resolution_preset on install');
}
console.log('PASS syncAspectFromPreset corrects an already-mismatched pair (the exact reported bug state)');

// Reverse direction must never happen: changing the aspect widget's value
// directly (not through resolution_preset) must not touch resolution_preset.
{
    const node = makeNode('Landscape 16:9 · 896x512', '16:9 - Landscape', allAspectOptions);
    node.aspectWidget.value = '1:1 - Square'; // simulate a direct user edit
    context.syncAspectFromPreset(node); // only called from resolution_preset's own callback in the real file
    assert.equal(node.presetWidget.value, 'Landscape 16:9 · 896x512', 'resolution_preset must never be written by this file');
}
console.log('PASS resolution_preset is never written (one-way only)');

// A derived label that is not an actual option must be left alone (no crash, no wrong write).
{
    const node = makeNode('Landscape 16:9 · 896x512', '9:16 - Portrait', ['9:16 - Portrait', 'CUSTOM']); // 16:9 missing on purpose
    context.syncAspectFromPreset(node);
    assert.equal(node.aspectWidget.value, '9:16 - Portrait', 'must not fabricate an option that does not exist');
}
console.log('PASS never writes an aspect value that is not a real option');

// install() must not filter resolution_preset's own option list.
{
    const node = makeNode('Landscape 16:9 · 896x512', '4:3 - Landscape', allAspectOptions);
    node.presetWidget.options = { values: ['Landscape 16:9 · 896x512', 'Square 1:1 · 768x768', 'Portrait 9:16 · 512x896'] };
    const before = [...node.presetWidget.options.values];
    context.install(node);
    assert.deepEqual(node.presetWidget.options.values, before, 'resolution_preset options must be untouched (no filtering)');
}
console.log('PASS resolution_preset option list is never filtered');

// aspect_preset_when_not_image must become non-interactive (disabled), not
// removed from the node -- removing it would shift every later widget's
// position in old saved workflows' positional widgets_values arrays.
{
    const node = makeNode('Landscape 16:9 · 896x512', '4:3 - Landscape', allAspectOptions);
    context.install(node);
    assert.equal(node.aspectWidget.disabled, true, 'aspect widget must be disabled');
    assert.equal(node.aspectWidget.options.disabled, true, 'aspect widget options.disabled must be set (Vue combo renderer reads this)');
}
console.log('PASS aspect_preset_when_not_image is disabled (read-only), not removed');

console.log('ALL RESOLUTION RATIO DERIVATION TESTS PASSED');

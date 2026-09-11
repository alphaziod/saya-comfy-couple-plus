const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../web/image_phase_controller.js'), 'utf8')
    .replace(/^import .*;$/gm, '');
const context = { app: { registerExtension() {} }, api: {}, console };
vm.createContext(context);
vm.runInContext(source, context);
for (const enabled of [false, true]) {
    const workflow = { original: 'unchanged' };
    const prompt = { workflow, output: { gate: {
        class_type: 'LazySwitchKJ', inputs: {
            switch: enabled, on_false: ['previous_image', 0], on_true: ['hidream_image', 0]
        }
    } } };
    context.pruneDisabledLazyBranches(prompt);
    const inputs = prompt.output.gate.inputs;
    assert.deepEqual(inputs.on_false, inputs.on_true);
    assert.equal(inputs.on_false[0], enabled ? 'hidream_image' : 'previous_image');
    assert.equal(prompt.workflow, workflow);
}
const dynamic = { output: { gate: { class_type: 'LazySwitchKJ', inputs: {
    switch: ['boolean_node', 0], on_false: ['a', 0], on_true: ['b', 0]
} } } };
const original = JSON.stringify(dynamic);
context.pruneDisabledLazyBranches(dynamic);
assert.equal(JSON.stringify(dynamic), original);
console.log('PASS static lazy branches pruned for validation; dynamic links and saved workflow preserved');

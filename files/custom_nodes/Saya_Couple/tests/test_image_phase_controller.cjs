const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const source = fs.readFileSync(path.join(__dirname, "../web/image_phase_controller.js"), "utf8")
    .replace(/^import .*;$/gm, "");
const timers = [];
const captures = [];
const seedWidget = { name: "seed", value: -1, callback(value) { this.callbackValue = value; } };
const masterSeed = {
    properties: { saya_master_seed: true },
    widgets: [seedWidget],
    setDirtyCanvas() { this.dirty = true; },
};
const phase1 = { mode: 0, properties: { saya_phase: 1, saya_original_mode: 0 }, changeMode(v) { this.mode = v; } };
const phase2 = { mode: 2, properties: { saya_phase: 2, saya_original_mode: 0 }, changeMode(v) { this.mode = v; } };
const app = {
    graph: { _nodes: [masterSeed, phase1, phase2], setDirtyCanvas() {} },
    registerExtension() {},
    async graphToPrompt() {
        captures.push([phase1.mode, phase2.mode]);
        return { output: {} };
    },
    async queuePrompt() { return this.graphToPrompt(); },
};
const context = {
    app,
    api: { addEventListener() {}, apiURL(v) { return v; } },
    async fetch() { return { ok: true, async json() { return { ok: true, removed: [] }; } }; },
    console,
    crypto: { getRandomValues(values) { values[0] = 1; values[1] = 2; } },
    window: {
        setTimeout(callback) { timers.push(callback); return timers.length; },
        clearTimeout() {},
    },
};
vm.createContext(context);
vm.runInContext(source, context);

(async () => {
    for (const name of ["randomizeMasterSeedInPrompt", "randomSeed", "updateMasterSeed", "masterSeedNode"]) {
        assert.equal(source.includes(name), false, `${name} must not exist: rgthree owns the seed`);
    }
    const restartBody = source.slice(source.indexOf("restartButton.onclick"), source.indexOf("function registerReviewNode"));
    assert.match(restartBody, /\/saya\/image-phases\/redo/, "REFAIRE calls /redo");
    assert.match(restartBody, /queuePhase\(1,/, "REFAIRE requeues Phase 1");
    assert.doesNotMatch(restartBody, /seed/i, "REFAIRE never touches a seed");
    assert.equal(seedWidget.value, -1, "Master Seed widget stays -1");

    await app.queuePrompt();
    assert.equal(seedWidget.value, -1, "Master Seed widget stays -1 after a queue");
    assert.deepEqual(captures.at(-1), [0, 2], "normal external queue starts Phase 1");
    assert.equal(timers.length, 0, "Phase 2 is not scheduled before explicit validation");

    await context.queuePhase(2, 0);
    await timers.shift()();
    assert.deepEqual(captures.at(-1), [2, 0], "internal continuation selects Phase 2");

    await app.queuePrompt();
    assert.deepEqual(captures.at(-1), [0, 2],
        "new external queue must reset stale Phase 2 state to Phase 1");
    console.log("PASS external queue resets stale phase state to Phase 1");
})().catch((error) => {
    console.error(error);
    process.exitCode = 1;
});

// Audit A1-02: a failure after Phase 1 must not invalidate the validated Phase 1
// checkpoint; A1-01: CONTINUE sends the reviewed transaction.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const source = fs.readFileSync(path.join(__dirname, "../web/image_phase_controller.js"), "utf8")
    .replace(/^import .*;$/gm, "");

function makeController() {
    const posts = [];
    const listeners = {};
    const timers = [];
    const phase1 = { mode: 0, properties: { saya_phase: 1, saya_original_mode: 0 }, changeMode(v) { this.mode = v; } };
    const phase3 = { mode: 2, properties: { saya_phase: 3, saya_original_mode: 0 }, changeMode(v) { this.mode = v; } };
    const app = {
        graph: { _nodes: [phase1, phase3], setDirtyCanvas() {} },
        registerExtension(ext) { app.ext = ext; },
        async graphToPrompt() { return { output: {} }; },
        async queuePrompt() { return this.graphToPrompt(); },
    };
    const context = {
        app,
        api: { addEventListener(name, cb) { listeners[name] = cb; }, apiURL(v) { return v; } },
        async fetch(url, opts) {
            posts.push({ url, body: JSON.parse(opts?.body ?? "{}") });
            return { ok: true, async json() { return { ok: true, removed: [] }; } };
        },
        console: { ...console, info() {}, debug() {} },
        crypto: { getRandomValues(v) { v[0] = 1; v[1] = 2; } },
        window: { setTimeout(cb) { timers.push(cb); return timers.length; }, clearTimeout() {} },
    };
    vm.createContext(context);
    vm.runInContext(source, context);
    app.ext.setup();
    return { app, context, posts, listeners, timers };
}

(async () => {
    const continueBody = source.slice(source.indexOf("continueButton.onclick"), source.indexOf("restartButton.onclick"));
    assert.match(continueBody, /transaction_uuid:\s*review\.transaction_uuid/, "CONTINUE sends the reviewed transaction");

    // Failure while Phase 3 is running: only the Phase 3 candidate is dropped.
    {
        const { app, context, posts, listeners, timers } = makeController();
        await app.queuePrompt();
        await context.queuePhase(3, 0);
        await timers.shift()();
        posts.length = 0;
        await listeners.execution_error({ detail: { exception_message: "OOM in phase 3" } });
        await new Promise((resolve) => setImmediate(resolve));
        const redo = posts.filter((p) => p.url.includes("/redo"));
        assert.equal(redo.length, 1, "one cleanup call");
        assert.equal(redo[0].body.phase, 3, "failure in Phase 3 cleans up Phase 3, never Phase 1");
    }

    // Failure during Phase 1 keeps the previous behaviour.
    {
        const { app, posts, listeners } = makeController();
        await app.queuePrompt();
        posts.length = 0;
        await listeners.execution_interrupted({ detail: {} });
        await new Promise((resolve) => setImmediate(resolve));
        const redo = posts.filter((p) => p.url.includes("/redo"));
        assert.equal(redo.length, 1, "one cleanup call");
        assert.equal(redo[0].body.phase, 1, "failure in Phase 1 cleans up Phase 1");
    }
    // After a failure in Phase 3, the next manual queue can resume at Phase 3 (no Phase 1 reset).
    for (const accept of [true, false]) {
        const { app, context, posts, listeners, timers } = makeController();
        context.window.confirm = () => accept;
        await app.queuePrompt();
        await context.queuePhase(3, 0);
        await timers.shift()();
        await listeners.execution_interrupted({ detail: {} });
        await new Promise((resolve) => setImmediate(resolve));
        posts.length = 0;
        await app.queuePrompt();
        const phase = vm.runInContext("activePhase", context);
        const redo = posts.filter((p) => p.url.includes("/redo"));
        if (accept) {
            assert.equal(phase, 3, "resume queues Phase 3");
            assert.equal(redo.length, 0, "resume never resets Phase 1");
        } else {
            assert.equal(phase, 1, "declining starts a new image at Phase 1");
            assert.deepEqual(redo.map((p) => p.body.phase), [1], "declining resets Phase 1");
        }
        await app.queuePrompt();
        assert.equal(vm.runInContext("activePhase", context), 1, "the resume offer is used once");
    }
    console.log("PASS phase failure cleanup targets the failing phase; resume after failure");
})().catch((error) => {
    console.error(error);
    process.exitCode = 1;
});

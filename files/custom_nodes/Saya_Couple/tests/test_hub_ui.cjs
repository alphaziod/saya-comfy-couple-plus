const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const source = fs.readFileSync(path.join(__dirname, "../web/saya_hub_ui.js"), "utf8")
    .replace(/^import .*;$/gm, "");
let extension;
const context = { app: { registerExtension(e) { extension = e; } }, console };
vm.createContext(context);
vm.runInContext(source, context);

function modelWidget(name, value = "Model 1") {
    return { name, value, options: { values: ["Model 1", "Model 2"] } };
}

function makeNode({ title, type = "subgraph", widgets = [], inputs = [], outputs = [], size = [120, 80], pos = [44, 55] }) {
    return {
        title, type, comfyClass: type, widgets, inputs, outputs, size: [...size], pos: [...pos],
        setSize(v) { this.size = [...v]; },
        setDirtyCanvas() { this.dirty = true; },
    };
}

{
    const widgets = Array.from({ length: 13 }, (_, i) => `Detailer ${String(i + 1).padStart(2, "0")}`)
        .map((name) => modelWidget(`${name} Model`));
    const node = makeNode({
        title: "Hub Detailers · Model 1/2",
        type: "saya-model-hub-detailers",
        widgets,
        inputs: [{ name: "Model 1 Patched" }, { name: "Model 2 Patched" }],
        outputs: [{ name: "Detailer 01 Model" }, { name: "Detailer 13 Model" }],
    });
    const beforePos = [...node.pos];
    extension.loadedGraphNode(node);
    assert.deepEqual(node.pos, beforePos, "hub UI must never move nodes");
    assert.equal(node.title, "Hub Detailers · Model Routing");
    assert.ok(node.size[0] >= 350 && node.size[1] >= 470, "detailer hub gets readable minimum size");
    assert.equal(node.widgets[0].label, "Detailer 01");
    assert.equal(node.widgets[12].label, "Detailer 13");
    assert.equal(node.outputs[0].label, "Detailer 01 · MODEL");
    assert.equal(node.outputs[1].label, "Detailer 13 · MODEL");
    assert.equal(node.inputs[0].label, "Model 1 · Patched");
}

{
    const node = makeNode({
        title: "Hub USDU · Model 1/2",
        type: "saya-model-hub-usdu",
        widgets: ["USDU 1", "USDU 2", "Hires Fix 1", "Hires Fix 3"].map((name) => modelWidget(`${name} Model`)),
        outputs: [
            { name: "USDU 1 Model" }, { name: "USDU 2 Model" },
            { name: "Hires Fix 1 Model" }, { name: "Hires Fix 3 Model" },
        ],
    });
    extension.loadedGraphNode(node);
    assert.equal(node.widgets[0].label, "USDU 1 · Model");
    assert.equal(node.widgets[3].label, "Hires Fix 3 · Model");
    assert.equal(node.outputs[2].label, "Hires Fix 1 · MODEL");
}

{
    const node = makeNode({
        title: "Hub Phase 6",
        type: "saya-model-hub-phase6",
        widgets: [modelWidget("Phase 6 Model")],
        outputs: [
            { name: "MODEL" }, { name: "CLIP" },
            { name: "checkpoint_identities" }, { name: "identifier" },
        ],
    });
    extension.loadedGraphNode(node);
    assert.equal(node.widgets[0].label, "Phase 6 · Model");
    assert.deepEqual(node.outputs.map((o) => o.label), [
        "Selected MODEL", "Selected CLIP", "Selected Identities", "Selected Identifier",
    ]);
}

{
    const node = makeNode({
        title: "Couple Mode",
        type: "saya-couple-mode",
        widgets: [{ name: "Couple Mode", value: "ON", options: { values: ["ON", "OFF"] } }],
        outputs: [{ name: "solo" }],
    });
    extension.loadedGraphNode(node);
    assert.equal(node.widgets[0].label, "Couple Mode");
    assert.equal(node.outputs[0].label, "SOLO flag");
}

{
    const node = makeNode({
        title: "VAE Routing Hub",
        type: "saya-vae-switch-hub-main-supports-only",
        widgets: [modelWidget("not actually model", "Main Model VAE")],
        outputs: [{ name: "10 Detailers VAE" }],
        size: [322, 462],
    });
    const snapshot = JSON.stringify(node);
    extension.loadedGraphNode(node);
    assert.equal(JSON.stringify(node), snapshot, "unrelated VAE hub must remain untouched");
}

console.log("PASS hub UI config: labels/sizes only, positions and unrelated hubs untouched");

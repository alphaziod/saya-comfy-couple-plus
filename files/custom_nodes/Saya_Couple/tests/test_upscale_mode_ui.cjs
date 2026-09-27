const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const source = fs.readFileSync(path.join(__dirname, "../web/saya_upscale_mode.js"), "utf8")
    .replace(/^import .*;$/gm, "").replace(/^export /gm, "");
let extension;
const context = { app: { registerExtension(e) { extension = e; } } };
vm.createContext(context);
vm.runInContext(source, context);

const make = (value, cls = "SayaUpscaleMode") => ({ comfyClass: cls, widgets: [{ name: "upscale_by", value }] });
for (const bad of [NaN, Infinity, -1, 0, "lanczos", undefined, null]) {
    const node = make(bad);
    extension.loadedGraphNode(node);
    assert.equal(node.widgets[0].value, 1.0, `${String(bad)} normalised to 1.0`);
}
const ok = make(2.0);
extension.loadedGraphNode(ok);
assert.equal(ok.widgets[0].value, 2.0, "valid value untouched");
const other = make(NaN, "OtherNode");
extension.loadedGraphNode(other);
assert.ok(Number.isNaN(other.widgets[0].value), "other node classes untouched");
console.log("PASS upscale_by NaN normalised on load");

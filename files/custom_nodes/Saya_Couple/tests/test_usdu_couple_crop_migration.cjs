const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const source = fs.readFileSync(path.join(__dirname, "../web/saya_usdu_couple_crop_migration.js"), "utf8")
    .replace(/^import .*;$/gm, "").replace(/^export /gm, "");
let extension;
const context = { app: { registerExtension(e) { extension = e; } } };
vm.createContext(context);
vm.runInContext(source, context);

const widgetInputs = (names) => names.map((name) => ({ name, type: "X", widget: { name }, link: null }));
const legacy = (coupleCrop, restore) => ({
    widgets_values: [0, "fixed", "usdu_1", 1, 512, 32, 8, 1.5, 12, "euler_cfg_pp", "beta", 0.12, 0.75, coupleCrop, restore],
    widgets_values_named: { couple_crop: coupleCrop, restore_to_base: restore },
    inputs: [{ name: "image", type: "IMAGE", link: 1 }, { name: "seed", type: "INT", widget: { name: "seed" }, link: 2 },
             ...widgetInputs(["couple_crop", "restore_to_base"])],
});

for (const [coupleCrop, restore] of [[true, false], [false, true], [true, true]]) {
    const info = legacy(coupleCrop, restore);
    context.migrateLegacyCoupleCrop(info);
    assert.equal(info.widgets_values.length, 14, "legacy couple_crop value dropped");
    assert.equal(info.widgets_values[13], restore, "restore_to_base keeps ITS value, not couple_crop's");
    assert.equal("couple_crop" in info.widgets_values_named, false, "named value dropped");
    assert.deepEqual(info.inputs.map((i) => i.name), ["image", "seed", "restore_to_base", "solo"], "stale slot removed, solo added");
    assert.equal(info.inputs[3].link, null, "solo must be wired by the user (required input)");
}

const current = legacy(true, false);
current.widgets_values.splice(13, 1);
const before = JSON.stringify(current.widgets_values);
context.migrateLegacyCoupleCrop(current);
context.migrateLegacyCoupleCrop(current);
assert.equal(JSON.stringify(current.widgets_values), before, "current format untouched, idempotent");
assert.equal(current.inputs.filter((i) => i.name === "solo").length, 1, "solo never duplicated");

const linkedAfter = legacy(true, false);
linkedAfter.inputs[3].link = 9; // restore_to_base linked after the stale slot
context.migrateLegacyCoupleCrop(linkedAfter);
assert.ok(linkedAfter.inputs.some((i) => i.name === "couple_crop"), "slot kept when removing it would shift a linked slot");

let configured;
const proto = { configure(info) { configured = info; } };
extension.beforeRegisterNodeDef({ prototype: proto }, { name: "SayaCoupleUSDUPass" });
proto.configure(legacy(true, false));
assert.equal(configured.widgets_values.length, 14, "migration runs before configure");
const other = { configure(info) { configured = info; } };
extension.beforeRegisterNodeDef({ prototype: other }, { name: "OtherNode" });
other.configure(legacy(true, false));
assert.equal(configured.widgets_values.length, 15, "other node classes untouched");
console.log("PASS usdu couple_crop legacy workflows migrated");

// @ts-expect-error ComfyUI injects this runtime module.
import { app } from "../../scripts/app.js";

// SayaCoupleUSDUPass lost its `couple_crop` widget: the couple crop now
// follows the global Couple/Solo mode through the required `solo` input.
// Workflows saved before that still carry the old value at widget index 13,
// and widgets_values are restored positionally, so without this migration
// restore_to_base would silently receive the old couple_crop value.

const NODE = "SayaCoupleUSDUPass";
const LEGACY_WIDGET_COUNT = 15; // seed, control_after_generate, ..., couple_crop, restore_to_base
const LEGACY_INDEX = 13;

export function migrateLegacyCoupleCrop(info) {
    const values = info.widgets_values;
    if (Array.isArray(values) && values.length === LEGACY_WIDGET_COUNT && typeof values[LEGACY_INDEX] === "boolean") {
        values.splice(LEGACY_INDEX, 1);
    }
    if (info.widgets_values_named) {
        delete info.widgets_values_named.couple_crop;
    }
    const inputs = info.inputs;
    if (!Array.isArray(inputs)) {
        return;
    }
    const index = inputs.findIndex((input) => input.name === "couple_crop");
    // Dropping a slot shifts the ones after it; only do it when none of them is linked.
    if (index !== -1 && inputs.slice(index).every((input) => input.link == null)) {
        inputs.splice(index, 1);
    }
    if (!inputs.some((input) => input.name === "solo")) {
        inputs.push({ name: "solo", type: "BOOLEAN", link: null });
    }
}

app.registerExtension({
    name: "saya.usdu.coupleCropMigration",
    beforeRegisterNodeDef(nodeType, nodeData) {
        if (nodeData.name !== NODE) {
            return;
        }
        const configure = nodeType.prototype.configure;
        nodeType.prototype.configure = function (info) {
            migrateLegacyCoupleCrop(info);
            return configure.apply(this, arguments);
        };
    },
});

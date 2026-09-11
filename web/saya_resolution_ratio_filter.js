// @ts-expect-error ComfyUI injects this runtime module.
import { app } from "../../scripts/app.js";

// Filters the "resolution_preset" combo of SayaResolutionScaleCalculator down
// to the presets matching the chosen "ratio_filter" family. Presets are named
// "<Family> · <W>x<H>" (see src/nodes/saya_resolution_scale.py), so filtering
// is a plain prefix match — no duplicated ratio data needed on the JS side.
const NODE_CLASS = "SayaResolutionScaleCalculator";
const FILTER_WIDGET = "ratio_filter";
const PRESET_WIDGET = "resolution_preset";
const ALL = "All";
const SEPARATOR = " · ";

function familyOf(presetName) {
    const index = presetName.indexOf(SEPARATOR);
    return index === -1 ? presetName : presetName.slice(0, index);
}

function findWidget(node, name) {
    return node.widgets?.find((widget) => widget.name === name);
}

function applyFilter(node) {
    const filterWidget = findWidget(node, FILTER_WIDGET);
    const presetWidget = findWidget(node, PRESET_WIDGET);
    if (!filterWidget || !presetWidget) return;

    const allPresets = node.__sayaAllPresets ?? presetWidget.options?.values ?? [];
    node.__sayaAllPresets = allPresets;

    const family = String(filterWidget.value ?? ALL);
    const filtered = family === ALL
        ? allPresets
        : allPresets.filter((name) => familyOf(name) === family);

    presetWidget.options.values = filtered.length ? filtered : allPresets;

    if (!presetWidget.options.values.includes(presetWidget.value)) {
        presetWidget.value = presetWidget.options.values[0];
        presetWidget.callback?.(presetWidget.value);
    }
    node.setDirtyCanvas?.(true, true);
}

function installFilter(node) {
    if (node.__sayaRatioFilterInstalled) return;
    node.__sayaRatioFilterInstalled = true;

    const filterWidget = findWidget(node, FILTER_WIDGET);
    const presetWidget = findWidget(node, PRESET_WIDGET);
    if (!filterWidget || !presetWidget) return;

    node.__sayaAllPresets = [...(presetWidget.options?.values ?? [])];

    const previousCallback = filterWidget.callback;
    filterWidget.callback = function (...args) {
        const result = previousCallback?.apply(this, args);
        applyFilter(node);
        return result;
    };

    applyFilter(node);
}

app.registerExtension({
    name: "Saya.ResolutionRatioFilter",
    async beforeRegisterNodeDef(nodeType, nodeData) {
        if (nodeData.name !== NODE_CLASS) return;
        const created = nodeType.prototype.onNodeCreated;
        nodeType.prototype.onNodeCreated = function (...args) {
            const result = created?.apply(this, args);
            installFilter(this);
            return result;
        };
    },
    loadedGraphNode(node) {
        if (String(node?.comfyClass ?? node?.type ?? "") === NODE_CLASS) {
            installFilter(node);
        }
    },
});

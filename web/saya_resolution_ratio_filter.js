// @ts-expect-error ComfyUI injects this runtime module.
import { app } from "../../scripts/app.js";

// Filters the "resolution_preset" combo of SayaResolutionScaleCalculator down
// to the presets matching "aspect_preset_when_not_image" (CUSTOM uses
// custom_aspect_width/custom_aspect_height instead). Both the aspect labels
// (e.g. "16:9 - Landscape") and the preset names (e.g. "Landscape 16:9 ·
// 1344x768", see src/nodes/saya_resolution_scale.py) embed the same plain
// "A:B" ratio text, so filtering is a text match — no ratio table duplicated
// here that could drift out of sync with the Python side.
const NODE_CLASS = "SayaResolutionScaleCalculator";
const ASPECT_WIDGET = "aspect_preset_when_not_image";
const PRESET_WIDGET = "resolution_preset";
const CUSTOM_WIDTH_WIDGET = "custom_aspect_width";
const CUSTOM_HEIGHT_WIDGET = "custom_aspect_height";
const CUSTOM_VALUE = "CUSTOM";
const RATIO_PATTERN = /(\d+):(\d+)/;

function findWidget(node, name) {
    return node.widgets?.find((widget) => widget.name === name);
}

function gcd(a, b) {
    return b === 0 ? a : gcd(b, a % b);
}

function ratioToken(width, height) {
    const divisor = gcd(Math.max(1, Math.round(width)), Math.max(1, Math.round(height))) || 1;
    return `${Math.round(width) / divisor}:${Math.round(height) / divisor}`;
}

function targetRatioToken(node) {
    const aspectWidget = findWidget(node, ASPECT_WIDGET);
    const aspectValue = String(aspectWidget?.value ?? "");

    if (aspectValue === CUSTOM_VALUE) {
        const width = Number(findWidget(node, CUSTOM_WIDTH_WIDGET)?.value ?? 0);
        const height = Number(findWidget(node, CUSTOM_HEIGHT_WIDGET)?.value ?? 0);
        return width > 0 && height > 0 ? ratioToken(width, height) : null;
    }

    const match = aspectValue.match(RATIO_PATTERN);
    return match ? `${match[1]}:${match[2]}` : null; // no match => "All (no filter)"
}

function applyFilter(node) {
    const presetWidget = findWidget(node, PRESET_WIDGET);
    if (!presetWidget) return;

    const allPresets = node.__sayaAllPresets ?? presetWidget.options?.values ?? [];
    node.__sayaAllPresets = allPresets;

    const targetRatio = targetRatioToken(node);
    const filtered = targetRatio === null
        ? allPresets
        : allPresets.filter((name) => name.match(RATIO_PATTERN)?.[0] === targetRatio);

    presetWidget.options.values = filtered.length ? filtered : allPresets;

    if (!presetWidget.options.values.includes(presetWidget.value)) {
        presetWidget.value = presetWidget.options.values[0];
        presetWidget.callback?.(presetWidget.value);
    }
    node.setDirtyCanvas?.(true, true);
}

function wrapCallback(widget, node) {
    const previousCallback = widget.callback;
    widget.callback = function (...args) {
        const result = previousCallback?.apply(this, args);
        applyFilter(node);
        return result;
    };
}

function installFilter(node) {
    if (node.__sayaRatioFilterInstalled) return;

    const aspectWidget = findWidget(node, ASPECT_WIDGET);
    const presetWidget = findWidget(node, PRESET_WIDGET);
    const widthWidget = findWidget(node, CUSTOM_WIDTH_WIDGET);
    const heightWidget = findWidget(node, CUSTOM_HEIGHT_WIDGET);
    if (!aspectWidget || !presetWidget) return;
    node.__sayaRatioFilterInstalled = true;

    node.__sayaAllPresets = [...(presetWidget.options?.values ?? [])];

    wrapCallback(aspectWidget, node);
    if (widthWidget) wrapCallback(widthWidget, node);
    if (heightWidget) wrapCallback(heightWidget, node);

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

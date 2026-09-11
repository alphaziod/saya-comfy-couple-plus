// @ts-expect-error ComfyUI injects this runtime module.
import { app } from "../../scripts/app.js";

// Filters a "resolution_preset" combo down to the presets matching
// "aspect_preset_when_not_image" (CUSTOM uses custom_aspect_width/
// custom_aspect_height instead). Both the aspect labels (e.g. "16:9 -
// Landscape") and the preset names (e.g. "Landscape 16:9 · 1344x768", see
// src/nodes/saya_resolution_scale.py) embed the same plain "A:B" ratio text,
// so filtering is a text match — no ratio table duplicated here.
//
// IMPORTANT: this does NOT match on node type/class. When
// SayaResolutionScaleCalculator sits inside a subgraph with its ratio
// widgets exposed as subgraph inputs, the interactive combo the user
// actually clicks lives on the subgraph's own proxy node in the parent
// graph — a node whose type is a per-workflow UUID, not
// "SayaResolutionScaleCalculator". There is no stable class name to hook
// via beforeRegisterNodeDef for that proxy. Matching by widget NAME instead
// (any node that happens to carry both widgets) works for the real node,
// the subgraph proxy, and any future node reusing these names alike.
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
    const aspectValue = String(findWidget(node, ASPECT_WIDGET)?.value ?? "");

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
    node.graph?.setDirtyCanvas?.(true, true);
    node.setDirtyCanvas?.(true, true);
}

function wrapCallback(widget, node) {
    // Marked per WIDGET OBJECT, not per node: if the host ever recreates the
    // widgets (e.g. loading a saved graph), the new widget objects are
    // unwrapped and get wrapped again here instead of leaving a dangling
    // wrap on the discarded old ones.
    if (widget.__sayaRatioFilterWrapped) return;
    widget.__sayaRatioFilterWrapped = true;
    const previousCallback = widget.callback;
    widget.callback = function (...args) {
        const result = previousCallback?.apply(this, args);
        applyFilter(node);
        return result;
    };
}

function installFilter(node) {
    const aspectWidget = findWidget(node, ASPECT_WIDGET);
    const presetWidget = findWidget(node, PRESET_WIDGET);
    if (!aspectWidget || !presetWidget) return;

    // Re-run on every call (nodeCreated, onConfigure, loadedGraphNode):
    // a node is first created with schema DEFAULTS, before configure()
    // writes a loaded graph's actual saved widget values, so the filter
    // must be re-applied once the real values are in place.
    if (node.__sayaAllPresets === undefined) {
        node.__sayaAllPresets = [...(presetWidget.options?.values ?? [])];
    }

    wrapCallback(aspectWidget, node);
    const widthWidget = findWidget(node, CUSTOM_WIDTH_WIDGET);
    const heightWidget = findWidget(node, CUSTOM_HEIGHT_WIDGET);
    if (widthWidget) wrapCallback(widthWidget, node);
    if (heightWidget) wrapCallback(heightWidget, node);

    applyFilter(node);
}

function watchConfigure(node) {
    if (node.__sayaConfigureWatched) return;
    node.__sayaConfigureWatched = true;
    const previousConfigure = node.onConfigure;
    node.onConfigure = function (...args) {
        const result = previousConfigure?.apply(this, args);
        installFilter(this);
        return result;
    };
}

app.registerExtension({
    name: "Saya.ResolutionRatioFilter",
    // Instance-level hooks (not beforeRegisterNodeDef/prototype patching):
    // works for ANY node exposing these widget names, regardless of its
    // type — required for the subgraph-proxy case described above, since
    // that type is a per-workflow UUID with no fixed class to register for.
    nodeCreated(node) {
        watchConfigure(node);
        installFilter(node);
    },
    loadedGraphNode(node) {
        watchConfigure(node);
        installFilter(node);
    },
});

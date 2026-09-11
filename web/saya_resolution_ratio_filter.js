// @ts-expect-error ComfyUI injects this runtime module.
import { app } from "../../scripts/app.js";

// Filters a "resolution_preset" combo down to the presets matching
// "aspect_preset_when_not_image" (CUSTOM uses custom_aspect_width/
// custom_aspect_height instead). Both the aspect labels (e.g. "16:9 -
// Landscape") and the preset names (e.g. "Landscape 16:9 · 1344x768", see
// src/nodes/saya_resolution_scale.py) embed the same plain "A:B" ratio text,
// so filtering is a text match — no ratio table duplicated here.
//
// Two things that broke earlier attempts at this, both worked around below:
//
// 1. Node type: when SayaResolutionScaleCalculator sits inside a subgraph
//    with its ratio widgets exposed as subgraph inputs (as in the real
//    "Ilust simple" workflow), the interactive combo the user actually
//    clicks lives on the subgraph's own proxy node in the parent graph — a
//    node whose type is a per-workflow UUID, not
//    "SayaResolutionScaleCalculator". There is no stable class name to hook
//    via beforeRegisterNodeDef for that proxy, so this matches by widget
//    NAME instead of node type — works for the real node, the subgraph
//    proxy, or any future node reusing these widget names alike.
//
// 2. Lifecycle timing: this ComfyUI build can render node widgets through a
//    Vue-based path (Comfy.VueNodes.Enabled) instead of the classic
//    canvas/litegraph one. Pre-computing a filtered array and writing it
//    into resolution_preset's options at some lifecycle hook (nodeCreated /
//    onConfigure / loadedGraphNode) is a race: a loaded node's real saved
//    values land at a different point than schema defaults, and it was
//    never possible to reliably tell "the real values are in now, filter
//    again" apart from "still just the defaults". Both the Vue widget
//    (WidgetSelectDefault) and classic litegraph combos explicitly support
//    options.values being a FUNCTION, calling it fresh on every read. Using
//    that instead of a static array means the filter is always correct at
//    the moment the dropdown opens, with nothing to race.
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

function installFilter(node) {
    const aspectWidget = findWidget(node, ASPECT_WIDGET);
    const presetWidget = findWidget(node, PRESET_WIDGET);
    if (!aspectWidget || !presetWidget) return;

    // Guard on the WIDGET OBJECT, not the node: if the host ever recreates
    // widgets (e.g. loading a saved graph, or a Vue node remount), the new
    // widget object is unwrapped and gets a fresh capture here instead of
    // silently keeping a dangling reference to the discarded old one.
    if (presetWidget.__sayaRatioFilterInstalled) return;
    presetWidget.__sayaRatioFilterInstalled = true;

    const currentValues = presetWidget.options?.values;
    const allPresets = Array.isArray(currentValues) ? [...currentValues] : [];
    if (!allPresets.length) return;

    // Read live on every access (dropdown open, Vue re-render, whatever) —
    // never stale, nothing to re-trigger on aspect/custom-width/height
    // change.
    presetWidget.options.values = function () {
        const target = targetRatioToken(node);
        if (target === null) return allPresets;
        const filtered = allPresets.filter((name) => name.match(RATIO_PATTERN)?.[0] === target);
        return filtered.length ? filtered : allPresets;
    };

    // Nicety, not required for filtering to work: snap the current value to
    // one that matches as soon as the user changes the ratio, instead of
    // leaving a now-mismatched preset selected until they open the dropdown.
    for (const widget of [aspectWidget, findWidget(node, CUSTOM_WIDTH_WIDGET), findWidget(node, CUSTOM_HEIGHT_WIDGET)]) {
        if (!widget || widget.__sayaRatioFilterWrapped) continue;
        widget.__sayaRatioFilterWrapped = true;
        const previousCallback = widget.callback;
        widget.callback = function (...args) {
            const result = previousCallback?.apply(this, args);
            const options = typeof presetWidget.options.values === "function"
                ? presetWidget.options.values()
                : presetWidget.options.values;
            if (!options.includes(presetWidget.value)) {
                presetWidget.value = options[0];
                presetWidget.callback?.(presetWidget.value);
            }
            node.graph?.setDirtyCanvas?.(true, true);
            node.setDirtyCanvas?.(true, true);
            return result;
        };
    }
}

app.registerExtension({
    name: "Saya.ResolutionRatioFilter",
    // Instance-level hook, not beforeRegisterNodeDef/prototype patching:
    // works for ANY node exposing these widget names, regardless of its
    // type — required for the subgraph-proxy case described above, since
    // that type is a per-workflow UUID with no fixed class to register for.
    nodeCreated(node) {
        installFilter(node);
    },
    loadedGraphNode(node) {
        installFilter(node);
    },
});

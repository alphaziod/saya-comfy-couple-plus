// @ts-expect-error ComfyUI injects this runtime module.
import { app } from "../../scripts/app.js";

// ComfyUI 1.45.x promoted/subgraph combo widgets keep their own option state.
// Changing the Python INPUT_TYPES or only wrapping node callbacks is therefore
// not enough: the host combo can be re-synchronised back to the source options.
// This extension treats the aspect selector as authoritative and reconciles the
// visible resolution combo itself.  It also rescans because promoted widgets can
// be created after nodeCreated/loadedGraphNode has already fired.

const ASPECT_WIDGET = "aspect_preset_when_not_image";
const PRESET_WIDGET = "resolution_preset";
const CUSTOM_WIDTH_WIDGET = "custom_aspect_width";
const CUSTOM_HEIGHT_WIDGET = "custom_aspect_height";
const CUSTOM_VALUE = "CUSTOM";
const ALL_VALUE = "All (no filter)";
const RATIO_PATTERN = /(\d+):(\d+)/;

// Deliberate fallback copy of the Python list.  Do not depend on the host combo
// to give us the full list: the exact bug we are fixing can expose only the stale
// 16:9 options on the promoted proxy widget.
const ALL_PRESETS = [
    "Square 1:1 · 768x768",
    "Square 1:1 · 1024x1024",
    "Square 1:1 · 1280x1280",
    "Landscape 4:3 · 1024x768",
    "Landscape 4:3 · 1280x960",
    "Landscape 4:3 · 1536x1152",
    "Portrait 3:4 · 768x1024",
    "Portrait 3:4 · 960x1280",
    "Portrait 3:4 · 1152x1536",
    "Landscape 3:2 · 960x640",
    "Landscape 3:2 · 1152x768",
    "Landscape 3:2 · 1216x832",
    "Landscape 3:2 · 1344x896",
    "Portrait 2:3 · 640x960",
    "Portrait 2:3 · 768x1152",
    "Portrait 2:3 · 832x1216",
    "Portrait 2:3 · 896x1344",
    "Landscape 16:9 · 896x512",
    "Landscape 16:9 · 1152x640",
    "Landscape 16:9 · 1344x768",
    "Landscape 16:9 · 1600x896",
    "Portrait 9:16 · 512x896",
    "Portrait 9:16 · 640x1152",
    "Portrait 9:16 · 768x1344",
    "Portrait 9:16 · 896x1600",
    "Ultrawide 21:9 · 1344x576",
    "Ultrawide 21:9 · 1600x704",
    "Ultrawide 21:9 · 1792x768",
    "Ultrawide Portrait 9:21 · 576x1344",
    "Ultrawide Portrait 9:21 · 704x1600",
    "Ultrawide Portrait 9:21 · 768x1792",
];

function findWidget(node, name) {
    return node?.widgets?.find((widget) => widget?.name === name);
}

function gcd(a, b) {
    a = Math.abs(Math.round(a));
    b = Math.abs(Math.round(b));
    while (b) [a, b] = [b, a % b];
    return a || 1;
}

function ratioToken(width, height) {
    const d = gcd(width, height);
    return `${Math.round(width) / d}:${Math.round(height) / d}`;
}

function targetRatioToken(node) {
    const aspectValue = String(findWidget(node, ASPECT_WIDGET)?.value ?? "");
    if (!aspectValue || aspectValue === ALL_VALUE) return null;

    if (aspectValue === CUSTOM_VALUE) {
        const width = Number(findWidget(node, CUSTOM_WIDTH_WIDGET)?.value ?? 0);
        const height = Number(findWidget(node, CUSTOM_HEIGHT_WIDGET)?.value ?? 0);
        return width > 0 && height > 0 ? ratioToken(width, height) : null;
    }

    const match = aspectValue.match(RATIO_PATTERN);
    return match ? `${Number(match[1])}:${Number(match[2])}` : null;
}

function presetRatioToken(name) {
    return String(name).split(" · ", 1)[0].match(RATIO_PATTERN)?.[0] ?? null;
}

function presetPixels(name) {
    const match = String(name).match(/(\d+)x(\d+)$/);
    return match ? Number(match[1]) * Number(match[2]) : 0;
}

function choicesFor(node) {
    const ratio = targetRatioToken(node);
    if (ratio === null) return ALL_PRESETS;
    const filtered = ALL_PRESETS.filter((name) => presetRatioToken(name) === ratio);
    return filtered.length ? filtered : ALL_PRESETS;
}

function sameValues(a, b) {
    if (!Array.isArray(a) || a.length !== b.length) return false;
    return a.every((value, index) => String(value) === b[index]);
}

function nearestChoice(current, choices) {
    if (choices.includes(String(current))) return String(current);
    const pixels = presetPixels(current);
    if (!pixels) return choices[0];
    return choices.reduce((best, candidate) =>
        Math.abs(presetPixels(candidate) - pixels) < Math.abs(presetPixels(best) - pixels)
            ? candidate
            : best
    , choices[0]);
}

function reconcile(node) {
    const aspectWidget = findWidget(node, ASPECT_WIDGET);
    const presetWidget = findWidget(node, PRESET_WIDGET);
    if (!aspectWidget || !presetWidget) return false;

    presetWidget.options ??= {};
    const choices = choicesFor(node);

    // Use a plain array.  Vue promoted widgets clone combo option objects, and
    // current ComfyUI explicitly syncs source combo options into promoted host
    // state; a function-valued options.values was not reliable here.
    const existing = presetWidget.options.values;
    if (!sameValues(existing, choices)) {
        presetWidget.options.values = [...choices];
    }

    const next = nearestChoice(presetWidget.value, choices);
    if (String(presetWidget.value ?? "") !== next) {
        presetWidget.value = next;
        presetWidget.callback?.(next);
        node.graph?.setDirtyCanvas?.(true, true);
        node.setDirtyCanvas?.(true, true);
    }
    return true;
}

function wrapWidget(widget, node) {
    if (!widget || widget.__sayaResolutionRatioWrapped) return;
    widget.__sayaResolutionRatioWrapped = true;
    const previous = widget.callback;
    widget.callback = function (...args) {
        const result = previous?.apply(this, args);
        queueMicrotask(() => reconcile(node));
        return result;
    };
}

function install(node) {
    const aspectWidget = findWidget(node, ASPECT_WIDGET);
    const presetWidget = findWidget(node, PRESET_WIDGET);
    if (!aspectWidget || !presetWidget) return false;

    wrapWidget(aspectWidget, node);
    wrapWidget(findWidget(node, CUSTOM_WIDTH_WIDGET), node);
    wrapWidget(findWidget(node, CUSTOM_HEIGHT_WIDGET), node);

    if (!node.__sayaResolutionRatioOnWidgetChangedWrapped) {
        node.__sayaResolutionRatioOnWidgetChangedWrapped = true;
        const previous = node.onWidgetChanged;
        node.onWidgetChanged = function (name, value, oldValue, widget) {
            const result = previous?.call(this, name, value, oldValue, widget);
            if (
                name === ASPECT_WIDGET ||
                name === CUSTOM_WIDTH_WIDGET ||
                name === CUSTOM_HEIGHT_WIDGET
            ) {
                queueMicrotask(() => reconcile(this));
            }
            return result;
        };
    }

    return reconcile(node);
}

function scanGraph(graph, seen = new Set()) {
    if (!graph || seen.has(graph)) return;
    seen.add(graph);

    const nodes = graph._nodes ?? graph.nodes ?? [];
    for (const node of nodes) {
        install(node);
        if (node?.subgraph) scanGraph(node.subgraph, seen);
    }
}

function scan() {
    try {
        if (app?.graph) scanGraph(app.graph);
    } catch (error) {
        console.warn("[Saya] resolution ratio sync failed", error);
    }
}

app.registerExtension({
    name: "Saya.ResolutionRatioFilter.V3",

    setup() {
        // Promoted subgraph widgets are not guaranteed to exist when the normal
        // creation hooks fire.  A lightweight reconciliation loop makes this
        // deterministic and also repairs any option overwrite done by ComfyUI.
        setInterval(scan, 200);
        document.addEventListener("pointerdown", scan, true);
        window.addEventListener("focus", scan);
        setTimeout(scan, 0);
        setTimeout(scan, 250);
    },

    nodeCreated(node) {
        install(node);
    },

    loadedGraphNode(node) {
        install(node);
    },
});

// @ts-expect-error ComfyUI injects this runtime module.
import { app } from "../../scripts/app.js";

// Filters resolution_preset down to the presets matching
// aspect_preset_when_not_image (CUSTOM uses custom_aspect_width/
// custom_aspect_height instead). Both the aspect labels (e.g. "16:9 -
// Landscape") and the preset names (e.g. "Landscape 16:9 · 1344x768", see
// src/nodes/saya_resolution_scale.py) embed the same plain "A:B" ratio text,
// so filtering is a text match — no ratio table duplicated here.
//
// ROOT CAUSE (traced against the installed comfyui_frontend_package):
//
// SayaResolutionScaleCalculator sits inside a subgraph ("02 · Resolution /
// T2I") with its ratio widgets exposed as subgraph inputs. The combo the
// user actually clicks is on the subgraph's own proxy node in the parent
// graph. That proxy does NOT keep its widgets in a plain, static array:
// `SubgraphNode.widgets` is a getter that projects each promoted input
// through `_projectPromotedWidget()`, which returns an object whose
// `value` / `options` / `callback` all read and write through a global
// Pinia store (the "widgetValue" store), keyed by
// `(rootGraphId, subgraphNodeId, widgetName)` — NOT the inner node's own
// `node.widgets`. That store entry only exists once ComfyUI has resolved
// the link from the subgraph's input to the inner widget and called
// `_setWidget()`, which fires a `widget-promoted` event on
// `subgraphNode.subgraph.events`.
//
// Earlier attempts failed because they either targeted the wrong node (the
// inner SayaResolutionScaleCalculator, whose widgets are link-driven and
// invisible once converted to subgraph inputs) or ran once at
// nodeCreated/loadedGraphNode — before that promotion had necessarily
// completed — with nothing to retry later. The fix: also listen for the
// `widget-promoted` event on the specific subgraph node's own event
// target. That is a targeted, one-shot-per-promotion signal (not a poll or
// a global listener), which is exactly the moment the projected widget
// object becomes available/refreshed.
//
// The projected widget's `options` getter returns the *live* store object
// by reference (not a copy), so mutating a property on it (e.g. `.values`)
// writes straight into the reactive store — no extra sync step needed.
const ASPECT_WIDGET = "aspect_preset_when_not_image";
const PRESET_WIDGET = "resolution_preset";
const CUSTOM_WIDTH_WIDGET = "custom_aspect_width";
const CUSTOM_HEIGHT_WIDGET = "custom_aspect_height";
const CUSTOM_VALUE = "CUSTOM";
const RATIO_PATTERN = /(\d+):(\d+)/;

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
    const divisor = gcd(width, height);
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
    return match ? `${Number(match[1])}:${Number(match[2])}` : null; // no match => "All (no filter)"
}

function presetRatioToken(name) {
    return String(name).split(" · ", 1)[0].match(RATIO_PATTERN)?.[0] ?? null;
}

function presetPixels(name) {
    const match = String(name).match(/(\d+)x(\d+)$/);
    return match ? Number(match[1]) * Number(match[2]) : 0;
}

// Picks the compatible preset closest in pixel count to the one that was
// selected before, instead of always resetting to the first option — this
// keeps whatever resolution "tier" the user had (fast/balanced/high) when
// they simply change the aspect ratio.
function nearestCompatible(previousName, choices) {
    const previousPixels = presetPixels(previousName);
    if (!previousPixels) return choices[0];
    return choices.reduce((best, candidate) =>
        Math.abs(presetPixels(candidate) - previousPixels) < Math.abs(presetPixels(best) - previousPixels)
            ? candidate
            : best
    , choices[0]);
}

function install(node) {
    const aspectWidget = findWidget(node, ASPECT_WIDGET);
    const presetWidget = findWidget(node, PRESET_WIDGET);
    if (!aspectWidget || !presetWidget) return;

    // Guard on the WIDGET OBJECT (stable/cached per promoted input slot, or
    // per plain-node widget), not the node: a fresh widget object (e.g. a
    // widget re-promoted after a link change) gets a fresh capture instead
    // of silently keeping a dangling reference to a discarded old one.
    if (presetWidget.__sayaRatioFilterInstalled) return;

    const currentValues = presetWidget.options?.values;
    if (!Array.isArray(currentValues) || !currentValues.length) return; // not resolved yet; retry on widget-promoted
    presetWidget.__sayaRatioFilterInstalled = true;
    const allPresets = [...currentValues];

    // Read live on every access (dropdown open, whatever renderer is in
    // use) — both classic litegraph combos and this ComfyUI build's Vue
    // combo widget call options.values() when it is a function.
    presetWidget.options.values = function () {
        const target = targetRatioToken(node);
        if (target === null) return allPresets;
        const filtered = allPresets.filter((name) => presetRatioToken(name) === target);
        return filtered.length ? filtered : allPresets;
    };

    // Nicety, not required for filtering to work: snap the current value to
    // a matching one (closest existing tier) as soon as the ratio changes.
    for (const widget of [aspectWidget, findWidget(node, CUSTOM_WIDTH_WIDGET), findWidget(node, CUSTOM_HEIGHT_WIDGET)]) {
        if (!widget || widget.__sayaRatioFilterWrapped) continue;
        widget.__sayaRatioFilterWrapped = true;
        const previousCallback = widget.callback;
        widget.callback = function (...args) {
            const result = previousCallback?.apply(this, args);
            const options = presetWidget.options.values();
            if (!options.includes(presetWidget.value)) {
                const next = nearestCompatible(presetWidget.value, options);
                presetWidget.value = next;
                presetWidget.callback?.(next);
            }
            node.graph?.setDirtyCanvas?.(true, true);
            node.setDirtyCanvas?.(true, true);
            return result;
        };
    }
}

// Subgraph proxy widgets are only fully resolved (and their options/value
// available) once ComfyUI links the subgraph input to the inner widget and
// promotes it — which dispatches this event on the subgraph's own event
// target. Listening here (once per subgraph node instance) is what lets
// `install` retry exactly when the widget becomes available, instead of
// racing nodeCreated/loadedGraphNode timing or polling the graph.
function watchPromotion(node) {
    if (node.__sayaWidgetPromotedWatched) return;
    const events = node?.subgraph?.events;
    if (!events?.addEventListener) return;
    node.__sayaWidgetPromotedWatched = true;
    events.addEventListener("widget-promoted", () => install(node));
}

function attach(node) {
    install(node);
    watchPromotion(node);
}

app.registerExtension({
    name: "Saya.ResolutionRatioFilter",
    // Instance-level hooks, not beforeRegisterNodeDef/prototype patching:
    // matches by widget NAME so this works for the real
    // SayaResolutionScaleCalculator node and for a subgraph's dynamically
    // typed proxy node alike.
    nodeCreated(node) {
        attach(node);
    },
    loadedGraphNode(node) {
        attach(node);
    },
});

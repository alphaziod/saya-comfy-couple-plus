// @ts-expect-error ComfyUI injects this runtime module.
import { app } from "../../scripts/app.js";

// One-way sync: resolution_preset is the source of truth. Whenever it
// changes, derive its ratio family and write the matching option into
// aspect_preset_when_not_image. The reverse never happens here: this file
// does not filter resolution_preset's option list and does not write to
// resolution_preset for any reason.
//
// Preset names look like "Landscape 16:9 · 1344x768" (see
// src/nodes/saya_resolution_scale.py) and aspect options look like
// "16:9 - Landscape". Both embed the same "A:B" ratio and the same
// Landscape/Portrait/Square/Ultrawide family word(s), just in a different
// order/separator, so the aspect label is derived by reformatting the
// preset's own family text — no second ratio table to keep in sync.
//
// ROOT CAUSE / MECHANISM (traced against the installed
// comfyui_frontend_package): SayaResolutionScaleCalculator sits inside a
// subgraph with its ratio widgets exposed as subgraph inputs, so the combo
// the user actually clicks lives on the subgraph's own proxy node in the
// parent graph, not on the inner node. SubgraphNode.widgets is a getter
// that projects each promoted input through _projectPromotedWidget(),
// whose value/options/callback all read and write through a global
// "widgetValue" Pinia store, keyed by (rootGraphId, subgraphNodeId,
// widgetName) -- not the inner node's own widgets. That store entry only
// exists once ComfyUI resolves the subgraph-input link and calls
// _setWidget(), which fires a "widget-promoted" event on the subgraph's
// own event target. Listening for that event (once per node instance) is
// what lets this retry exactly when the widgets become available, instead
// of racing nodeCreated/loadedGraphNode timing or polling the graph.
const PRESET_WIDGET = "resolution_preset";
const ASPECT_WIDGET = "aspect_preset_when_not_image";
const NAME_AND_RATIO = /^(.*?)\s*(\d+:\d+)(?:\s|$)/;

function findWidget(node, name) {
    return node?.widgets?.find((widget) => widget?.name === name);
}

function optionsOf(widget) {
    const values = widget?.options?.values;
    if (typeof values === "function") return values();
    return Array.isArray(values) ? values : [];
}

// "Landscape 16:9 · 1344x768" -> "16:9 - Landscape"
// "Square 1:1 · 1024x1024" -> "1:1 - Square"
// "Ultrawide Portrait 9:21 · 576x1344" -> "9:21 - Ultrawide Portrait"
function deriveAspectLabel(presetName) {
    const family = String(presetName).split(" · ", 1)[0];
    const match = family.match(NAME_AND_RATIO);
    if (!match) return null;
    const [, kind, ratio] = match;
    return kind ? `${ratio} - ${kind}` : ratio;
}

// aspect_preset_when_not_image is now a derived, read-only display: nothing
// should hand-pick it any more once resolution_preset decides it. Disabling
// it (rather than removing it from INPUT_TYPES) keeps every saved
// workflow's positional widgets_values array intact -- it sits in the
// middle of that array, so removing the widget entirely would shift every
// later value (swap_aspect_when_not_image, custom_aspect_width/height,
// mode, custom_divisor) in every already-saved graph.
function disableAspectWidget(aspectWidget) {
    aspectWidget.disabled = true;
    aspectWidget.options ??= {};
    aspectWidget.options.disabled = true;
}

function syncAspectFromPreset(node) {
    const presetWidget = findWidget(node, PRESET_WIDGET);
    const aspectWidget = findWidget(node, ASPECT_WIDGET);
    if (!presetWidget || !aspectWidget) return;

    disableAspectWidget(aspectWidget);

    const derived = deriveAspectLabel(presetWidget.value);
    if (derived === null || derived === String(aspectWidget.value ?? "")) return;

    // Only ever set a value that is a real, existing option -- never
    // fabricate or force one the widget doesn't actually offer.
    if (!optionsOf(aspectWidget).includes(derived)) return;

    aspectWidget.value = derived;
    aspectWidget.callback?.(derived);
    node.graph?.setDirtyCanvas?.(true, true);
    node.setDirtyCanvas?.(true, true);
}

function install(node) {
    const presetWidget = findWidget(node, PRESET_WIDGET);
    const aspectWidget = findWidget(node, ASPECT_WIDGET);
    if (!presetWidget || !aspectWidget) return;
    if (presetWidget.__sayaAspectSyncInstalled) return;
    presetWidget.__sayaAspectSyncInstalled = true;

    const previousCallback = presetWidget.callback;
    presetWidget.callback = function (...args) {
        const result = previousCallback?.apply(this, args);
        syncAspectFromPreset(node);
        return result;
    };

    // Correct an already-mismatched pair as soon as the widgets are
    // available (covers a workflow loaded with a stale aspect value, e.g.
    // resolution_preset "Landscape 16:9 · 896x512" saved next to aspect
    // "4:3 - Landscape").
    syncAspectFromPreset(node);
}

// Subgraph proxy widgets are only fully resolved (value/options available)
// once ComfyUI links the subgraph input to the inner widget and promotes
// it -- see the mechanism note above. Listening here is what lets install()
// retry exactly when that happens, with no poll and no graph scan.
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

// @ts-expect-error ComfyUI injects this runtime module.
import { app } from "../../scripts/app.js";

// Saya nodes · progressive disclosure.
//
// A widget that CANNOT affect the render in the node's current state is
// hidden, so the panel only shows controls that are live.
//
// Compatibility rules:
// - widgets are HIDDEN by swapping their `type`, never removed and never
//   reordered, so the positional `widgets_values` of every saved workflow
//   stays valid;
// - matching is done by widget NAME, and a node whose expected widgets are
//   missing (older graph, partial subgraph promotion) makes the rule stand
//   down instead of half-collapsing;
// - every callback we attach chains the previous one;
// - `loadedGraphNode` and the subgraph "widget-promoted" event are covered.
//
// Why each rule exists is written next to it: a rule is only allowed here
// when the value provably cannot reach the render.

const HIDDEN = "hidden";

// Node property (serialized outside widgets_values, so it is positionally
// harmless) toggled from the node's right-click menu.
const ADVANCED_PROPERTY = "saya_show_advanced";

// ---------------------------------------------------------------------------
// Rule table
// ---------------------------------------------------------------------------
//
// rule = {
//   widgets: [{ name, equals: [...] }]   all must match (missing -> stand down)
//   inputs:  { all: [...], any: [...], none: [...] }   (missing -> stand down)
//   hide:    ["name", { name, onlyWhenDefault: <value> }]
// }
//
// `onlyWhenDefault` is a safety valve: a widget that still holds its
// declared default is noise, but the same widget holding a value someone
// deliberately typed stays visible even when it is inert. Nothing a user
// set is ever hidden from them.

const SPECS = [
    {
        id: "SayaDuoTiledUpscale",
        types: ["SayaDuoTiledUpscale"],
        // Distinctive enough not to collide with the other Saya nodes.
        signature: ["upscale_by", "mode_type", "seam_fix_mode", "tile_padding"],
        rules: [
            {
                // UltimateSDUpscale's processing.sample(): with BOTH a custom
                // sampler and custom sigmas it calls SamplerCustom, which never
                // receives steps / sampler_name / scheduler / denoise. The
                // node's own Python tooltip says the same ("Fallback only").
                inputs: { all: ["custom_sampler", "custom_sigmas"] },
                hide: ["steps", "sampler_name", "scheduler", "denoise"],
            },
            {
                // seams_fix.enabled = mode != NONE: nothing downstream reads
                // any seam-fix parameter while the mode is "None".
                widgets: [{ name: "seam_fix_mode", equals: ["None"] }],
                hide: [
                    "seam_fix_denoise",
                    "seam_fix_width",
                    "seam_fix_mask_blur",
                    "seam_fix_padding",
                ],
            },
            {
                // seams_fix_width (self.width) is read only inside
                // band_pass_process(); the half-tile passes never touch it.
                widgets: [{
                    name: "seam_fix_mode",
                    equals: ["Half Tile", "Half Tile + Intersections"],
                }],
                hide: ["seam_fix_width"],
            },
        ],
    },
    {
        id: "SayaDuoSegsDetail",
        types: ["SayaDuoSegsDetail"],
        signature: ["guide_size", "max_size", "force_inpaint", "cycle"],
        rules: [],
        // Thirteen of these nodes live in the workflow, eighteen widgets
        // each. These eight are Impact Pack plumbing that the duo passes
        // leave alone; they are folded away WHILE THEY HOLD THEIR DEFAULT
        // and revealed from the node menu (or by holding a real value).
        advanced: [
            { name: "guide_size_for", default: true },
            { name: "force_inpaint", default: true },
            { name: "wildcard", default: "" },
            { name: "cycle", default: 1 },
            { name: "inpaint_model", default: false },
            { name: "noise_mask_feather", default: 20 },
            { name: "tiled_encode", default: false },
            { name: "tiled_decode", default: false },
        ],
    },
    {
        id: "SayaHiDreamLoraSettings",
        types: ["SayaHiDreamLoraSettings"],
        signature: ["enabled", "lora_name", "strength_model", "trigger_text"],
        rules: [
            {
                // configure(): active = enabled and ...; a disabled node
                // emits enabled=False (the loader returns the MODEL
                // untouched) and an empty trigger string.
                widgets: [{ name: "enabled", equals: [false] }],
                hide: [
                    "lora_name",
                    "strength_model",
                    // Typed trigger text is content, not a parameter: it is
                    // only folded away while it is still empty.
                    { name: "trigger_text", onlyWhenDefault: "" },
                ],
            },
        ],
    },
    {
        id: "SayaDuoLatentShape",
        types: ["SayaDuoLatentShape"],
        signature: ["latent_downscale", "latent_channels"],
        rules: [
            {
                // latent_spec(): a connected VAE overrides the downscale —
                // but only when the VAE object exposes it, so the fallback
                // is hidden only while it still holds its declared default.
                inputs: { all: ["vae"] },
                hide: [{ name: "latent_downscale", onlyWhenDefault: 8 }],
            },
            {
                // Same for the channel count, which either a VAE or a MODEL
                // can supply.
                inputs: { any: ["vae", "model"] },
                hide: [{ name: "latent_channels", onlyWhenDefault: 4 }],
            },
        ],
    },
];

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

function findWidget(node, name) {
    return node?.widgets?.find((widget) => widget?.name === name);
}

function findInput(node, name) {
    return node?.inputs?.find((input) => input?.name === name);
}

/** true / false, or null when this node has no such input at all. */
function inputConnected(node, name) {
    const input = findInput(node, name);
    if (!input) return null;
    return input.link !== null && input.link !== undefined;
}

/** Tolerant equality: widget values come back from JSON loosely typed. */
function sameValue(left, right) {
    if (typeof left === "boolean" || typeof right === "boolean") {
        return Boolean(left) === Boolean(right);
    }
    if (typeof left === "number" || typeof right === "number") {
        return Number(left) === Number(right);
    }
    return String(left ?? "") === String(right ?? "");
}

function hideWidget(widget) {
    if (!widget || widget.type === HIDDEN) return;
    widget.__sayaOriginalType = widget.type;
    widget.type = HIDDEN;
}

function showWidget(widget) {
    if (!widget || widget.__sayaOriginalType === undefined) return;
    widget.type = widget.__sayaOriginalType;
    delete widget.__sayaOriginalType;
}

function hideEntryName(entry) {
    return typeof entry === "string" ? entry : entry?.name;
}

// ---------------------------------------------------------------------------
// Spec matching
// ---------------------------------------------------------------------------

function specForNode(node) {
    const type = node?.comfyClass ?? node?.type;
    for (const spec of SPECS) {
        if (spec.types.includes(type)) return spec;
    }
    // Subgraph proxies carry the promoted widgets but not the inner node's
    // type, exactly like the region-mode extension, so fall back to the
    // widget signature. SPECS order decides ties (most specific first).
    for (const spec of SPECS) {
        if (spec.signature.every((name) => findWidget(node, name) !== undefined)) {
            return spec;
        }
    }
    return null;
}

/** Every widget name this spec is allowed to touch. */
function managedNames(spec) {
    const names = new Set();
    for (const rule of spec.rules ?? []) {
        for (const entry of rule.hide ?? []) {
            const name = hideEntryName(entry);
            if (name) names.add(name);
        }
    }
    for (const entry of spec.advanced ?? []) {
        if (entry?.name) names.add(entry.name);
    }
    return names;
}

function ruleMatches(node, rule) {
    for (const condition of rule.widgets ?? []) {
        const widget = findWidget(node, condition.name);
        if (!widget) return false;  // unknown state: stand down.
        if (!condition.equals.some((value) => sameValue(widget.value, value))) {
            return false;
        }
    }
    const inputs = rule.inputs;
    if (inputs) {
        for (const name of inputs.all ?? []) {
            if (inputConnected(node, name) !== true) return false;
        }
        for (const name of inputs.none ?? []) {
            if (inputConnected(node, name) !== false) return false;
        }
        const any = inputs.any ?? [];
        if (any.length) {
            const states = any.map((name) => inputConnected(node, name));
            // A single unknown socket means the node is not the layout this
            // rule was written against.
            if (states.some((state) => state === null)) return false;
            if (!states.some((state) => state === true)) return false;
        }
    }
    return true;
}

function hiddenNames(node, spec) {
    const hidden = new Set();
    for (const rule of spec.rules ?? []) {
        if (!ruleMatches(node, rule)) continue;
        for (const entry of rule.hide ?? []) {
            const name = hideEntryName(entry);
            const widget = findWidget(node, name);
            if (!widget) continue;
            if (typeof entry === "object" && "onlyWhenDefault" in entry
                && !sameValue(widget.value, entry.onlyWhenDefault)) {
                continue;  // someone typed a real value here: keep it visible.
            }
            hidden.add(name);
        }
    }
    const advanced = spec.advanced ?? [];
    if (advanced.length && node?.properties?.[ADVANCED_PROPERTY] !== true) {
        for (const entry of advanced) {
            const widget = findWidget(node, entry.name);
            if (!widget) continue;
            if (!sameValue(widget.value, entry.default)) continue;
            hidden.add(entry.name);
        }
    }
    return hidden;
}

function applyVisibility(node, spec) {
    if (!node || !spec || !Array.isArray(node.widgets)) return;
    const managed = managedNames(spec);
    const hidden = hiddenNames(node, spec);
    for (const widget of node.widgets) {
        const name = widget?.name;
        if (!name || !managed.has(name)) continue;
        if (hidden.has(name)) hideWidget(widget);
        else showWidget(widget);
    }
    node?.setDirtyCanvas?.(true, true);
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

/** Widget names whose value can change what is visible. */
function watchedNames(spec) {
    const names = managedNames(spec);
    for (const rule of spec.rules ?? []) {
        for (const condition of rule.widgets ?? []) names.add(condition.name);
    }
    return names;
}

/** Input names whose connection state (not value) can change what is visible. */
function watchedInputNames(spec) {
    const names = new Set();
    for (const rule of spec.rules ?? []) {
        for (const name of rule.inputs?.all ?? []) names.add(name);
        for (const name of rule.inputs?.any ?? []) names.add(name);
        for (const name of rule.inputs?.none ?? []) names.add(name);
    }
    return names;
}

// ---------------------------------------------------------------------------
// Draw-hook refresh (promoted-widget fallback)
// ---------------------------------------------------------------------------
//
// Visibility is normally kept in sync by wrapping the controller widget's own
// callback (chainWidgetCallbacks) and the node's onConnectionsChange. On a
// subgraph, a widget PROMOTED to a subgraph input can have its value changed
// (via the subgraph's own boundary widget) WITHOUT the inner widget's
// callback ever running — the promotion mechanism writes .value directly.
// Wrapping the callback is then a no-op safety net that never fires.
//
// The draw hook below is a cheap, callback-independent fallback: every time
// the node is about to be painted, re-check a small signature of whatever
// currently drives visibility (controller widget values + watched input
// connection state) and only re-run applyVisibility when that signature
// actually changed since the last paint. A node is redrawn constantly while
// on screen, so this catches a silent promoted-widget change within one
// frame without doing any real work on the (overwhelmingly common) frames
// where nothing changed.
function computeVisibilitySignature(node, spec) {
    const parts = [];
    for (const name of watchedNames(spec)) {
        const widget = findWidget(node, name);
        parts.push(`${name}=${widget ? String(widget.value) : "\u0000"}`);
    }
    for (const name of watchedInputNames(spec)) {
        const connected = inputConnected(node, name);
        parts.push(`${name}:${connected === null ? "?" : connected ? "1" : "0"}`);
    }
    parts.push(`adv=${node?.properties?.[ADVANCED_PROPERTY] === true}`);
    return parts.join("|");
}

function refreshIfSignatureChanged(node, spec) {
    const signature = computeVisibilitySignature(node, spec);
    if (signature === node.__sayaNodeUxSignature) return;
    node.__sayaNodeUxSignature = signature;
    applyVisibility(node, spec);
}

function chainDrawRefresh(node, spec) {
    if (node.__sayaNodeUxDrawWatched) return;
    node.__sayaNodeUxDrawWatched = true;
    const previous = node.onDrawForeground;
    node.onDrawForeground = function (...args) {
        const result = previous?.apply(this, args);
        // Runs every frame: a throw here would break canvas rendering, so a
        // UX refresh failure is logged once per node and never propagated.
        try {
            refreshIfSignatureChanged(node, spec);
        } catch (error) {
            if (!node.__sayaNodeUxDrawError) {
                node.__sayaNodeUxDrawError = true;
                console.error("[Saya node UX] visibility refresh failed", error);
            }
        }
        return result;
    };
}

function chainWidgetCallbacks(node, spec) {
    for (const name of watchedNames(spec)) {
        const widget = findWidget(node, name);
        if (!widget || widget.__sayaNodeUxWatched) continue;
        widget.__sayaNodeUxWatched = true;
        const previousCallback = widget.callback;
        widget.callback = function (...args) {
            const result = previousCallback?.apply(this, args);
            applyVisibility(node, spec);
            return result;
        };
    }
}

function chainConnectionsChange(node, spec) {
    if (node.__sayaNodeUxConnections) return;
    node.__sayaNodeUxConnections = true;
    const previous = node.onConnectionsChange;
    node.onConnectionsChange = function (...args) {
        const result = previous?.apply(this, args);
        applyVisibility(node, spec);
        return result;
    };
}

function chainMenu(node, spec) {
    if (!(spec.advanced ?? []).length || node.__sayaNodeUxMenu) return;
    node.__sayaNodeUxMenu = true;
    const previous = node.getExtraMenuOptions;
    node.getExtraMenuOptions = function (canvas, options) {
        const result = previous?.apply(this, arguments);
        if (Array.isArray(options)) {
            const shown = node.properties?.[ADVANCED_PROPERTY] === true;
            options.push({
                content: shown
                    ? "Saya: hide advanced widgets"
                    : "Saya: show advanced widgets",
                callback: () => {
                    if (!node.properties) node.properties = {};
                    node.properties[ADVANCED_PROPERTY] = !shown;
                    applyVisibility(node, spec);
                },
            });
        }
        return result;
    };
}

function install(node) {
    if (!node || !Array.isArray(node.widgets)) return;
    const spec = specForNode(node);
    if (!spec) return;
    // Nothing of this spec present yet (promotion pending): retry later.
    if (!spec.signature.every((name) => findWidget(node, name) !== undefined)) return;
    chainWidgetCallbacks(node, spec);
    chainConnectionsChange(node, spec);
    chainMenu(node, spec);
    chainDrawRefresh(node, spec);
    applyVisibility(node, spec);
    // Baseline for the draw hook: the paint immediately after install must
    // NOT redundantly re-run applyVisibility (it was just run on the line
    // above), only a REAL change from here on should trigger it again.
    node.__sayaNodeUxSignature = computeVisibilitySignature(node, spec);
}

// Subgraph proxy widgets only resolve once ComfyUI promotes them; retry on
// the "widget-promoted" event.
function watchPromotion(node) {
    if (!node || node.__sayaNodeUxPromotedWatched) return;
    const events = node?.subgraph?.events;
    if (!events?.addEventListener) return;
    node.__sayaNodeUxPromotedWatched = true;
    events.addEventListener("widget-promoted", () => install(node));
}

app.registerExtension({
    name: "Saya.NodeUx",
    nodeCreated(node) {
        install(node);
        watchPromotion(node);
    },
    loadedGraphNode(node) {
        install(node);
        watchPromotion(node);
    },
});

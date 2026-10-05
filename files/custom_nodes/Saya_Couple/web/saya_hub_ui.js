// @ts-expect-error ComfyUI injects this runtime module.
import { app } from "../../scripts/app.js";

// Main-workflow hub presentation only.
//
// This extension deliberately does NOT touch links, widget values, node modes,
// node positions, execution order, or subgraph internals. It only normalizes
// labels, minimum dimensions and colors for the four user-facing routing hubs
// introduced by the Couple/Model routing workflow.

const MODEL_1 = "Model 1";
const MODEL_2 = "Model 2";

const COLORS = {
    routing: { color: "#303650", bgcolor: "#485176" },
    couple: { color: "#4a2f46", bgcolor: "#68435f" },
};

const DETAILER_LABELS = [
    "Body",
    "Head & Hair",
    "Face",
    "Full Eyes",
    "Eyes · One by One",
    "Breasts",
    "Hands",
    "Feet",
    "Buttocks",
    "Anus",
    "Vulva",
    "Penis",
    "NSFW",
];

const USDU_LABELS = [
    "USDU 1",
    "USDU 2",
    "Hires Fix 1",
    "Hires Fix 3",
];

function titleOf(node) {
    return String(node?.title ?? node?.getTitle?.() ?? "").trim();
}

function typeOf(node) {
    return String(node?.comfyClass ?? node?.type ?? "").trim();
}

function namesOfWidgets(node) {
    return (node?.widgets ?? []).map((widget) => String(widget?.name ?? ""));
}

function hasAllWidgets(node, names) {
    const present = new Set(namesOfWidgets(node));
    return names.every((name) => present.has(name));
}

function valuesContainModelPair(widget) {
    const values = widget?.options?.values;
    if (!Array.isArray(values)) return false;
    return values.includes(MODEL_1) && values.includes(MODEL_2);
}

function modelWidgets(node) {
    return (node?.widgets ?? []).filter((widget) =>
        widget && (valuesContainModelPair(widget) || widget.value === MODEL_1 || widget.value === MODEL_2)
    );
}

function findWidget(node, names) {
    const wanted = new Set(Array.isArray(names) ? names : [names]);
    return (node?.widgets ?? []).find((widget) => wanted.has(widget?.name));
}

function setMinSize(node, width, height) {
    const current = Array.isArray(node?.size) || ArrayBuffer.isView(node?.size)
        ? [Number(node.size[0]) || 0, Number(node.size[1]) || 0]
        : [0, 0];
    const next = [Math.max(current[0], width), Math.max(current[1], height)];
    if (next[0] === current[0] && next[1] === current[1]) return;
    if (typeof node?.setSize === "function") node.setSize(next);
    else node.size = next;
}

function setSocketLabel(socket, label) {
    if (!socket || !label) return;
    socket.label = label;
    socket.localized_name = label;
}

function setWidgetLabel(widget, label) {
    if (!widget || !label) return;
    // LiteGraph renders label/name differently depending on widget type and
    // ComfyUI version. Setting label is presentation-only; keep widget.name
    // untouched because name is the serialization / promotion contract.
    widget.label = label;
}

function applySocketLabels(node, mapping) {
    for (const socket of node?.inputs ?? []) {
        const label = mapping.inputs?.[socket?.name];
        if (label) setSocketLabel(socket, label);
    }
    for (const socket of node?.outputs ?? []) {
        const label = mapping.outputs?.[socket?.name];
        if (label) setSocketLabel(socket, label);
    }
}

function classify(node) {
    const title = titleOf(node).toLowerCase();
    const type = typeOf(node).toLowerCase();
    const widgetNames = namesOfWidgets(node);
    const models = modelWidgets(node);

    if (title.includes("hub detailers") || type.includes("model-hub-detailers")) {
        return "detailers";
    }
    if (title.includes("hub usdu") || type.includes("model-hub-usdu")) {
        return "usdu";
    }
    if (title.includes("hub phase 6") || title.includes("phase 6 · model")
        || type.includes("model-hub-phase6") || type.includes("model-hub-phase-6")) {
        return "phase6";
    }
    if (title.includes("couple mode") || type.includes("couple-mode")) {
        return "couple";
    }

    // Fallbacks for renamed subgraphs: only activate on strong widget
    // signatures so unrelated nodes are never restyled accidentally.
    if (models.length >= 13) return "detailers";
    if (models.length === 4 && widgetNames.some((name) => /usdu/i.test(name))) return "usdu";
    if (models.length === 1 && widgetNames.some((name) => /phase\s*6/i.test(name))) return "phase6";
    if (hasAllWidgets(node, ["Couple Mode"]) || hasAllWidgets(node, ["couple_mode"])) return "couple";
    return null;
}

function configureDetailers(node) {
    node.title = "Hub Detailers · Model Routing";
    node.color = COLORS.routing.color;
    node.bgcolor = COLORS.routing.bgcolor;
    setMinSize(node, 350, 470);

    const widgets = modelWidgets(node);
    DETAILER_LABELS.forEach((label, index) => setWidgetLabel(widgets[index], label));

    const outputMap = {};
    for (const label of DETAILER_LABELS) {
        outputMap[`${label} Model`] = `${label} · MODEL`;
        outputMap[label] = `${label} · MODEL`;
    }
    applySocketLabels(node, {
        inputs: {
            "Model 1 Patched": "Model 1 · Patched",
            "Model 2 Patched": "Model 2 · Patched",
        },
        outputs: outputMap,
    });
}

function configureUsdu(node) {
    node.title = "Hub USDU · Model Routing";
    node.color = COLORS.routing.color;
    node.bgcolor = COLORS.routing.bgcolor;
    setMinSize(node, 350, 230);

    const widgets = modelWidgets(node);
    USDU_LABELS.forEach((label, index) => setWidgetLabel(widgets[index], `${label} · Model`));

    const outputMap = {};
    for (const label of USDU_LABELS) {
        outputMap[`${label} Model`] = `${label} · MODEL`;
        outputMap[label] = `${label} · MODEL`;
    }
    applySocketLabels(node, {
        inputs: {
            "Model 1 Patched": "Model 1 · Patched",
            "Model 2 Patched": "Model 2 · Patched",
        },
        outputs: outputMap,
    });
}

function configurePhase6(node) {
    node.title = "Hub Phase 6 · Model Routing";
    node.color = COLORS.routing.color;
    node.bgcolor = COLORS.routing.bgcolor;
    setMinSize(node, 350, 170);

    const widget = modelWidgets(node)[0];
    if (widget) setWidgetLabel(widget, "Phase 6 · Model");

    applySocketLabels(node, {
        inputs: {
            "Model 1": "Model 1",
            "Model 2": "Model 2",
            "Model 1 CLIP": "Model 1 · CLIP",
            "Model 2 CLIP": "Model 2 · CLIP",
            "Model 1 Identities": "Model 1 · Identities",
            "Model 2 Identities": "Model 2 · Identities",
            "Model 1 Identifier": "Model 1 · Identifier",
            "Model 2 Identifier": "Model 2 · Identifier",
        },
        outputs: {
            MODEL: "Selected MODEL",
            Model: "Selected MODEL",
            model: "Selected MODEL",
            CLIP: "Selected CLIP",
            Clip: "Selected CLIP",
            clip: "Selected CLIP",
            checkpoint_identities: "Selected Identities",
            "Checkpoint Identities": "Selected Identities",
            identifier: "Selected Identifier",
            "Model Identifier": "Selected Identifier",
        },
    });
}

function configureCoupleMode(node) {
    node.title = "Couple Mode · Couple / Solo";
    node.color = COLORS.couple.color;
    node.bgcolor = COLORS.couple.bgcolor;
    setMinSize(node, 300, 130);

    const widget = findWidget(node, ["Couple Mode", "couple_mode", "choice", "mode"])
        ?? (node?.widgets ?? []).find((candidate) => ["ON", "OFF"].includes(candidate?.value));
    if (widget) setWidgetLabel(widget, "Couple Mode");

    applySocketLabels(node, {
        outputs: {
            solo: "SOLO flag",
            Solo: "SOLO flag",
            BOOLEAN: "SOLO flag",
            boolean: "SOLO flag",
        },
    });
}

function configure(node) {
    const kind = classify(node);
    if (!kind) return;

    // Prevent repeated callback wrapping, while still allowing labels/sizes to
    // be reapplied after graph load or subgraph widget promotion.
    if (kind === "detailers") configureDetailers(node);
    else if (kind === "usdu") configureUsdu(node);
    else if (kind === "phase6") configurePhase6(node);
    else if (kind === "couple") configureCoupleMode(node);

    node?.setDirtyCanvas?.(true, true);
}

function watchPromotion(node) {
    if (!node || node.__sayaHubUiPromotionWatched) return;
    const events = node?.subgraph?.events;
    if (!events?.addEventListener) return;
    node.__sayaHubUiPromotionWatched = true;
    events.addEventListener("widget-promoted", () => configure(node));
}

app.registerExtension({
    name: "Saya.HubUi",
    nodeCreated(node) {
        configure(node);
        watchPromotion(node);
    },
    loadedGraphNode(node) {
        configure(node);
        watchPromotion(node);
    },
});

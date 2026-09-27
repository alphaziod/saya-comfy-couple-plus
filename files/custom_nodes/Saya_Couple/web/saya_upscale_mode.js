import { app } from "../../scripts/app.js";

// Old/foreign instances may carry a NaN or invalid upscale_by: normalise on load.
export function normalizeUpscaleBy(node) {
    if (node?.comfyClass !== "SayaUpscaleMode" && node?.type !== "SayaUpscaleMode") return;
    const widget = node.widgets?.find((item) => item?.name === "upscale_by");
    if (!widget) return;
    const value = Number(widget.value);
    if (!Number.isFinite(value) || value <= 0) widget.value = 1.0;
}

app.registerExtension({
    name: "Saya.UpscaleMode",
    loadedGraphNode(node) { normalizeUpscaleBy(node); },
    nodeCreated(node) { normalizeUpscaleBy(node); },
});

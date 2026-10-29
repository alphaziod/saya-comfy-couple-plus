// Ferme les workflows déjà ouverts pour qu'un batch ne laisse pas un onglet par image.
// L'image est déjà écrite. On garde l'onglet actif.
import { app } from "../../scripts/app.js";
import { api } from "../../scripts/api.js";

api.addEventListener("saya_close_workflow", async () => {
    const service = app.extensionManager?.workflow;
    if (!service?.closeWorkflow || !service.openWorkflows) {
        return;
    }
    const open = [...service.openWorkflows];
    if (open.length < 2) {
        return;
    }
    const keep = service.activeWorkflow;
    for (const workflow of open) {
        if (!workflow || workflow === keep) {
            continue;
        }
        try {
            await service.closeWorkflow(workflow, { warnIfUnsaved: false });
        } catch (error) {
            console.warn("saya close workflow", error);
        }
    }
});

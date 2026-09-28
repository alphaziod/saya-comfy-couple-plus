"""HTTP endpoints used by the image-phase controller frontend."""

from __future__ import annotations

import logging
from typing import Any

from ..services.image_phases import (
    discard_candidate,
    invalidate_phase,
    normalize_detailer,
    parse_phase,
    phase_status,
    promote_candidate,
)

LOGGER = logging.getLogger(__name__)


def register_image_phase_routes() -> None:
    """Register validation, redo, status, and emergency-unload endpoints."""
    from aiohttp import web
    from server import PromptServer

    routes = PromptServer.instance.routes

    @routes.post("/saya/image-phases/validate")
    async def validate_phase(request: Any) -> Any:
        try:
            payload = await request.json()
            phase = parse_phase(payload.get("phase"))
            detailer = normalize_detailer(payload.get("detailer", "none"))
            root = str(payload.get("checkpoint_root", "image/checkpoints"))
            transaction = str(payload.get("transaction_uuid") or "") or None
            manifest = promote_candidate(phase, detailer, root, transaction)
            return web.json_response({"ok": True, "manifest": manifest})
        except (ValueError, OSError) as error:
            LOGGER.exception("Image phase validation failed")
            return web.json_response({"ok": False, "error": str(error)}, status=400)

    @routes.post("/saya/image-phases/redo")
    async def redo_phase(request: Any) -> Any:
        try:
            payload = await request.json()
            phase = parse_phase(payload.get("phase"))
            root = str(payload.get("checkpoint_root", "image/checkpoints"))
            discarded = invalidate_phase(phase, root) if phase == 1 else discard_candidate(phase, root)
            return web.json_response({"ok": True, **discarded})
        except (ValueError, OSError) as error:
            return web.json_response({"ok": False, "error": str(error)}, status=400)

    @routes.post("/saya/image-phases/unload")
    async def unload_phase(request: Any) -> Any:
        del request
        return web.json_response({"ok": True, "unload": {"disabled": True, "reason": "automatic model unload removed"}})

    @routes.post("/saya/image-phases/status")
    async def status_phase(request: Any) -> Any:
        try:
            payload = await request.json()
            phase = parse_phase(payload.get("phase"))
            detailer = normalize_detailer(payload.get("detailer", "none"))
            root = str(payload.get("checkpoint_root", "image/checkpoints"))
            return web.json_response({"ok": True, **phase_status(phase, detailer, root)})
        except (ValueError, OSError) as error:
            return web.json_response({"ok": False, "error": str(error)}, status=400)

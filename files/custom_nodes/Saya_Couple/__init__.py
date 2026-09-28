"""Saya image custom nodes."""

import faulthandler
import signal

# Hang diagnostics: ``kill -USR1 <pid>`` dumps every thread's stack to the log (no effect otherwise).
if hasattr(signal, "SIGUSR1"):  # not available on Windows
    faulthandler.register(signal.SIGUSR1, all_threads=True)

from .registry import (
    NODE_CLASS_MAPPINGS,
    NODE_DISPLAY_NAME_MAPPINGS,
)

from .src.routes.image_phases import register_image_phase_routes
from .src.nodes.saya_dual_attention import CORE_PATCH_HINT, core_patch_missing

register_image_phase_routes()

_core_missing = core_patch_missing()
if _core_missing:
    import logging

    logging.getLogger(__name__).warning(
        "[Saya Couple] Couple mode is unavailable on this ComfyUI core (%s): %s. Solo is unaffected.",
        _core_missing, CORE_PATCH_HINT,
    )

WEB_DIRECTORY = "./web"

__all__ = [
    "NODE_CLASS_MAPPINGS",
    "NODE_DISPLAY_NAME_MAPPINGS",
    "WEB_DIRECTORY",
]

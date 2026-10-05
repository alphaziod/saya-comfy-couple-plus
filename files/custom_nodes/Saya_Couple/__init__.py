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

register_image_phase_routes()
# Since M1 (2026-10-05) Couple runs on a stock ComfyUI core (ModelPatcher object patches): nothing to check here.

WEB_DIRECTORY = "./web"

__all__ = [
    "NODE_CLASS_MAPPINGS",
    "NODE_DISPLAY_NAME_MAPPINGS",
    "WEB_DIRECTORY",
]

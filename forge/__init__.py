"""ComfyUI entry point for Saya Forge Couple 2.4.1 + USDU Identity Safe bridge."""

from .node import SayaComfyCoupleForge, SayaComfyCoupleForgeCopy
from .usdu_bridge import SayaUSDU1IdentitySafe, SayaUSDU2IdentitySafe

NODE_CLASS_MAPPINGS = {
    "SayaComfyCoupleForge": SayaComfyCoupleForge,
    "SayaComfyCoupleForgeCopy": SayaComfyCoupleForgeCopy,
    "SayaUSDU1IdentitySafe": SayaUSDU1IdentitySafe,
    "SayaUSDU2IdentitySafe": SayaUSDU2IdentitySafe,
}

NODE_DISPLAY_NAME_MAPPINGS = {
    "SayaComfyCoupleForge": "Saya Comfy Couple - Forge Coarse Main 2.3",
    "SayaComfyCoupleForgeCopy": "Saya Comfy Couple - Forge Coarse Main 2.3 - COPY",
    "SayaUSDU1IdentitySafe": "Saya Forge Couple · USDU 1 · Identity Safe V2.1",
    "SayaUSDU2IdentitySafe": "Saya Forge Couple · USDU 2 · Identity Safe V2.1",
}

__all__ = ["NODE_CLASS_MAPPINGS", "NODE_DISPLAY_NAME_MAPPINGS"]

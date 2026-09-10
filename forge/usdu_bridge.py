"""
Saya Forge Couple -> UltimateSDUpscale V2.1 bridge.

These nodes are aliases/wrappers only. They deliberately do NOT duplicate
UltimateSDUpscale or the Identity Safe algorithm. The actual implementation
remains in the installed ComfyUI_UltimateSDUpscale plugin.

Reason:
- USDU owns tile geometry / sampling / compositing.
- SayaForgeCouple owns P1/P2/MAIN attention routing.
- Duplicating the USDU implementation here would create two diverging copies.

Requirements:
- UltimateSDUpscaleCustomSample must be installed.
- The installed class must expose the V2.1 `structure_preservation` optional input.
"""

from __future__ import annotations

import copy


_BASE_NODE_NAME = "UltimateSDUpscaleCustomSample"


def _resolve_base():
    # Resolve lazily, after ComfyUI has loaded all custom-node mappings.
    import nodes as comfy_nodes

    base = comfy_nodes.NODE_CLASS_MAPPINGS.get(_BASE_NODE_NAME)
    if base is None:
        raise RuntimeError(
            "Saya USDU bridge: UltimateSDUpscaleCustomSample is not loaded. "
            "Install/enable ComfyUI_UltimateSDUpscale first."
        )

    schema = base.INPUT_TYPES()
    optional = schema.get("optional", {})
    if "structure_preservation" not in optional:
        raise RuntimeError(
            "Saya USDU bridge: the loaded UltimateSDUpscaleCustomSample does not "
            "contain structure_preservation. Install the audited Identity Safe V2.1 patch."
        )
    return base


class _SayaUSDUIdentitySafeBridge:
    RETURN_TYPES = ("IMAGE",)
    FUNCTION = "upscale"
    CATEGORY = "Saya/Forge Couple/USDU"

    @classmethod
    def INPUT_TYPES(cls):
        base = _resolve_base()
        schema = copy.deepcopy(base.INPUT_TYPES())

        # New Saya aliases default to the tested starting value.
        spec = schema["optional"]["structure_preservation"]
        typ, opts = spec
        opts = dict(opts)
        opts["default"] = 0.75
        opts["tooltip"] = (
            "Saya Identity Safe V2.1 structure lock. "
            "0 = exact upstream behavior, 0.75 = recommended first real-image test."
        )
        schema["optional"]["structure_preservation"] = (typ, opts)
        return schema

    def upscale(self, **kwargs):
        base = _resolve_base()
        return base().upscale(**kwargs)


class SayaUSDU1IdentitySafe(_SayaUSDUIdentitySafeBridge):
    DESCRIPTION = (
        "Saya Forge Couple bridge to audited UltimateSDUpscale Identity Safe V2.1. "
        "USDU pass 1 label only; execution is delegated to the installed USDU implementation."
    )


class SayaUSDU2IdentitySafe(_SayaUSDUIdentitySafeBridge):
    DESCRIPTION = (
        "Saya Forge Couple bridge to audited UltimateSDUpscale Identity Safe V2.1. "
        "USDU pass 2 label only; execution is delegated to the installed USDU implementation."
    )

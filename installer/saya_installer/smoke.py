"""Smoke tests, run with ComfyUI's own Python:  <python> smoke.py <comfyui_root>

Prints one JSON line. Never writes to the ComfyUI install. 2.0: the engine lives in the pack and is injected on a
STOCK block with the same object patch the installer relies on.
"""

import json
import os
import sys

root = os.path.abspath(sys.argv[1])
sys.path.insert(0, root)
os.chdir(root)
sys.dont_write_bytecode = True
os.environ["SAYA_COMFYUI_ROOT"] = root
os.environ["SAYA_COMFY_ROOT"] = root
out = {"core_import": False, "saya_symbols": False, "gain": None, "dual_path_live": False, "pack_import": False, "errors": []}

try:
    import comfy.ldm.modules.attention as A
    out["core_import"] = True
    out["core_stock"] = not hasattr(A, "saya_dual_attn2")  # a 1.x core patch would still define it
    sys.path.insert(0, os.path.join(root, "custom_nodes", "Saya_Couple", "tests"))
    import harness
    pack = harness.load_pack()
    out["pack_import"] = "SayaMultiCouple" in pack.NODE_CLASS_MAPPINGS
    out["pack_nodes"] = len(pack.NODE_CLASS_MAPPINGS)
    from saya_couple.src.engine import dual_attention as E
    from saya_couple.src.nodes import saya_dual_attention as D
    out["saya_symbols"] = all(hasattr(E, n) for n in ("saya_regional_fusion", "saya_dual_attn2", "saya_dual_check_hooks", "SAYA_FUSION_MODES"))
    out["gain"] = getattr(E, "SAYA_LOCKED_DELTA_PERSON_GAIN", None)
    out["fusion_modes"] = sorted(getattr(E, "SAYA_FUSION_MODES", {}))

    import torch
    from torch import nn
    import comfy.ops
    torch.manual_seed(0)
    block = A.BasicTransformerBlock(16, 2, 8, context_dim=12, operations=comfy.ops.disable_weight_init)
    for p in block.parameters():
        nn.init.normal_(p, std=0.3)
    wrapped = D.saya_block(block)  # what enable_dual_attention installs through ModelPatcher.add_object_patch
    g = torch.Generator().manual_seed(1)
    x, ctx = torch.randn(2, 16, 16, generator=g), torch.randn(2, 5, 12, generator=g)
    base = {"cond_or_uncond": [1, 0], "activations_shape": [2, 16, 4, 4]}
    dual = dict(base, saya_dual={
        "p1": torch.randn(1, 7, 12, generator=g), "p2": torch.randn(1, 3, 12, generator=g),
        "mask_1": torch.cat([torch.ones(4, 2), torch.zeros(4, 2)], 1), "mask_2": torch.cat([torch.zeros(4, 2), torch.ones(4, 2)], 1),
        "fusion_mode": "main_only", "params": {}})
    ref = block(x, ctx, dict(base))
    same = torch.equal(wrapped(x, ctx, dict(base)), ref) and torch.equal(wrapped(x, ctx, dict(dual)), ref)  # no payload / main_only == native
    dual["saya_dual"]["fusion_mode"] = "main_locked_delta"
    moved = not torch.equal(wrapped(x, ctx, dict(dual)), ref)                                                 # persons really act
    out["dual_path_live"] = bool(same and moved)
except Exception as e:  # noqa: BLE001
    out["errors"].append(f"{type(e).__name__}: {e}")

print(json.dumps(out))

"""Smoke tests, run with ComfyUI's own Python:  <python> smoke.py <comfyui_root>

Prints one JSON line. Never writes to the ComfyUI install.
"""

import json
import os
import sys

root = os.path.abspath(sys.argv[1])
sys.path.insert(0, root)
os.chdir(root)
sys.dont_write_bytecode = True
out = {"core_import": False, "saya_symbols": False, "gain": None, "dual_path_live": False, "pack_import": False, "errors": []}

try:
    import comfy.ldm.modules.attention as A
    out["core_import"] = True
    out["saya_symbols"] = all(hasattr(A, n) for n in ("saya_regional_fusion", "saya_dual_attn2", "saya_dual_check_hooks", "SAYA_FUSION_MODES"))
    out["gain"] = getattr(A, "SAYA_LOCKED_DELTA_PERSON_GAIN", None)
    out["fusion_modes"] = sorted(getattr(A, "SAYA_FUSION_MODES", {}))

    import torch
    from torch import nn
    import comfy.ops
    torch.manual_seed(0)
    block = A.BasicTransformerBlock(16, 2, 8, context_dim=12, operations=comfy.ops.disable_weight_init)
    for p in block.parameters():
        nn.init.normal_(p, std=0.3)
    g = torch.Generator().manual_seed(1)
    x, ctx = torch.randn(2, 16, 16, generator=g), torch.randn(2, 5, 12, generator=g)
    base = {"cond_or_uncond": [1, 0], "activations_shape": [2, 16, 4, 4]}
    dual = dict(base, saya_dual_mode=True, saya_dual={
        "p1": torch.randn(1, 7, 12, generator=g), "p2": torch.randn(1, 3, 12, generator=g),
        "mask_1": torch.cat([torch.ones(4, 2), torch.zeros(4, 2)], 1), "mask_2": torch.cat([torch.zeros(4, 2), torch.ones(4, 2)], 1),
        "fusion_mode": "main_only", "params": {}})
    ref = block(x, ctx, dict(base))
    same = torch.equal(block(x, ctx, dict(dual)), ref)                      # main_only == native (MAIN untouched)
    dual["saya_dual"]["fusion_mode"] = "main_locked_delta"
    moved = not torch.equal(block(x, ctx, dict(dual)), ref)                 # persons really act
    out["dual_path_live"] = bool(same and moved)
except Exception as e:  # noqa: BLE001
    out["errors"].append(f"core: {type(e).__name__}: {e}")

try:
    pack_tests = os.path.join(root, "custom_nodes", "Saya_Couple", "tests")
    sys.path.insert(0, pack_tests)
    os.environ["SAYA_COMFY_ROOT"] = root
    import harness
    pack = harness.load_pack()
    out["pack_import"] = "SayaMultiCouple" in pack.NODE_CLASS_MAPPINGS
    out["pack_nodes"] = len(pack.NODE_CLASS_MAPPINGS)
except Exception as e:  # noqa: BLE001
    out["errors"].append(f"pack: {type(e).__name__}: {e}")

print(json.dumps(out))

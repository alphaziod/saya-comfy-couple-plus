"""Proves what the PERSON gain does, on the installed ComfyUI (CPU, a few seconds, no model needed).

    <ComfyUI python> tests/proof_gain.py --comfyui /path/to/ComfyUI

1. g = 1.0 is bit-identical to the pre-gain main_locked_delta formula (OUT = M + D - proj(D), per person).
2. Any g scales ONLY the locked deltas: OUT - MAIN == g * (D1_locked + D2_locked).
3. MAIN and the P1/P2 attention results reaching the fusion do not depend on g.
4. Unconditional rows stay exactly MAIN; the delta stays orthogonal to MAIN.
5. With the Saya flag absent, the block is bit-identical to the Saya 'main_only' path (MAIN = native).
"""

import argparse
import os
import sys

ap = argparse.ArgumentParser()
ap.add_argument("--comfyui", required=True)
a = ap.parse_args()
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.abspath(a.comfyui))
os.chdir(a.comfyui)

import torch  # noqa: E402
from torch import nn  # noqa: E402
import comfy.ops  # noqa: E402
import comfy.ldm.modules.attention as A  # noqa: E402

ok = True


def check(cond, label):
    global ok
    ok &= bool(cond)
    print(("PASS " if cond else "FAIL ") + label)


def pre_gain_fusion(main_out, p1, p2, m1, m2):
    main = main_out.float()
    denom = (main * main).sum(dim=-1, keepdim=True).clamp_min(A.SAYA_LOCKED_DELTA_EPS)
    out = main
    for p, m in ((p1, m1), (p2, m2)):
        delta = m.float() * (p.float() - main)
        out = out + delta - (delta * main).sum(dim=-1, keepdim=True) / denom * main
    return out.to(main_out.dtype)


fusion = A.SAYA_FUSION_MODES["main_locked_delta"][0]
file_gain = A.SAYA_LOCKED_DELTA_PERSON_GAIN
print(f"SAYA_LOCKED_DELTA_PERSON_GAIN in the installed file = {file_gain}")
g = torch.Generator().manual_seed(0)
try:
    A.SAYA_LOCKED_DELTA_PERSON_GAIN = 1.0
    for dt in (torch.float32, torch.bfloat16, torch.float16):
        m, p1, p2 = [torch.randn(4, 1536, 640, generator=g).to(dt) for _ in range(3)]
        m1 = (torch.rand(4, 1536, 1, generator=g) > .5).to(dt); m2 = 1 - m1
        m1[:2] = 0; m2[:2] = 0
        check(torch.equal(fusion(m, p1, p2, m1, m2), pre_gain_fusion(m, p1, p2, m1, m2)), f"1. g=1.0 bit-identical to the pre-gain formula ({dt})")
    m, p1, p2 = [torch.randn(2, 64, 32, generator=g) for _ in range(3)]
    m1 = torch.zeros(2, 64, 1); m1[1, :32] = 1; m2 = torch.zeros(2, 64, 1); m2[1, 32:] = 1
    ref = fusion(m, p1, p2, m1, m2)
    for gain in (file_gain, 0.5, 0.78):
        A.SAYA_LOCKED_DELTA_PERSON_GAIN = gain
        out = fusion(m, p1, p2, m1, m2)
        err = float((out - m - gain * (ref - m)).abs().max())
        check(err < 1e-5, f"2. g={gain}: OUT - MAIN == g * locked deltas (max err {err:.1e})")
        check(torch.equal(out[0], m[0]), f"4. g={gain}: unconditional row is exactly MAIN")
        cos = ((out[1] - m[1]) * m[1]).sum(-1) / (m[1].norm(dim=-1) * (out[1] - m[1]).norm(dim=-1)).clamp_min(1e-12)
        check(float(cos.abs().max()) < 1e-4, f"4. g={gain}: delta orthogonal to MAIN (|cos| max {float(cos.abs().max()):.1e})")

    torch.manual_seed(0)
    block = A.BasicTransformerBlock(64, 4, 16, context_dim=32, operations=comfy.ops.disable_weight_init)
    for prm in block.parameters():
        nn.init.normal_(prm, std=.3)
    x, ctx = torch.randn(2, 48, 64, generator=g), torch.randn(2, 7, 32, generator=g)
    seen = []
    real = A.SAYA_FUSION_MODES["main_locked_delta"]
    A.SAYA_FUSION_MODES["main_locked_delta"] = (lambda *t: (seen.append([v.clone() for v in t]), real[0](*t))[1], real[1])

    def opts(mode):
        gg = torch.Generator().manual_seed(7)
        return {"cond_or_uncond": [1, 0], "activations_shape": [2, 64, 8, 6], "saya_dual_mode": True,
                "saya_dual": {"p1": torch.randn(1, 9, 32, generator=gg), "p2": torch.randn(1, 5, 32, generator=gg),
                              "mask_1": torch.cat([torch.ones(8, 3), torch.zeros(8, 3)], 1), "mask_2": torch.cat([torch.zeros(8, 3), torch.ones(8, 3)], 1),
                              "fusion_mode": mode, "params": {}}}
    for gain in (1.0, 0.5):
        A.SAYA_LOCKED_DELTA_PERSON_GAIN = gain
        block(x, ctx, opts("main_locked_delta"))
    check(all(torch.equal(u, v) for u, v in zip(seen[0], seen[1])), "3. MAIN, P1, P2 and masks reaching the fusion are identical for g=1.0 and g=0.5")
    A.SAYA_FUSION_MODES["main_locked_delta"] = real
    base = {"cond_or_uncond": [1, 0], "activations_shape": [2, 64, 8, 6]}
    check(torch.equal(block(x, ctx, dict(base)), block(x, ctx, opts("main_only"))), "5. flag absent == Saya main_only path (MAIN is the native cross-attention)")
finally:
    A.SAYA_LOCKED_DELTA_PERSON_GAIN = file_gain
print("ALL PASS" if ok else "SOME CHECKS FAILED")
sys.exit(0 if ok else 1)

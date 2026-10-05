"""DIAGNOSTIC (not part of the suite, heavy, CPU, real SDXL weights).

Where does the PERSON delta of the Saya attn2 branch stop being local, and where is
"MAIN stays the authority" no longer guaranteed?  Forward hooks only: no core change.

    .venv/bin/python custom_nodes/Saya_Couple_Upated/tests/diag_saya_dual_propagation.py [--h 96 --w 64 --sigma 5 --out report.json]

Runs (same weights, same noise, same sigma, cond only):
    A     main_only everywhere                       (reference = native MAIN)
    F1    main_locked_delta on the FIRST attn2 only, P1 only (mask_2 = 0)
    F2    same, P2 only (mask_1 = 0)
    F12   same, both persons
    FULL  main_locked_delta on every attn2, both persons
"F*" isolates one perturbation source so every later difference is its propagation.
"""

import argparse
import json
import os
import sys
import time
from pathlib import Path

sys.argv, _argv = sys.argv[:1] + ["--cpu"], sys.argv[1:]
from harness import COMFY_ROOT

sys.path.insert(0, str(COMFY_ROOT))

import torch

import comfy.options

comfy.options.enable_args_parsing()
import comfy.ldm.modules.attention as attention
import comfy.ldm.modules.diffusionmodules.openaimodel as openaimodel
import comfy.sd

# Local diagnostic: point it at your own SDXL checkpoint and saved Couple workflow.
CKPT = os.environ.get("SAYA_DIAG_CKPT", "")
WORKFLOW = os.environ.get(
    "SAYA_DIAG_WORKFLOW", str(COMFY_ROOT / "user" / "default" / "workflows" / "ILUSTCOUPLECLEAN.json")
)

parser = argparse.ArgumentParser()
parser.add_argument("--h", type=int, default=96)
parser.add_argument("--w", type=int, default=64)
parser.add_argument("--sigma", type=float, default=5.0)
parser.add_argument("--out", default=None)
args = parser.parse_args(_argv)
if not CKPT:
    sys.exit("set SAYA_DIAG_CKPT to an SDXL checkpoint path")


def prompts():
    nodes = {n["id"]: n for n in json.load(open(WORKFLOW))["nodes"]}
    return {name: nodes[nid]["widgets_values"][0] for name, nid in (("main", 144), ("p1", 807), ("p2", 808))}


def build():
    patcher, clip = comfy.sd.load_checkpoint_guess_config(CKPT, output_vae=False, output_clip=True, embedding_directory=None)[:2]
    conds = {}
    for name, text in prompts().items():
        cond = clip.encode_from_tokens_scheduled(clip.tokenize(text))
        conds[name] = (cond[0][0].float(), cond[0][1]["pooled_output"].float())
    return patcher, conds


class Recorder:
    def __init__(self, unet):
        self.points = {}
        self.exec_order = []
        self.handles = []
        self.fusion_calls = 0
        for name, module in unet.named_modules():
            if isinstance(module, attention.BasicTransformerBlock):
                self._block(name, module)
            elif isinstance(module, (openaimodel.ResBlock, attention.SpatialTransformer, openaimodel.Downsample, openaimodel.Upsample)):
                # the UNet calls its layers directly, so the container blocks never fire a hook
                self.handles.append(module.register_forward_hook(self._out(f"unet.{name}.{type(module).__name__}")))
        self.handles.append(unet.register_forward_hook(self._out("unet.out")))

    def _out(self, key):
        def hook(module, inputs, output):
            self.points[key] = output.detach().clone()
        return hook

    def _pre(self, key):
        def hook(module, inputs):
            self.points[key] = inputs[0].detach().clone()
        return hook

    def _block(self, name, block):
        self.handles += [
            block.norm1.register_forward_pre_hook(self._pre(f"{name}.x_in")),
            block.attn1.register_forward_hook(self._out(f"{name}.attn1_out")),
            block.norm2.register_forward_pre_hook(self._pre(f"{name}.x_mid")),
            block.norm3.register_forward_pre_hook(self._pre(f"{name}.x_post_attn2")),
            block.norm3.register_forward_hook(self._out(f"{name}.norm3_out")),
            block.ff.register_forward_hook(self._out(f"{name}.ff_out")),
            block.register_forward_hook(self._out(f"{name}.block_out")),
        ]
        self.handles.append(block.norm1.register_forward_pre_hook(lambda m, i: self.exec_order.append(name)))

    def close(self):
        for h in self.handles:
            h.remove()


def run(patcher, conds, mode, mask_1, mask_2, seed=0):
    unet = patcher.model.diffusion_model
    rec = Recorder(unet)
    real = dict(attention.SAYA_FUSION_MODES)
    only, locked = real["main_only"][0], real["main_locked_delta"][0]

    def spy(fn, key):
        def fused(m, a, b, ma, mb):
            out = fn(m, a, b, ma, mb)
            rec.points[f"fusion{rec.fusion_calls:02d}.{key}"] = out.detach().clone()
            rec.fusion_calls += 1
            return out
        return fused

    def first_only(m, a, b, ma, mb):
        return (locked if rec.fusion_calls == 0 else only)(m, a, b, ma, mb)

    attention.SAYA_FUSION_MODES["diag_only"] = (spy(only, "main"), frozenset())
    attention.SAYA_FUSION_MODES["diag_locked"] = (spy(locked, "out"), frozenset())
    attention.SAYA_FUSION_MODES["diag_first"] = (spy(first_only, "out"), frozenset())
    ctx = {k: v[0] for k, v in conds.items()}
    height, width = args.h, args.w
    generator = torch.Generator().manual_seed(seed)
    x = torch.randn(1, 4, height, width, generator=generator) * args.sigma
    options = {
        "cond_or_uncond": [0],
        "saya_dual_mode": True,
        "saya_dual": {"p1": ctx["p1"], "p2": ctx["p2"], "mask_1": mask_1, "mask_2": mask_2, "fusion_mode": mode, "params": {}},
    }
    y = patcher.model.encode_adm(pooled_output=conds["main"][1], width=width * 8, height=height * 8)
    try:
        with torch.no_grad():
            patcher.model.apply_model(x, torch.tensor([args.sigma]), c_crossattn=ctx["main"], y=y, transformer_options=options)
    finally:
        rec.close()
        for key in ("diag_only", "diag_locked", "diag_first"):
            attention.SAYA_FUSION_MODES.pop(key, None)
    return rec


def tokens(t):
    """[1,S,C] or [1,C,h,w] -> ([S,C] float64, (h,w) or None)."""
    t = t.double()
    if t.ndim == 4:
        h, w = t.shape[-2:]
        return t[0].flatten(1).T, (h, w)
    return t[0], None


GRID = {}


def grid_of(S, spatial):
    if spatial:
        return spatial
    return GRID[S]


def metrics(a, b, region=None, wrong_side=None):
    ta, sa = tokens(a)
    tb, _ = tokens(b)
    hw = grid_of(ta.shape[0], sa)
    d = tb - ta
    total = (d * d).sum()
    if total == 0:
        return None
    na, nb = ta.norm(dim=-1), tb.norm(dim=-1)
    dot = (ta * tb).sum(-1)
    alpha = (dot / (na * na).clamp_min(1e-12))
    cos_ab = dot / (na * nb).clamp_min(1e-12)
    cos_da = (d * ta).sum(-1) / (d.norm(dim=-1) * na).clamp_min(1e-12)
    out = dict(rel=float((total / (ta * ta).sum()).sqrt()), norm_ratio=float((nb / na.clamp_min(1e-12)).mean()),
               cos_ab=float(cos_ab.mean()), alpha=float(alpha.mean()), abs_cos_d_A=float(cos_da.abs().mean()))
    if region is not None:
        h, w = hw
        col = torch.arange(h * w) % w
        boundary = w // 2
        e = (d * d).sum(-1)
        wrong = (col < boundary) if region == "right" else (col >= boundary)
        out["leak"] = float(e[wrong].sum() / total)
        dist = (boundary - col) if region == "right" else (col - boundary + 1)
        out["leak_far"] = float(e[wrong & (dist > 4)].sum() / total)
    return out


def fmt(m):
    if m is None:
        return "identical"
    s = f"rel={m['rel']:.3e} alpha={m['alpha']:.5f} cos={m['cos_ab']:.5f} |cos(d,A)|={m['abs_cos_d_A']:.3f} ratio={m['norm_ratio']:.4f}"
    if "leak" in m:
        s += f" leak={m['leak']:.3e} far={m['leak_far']:.3e}"
    return s


def main():
    started = time.monotonic()
    patcher, conds = build()
    print(f"loaded in {time.monotonic() - started:.0f}s; latent {args.h}x{args.w}, sigma {args.sigma}", flush=True)
    height, width = args.h * 8, args.w * 8
    GRID.update({(args.h // 2) * (args.w // 2): (args.h // 2, args.w // 2), (args.h // 4) * (args.w // 4): (args.h // 4, args.w // 4)})
    right = torch.zeros(1, height, width)
    right[:, :, width // 2:] = 1.0
    left = 1.0 - right
    zeros = torch.zeros(1, height, width)

    runs = {}
    plan = [("A", "diag_only", right, left), ("F1", "diag_first", right, zeros), ("F2", "diag_first", zeros, left),
            ("F12", "diag_first", right, left), ("FULL", "diag_locked", right, left)]
    for name, mode, m1, m2 in plan:
        t = time.monotonic()
        runs[name] = run(patcher, conds, mode, m1, m2)
        print(f"run {name}: {time.monotonic() - t:.0f}s, {runs[name].fusion_calls} attn2 calls, {len(runs[name].points)} points", flush=True)

    A = runs["A"].points
    order = runs["A"].exec_order
    first = order[0]
    report = {"blocks_in_order": order, "first_block": first}
    region_of = {"F1": "right", "F2": "left", "F12": None, "FULL": None}

    def block_points(name):
        return [f"{name}.{p}" for p in ("x_in", "attn1_out", "x_mid", "x_post_attn2", "norm3_out", "ff_out", "block_out")]

    second = order[1]
    unet_points = ["input_blocks.4.1.SpatialTransformer", "input_blocks.5.0.ResBlock", "input_blocks.5.1.SpatialTransformer", "input_blocks.6.0.Downsample",
                   "input_blocks.7.0.ResBlock", "input_blocks.7.1.SpatialTransformer", "input_blocks.8.1.SpatialTransformer", "middle_block.0.ResBlock",
                   "middle_block.1.SpatialTransformer", "output_blocks.0.0.ResBlock", "output_blocks.0.1.SpatialTransformer", "output_blocks.3.0.ResBlock",
                   "output_blocks.3.2.Upsample", "output_blocks.5.0.ResBlock", "output_blocks.8.0.ResBlock"]
    listing = ["fusion00.out"] + block_points(first)[3:] + block_points(second) + [f"unet.{n}" for n in unet_points] + ["unet.out"]
    print(f"\nfirst Saya attn2 block = {first}; second block = {second}")
    for run_name in ("F1", "F2", "F12"):
        print(f"\n=== {run_name}: single perturbation source at {first} attn2 (leak = share of delta energy on the WRONG side of the split) ===")
        X = runs[run_name].points
        report[run_name] = {}
        for key in listing:
            ka = key if not key.startswith("fusion00") else "fusion00.main"
            if ka not in A or key not in X:
                continue
            m = metrics(A[ka], X[key], region_of[run_name])
            report[run_name][key] = m
            print(f"  {key:62s} {fmt(m)}")
    print("\n=== FIRST point (execution order) where the F1 / F2 delta crosses the split (leak > 1e-9) ===")
    for run_name in ("F1", "F2"):
        X = runs[run_name].points
        hit = None
        for key in X:
            if key.startswith("fusion") or key not in A:
                continue
            m = metrics(A[key], X[key], region_of[run_name])
            if m is not None and m.get("leak", 0) > 1e-9:
                hit = (key, m)
                break
        print(f"  {run_name}: {hit[0] if hit else 'never'}  {fmt(hit[1]) if hit else ''}")
        report[f"{run_name}_first_leak"] = hit[0] if hit else None
    print("\n=== FIRST point where MAIN is no longer exactly protected (|alpha-1| > 1e-6), F12, execution order ===")
    X = runs["F12"].points
    for key in X:
        if key not in A and not key.startswith("fusion"):
            continue
        ka = "fusion00.main" if key == "fusion00.out" else key
        if ka not in A:
            continue
        m = metrics(A[ka], X[key])
        if m is not None:
            print(f"  {key}  alpha={m['alpha']:.9f}  |cos(d,A)|={m['abs_cos_d_A']:.3e}")
            if abs(m["alpha"] - 1) > 1e-6:
                break
    print("\n=== FULL (every attn2 perturbed): MAIN stream vs A, per block output (execution order) ===")
    F = runs["FULL"].points
    report["FULL"] = {}
    for name in order[:: max(1, len(order) // 12)] + [order[-1]]:
        key = f"{name}.block_out"
        m = metrics(A[key], F[key])
        report["FULL"][key] = m
        print(f"  {key:62s} {fmt(m)}")
    for key in ("unet.out",):
        m = metrics(A[key], F[key])
        print(f"  {key:62s} {fmt(m)}")
    if args.out:
        Path(args.out).write_text(json.dumps(report, indent=1, default=str))
    print(f"\ndone in {time.monotonic() - started:.0f}s")


main()

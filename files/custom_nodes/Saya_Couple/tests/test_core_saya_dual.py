"""Saya Couple engine (src/engine/dual_attention.py) injected on a STOCK core block by object patch (M1).

Without payload the Saya copy of a block is bit-identical to HEAD's block; the forced attn2 path keeps MAIN
native; P1/P2 get their own attention on cond elements only; the engine owns the fusion table; every
foreign attn2 hook and every inconsistency raises RuntimeError, never a fallback."""

import subprocess
import types

import torch
from torch import nn

from harness import COMFY_ROOT, Check, load_pack

DIM, HEADS, DHEAD, CTX, TOK, HW = 16, 2, 8, 12, 5, (4, 4)


def _head_module():
    src = subprocess.run(["git", "show", "HEAD:comfy/ldm/modules/attention.py"], cwd=COMFY_ROOT, capture_output=True, text=True, check=True).stdout
    module = types.ModuleType("comfy.ldm.modules._attention_head")
    module.__package__ = "comfy.ldm.modules"
    exec(compile(src, "attention_HEAD.py", "exec"), module.__dict__)
    return module


def _blocks():
    """(engine module, Saya copy of a stock block, HEAD block with the same weights)."""
    load_pack()
    import comfy.ldm.modules.attention as attention
    import comfy.ops
    from saya_couple.src.engine import dual_attention as new
    from saya_couple.src.nodes import saya_dual_attention as dual

    head = _head_module()
    torch.manual_seed(0)
    b_stock = attention.BasicTransformerBlock(DIM, HEADS, DHEAD, context_dim=CTX, operations=comfy.ops.disable_weight_init)
    for p in b_stock.parameters():
        nn.init.normal_(p, std=0.3)
    b_head = head.BasicTransformerBlock(DIM, HEADS, DHEAD, context_dim=CTX, operations=comfy.ops.disable_weight_init)
    b_head.load_state_dict(b_stock.state_dict())
    return new, dual.saya_block(b_stock), b_head


def _inputs(flags, seed=1):
    g = torch.Generator().manual_seed(seed)
    x = torch.randn(len(flags), HW[0] * HW[1], DIM, generator=g)
    ctx = torch.randn(len(flags), TOK, CTX, generator=g)
    return x, ctx


def _base(flags):
    return {"cond_or_uncond": list(flags), "activations_shape": [len(flags), DIM, *HW]}


def _payload(p1=None, p2=None, mask_1=None, mask_2=None, mode="main_only", params=None):
    g = torch.Generator().manual_seed(7)
    default_p1, default_p2 = torch.randn(1, 7, CTX, generator=g), torch.randn(1, 3, CTX, generator=g)
    return {
        "p1": default_p1 if p1 is None else p1,
        "p2": default_p2 if p2 is None else p2,
        "mask_1": torch.cat([torch.ones(1, 8, 4), torch.zeros(1, 8, 4)], dim=2) if mask_1 is None else mask_1,
        "mask_2": torch.cat([torch.zeros(1, 8, 4), torch.ones(1, 8, 4)], dim=2) if mask_2 is None else mask_2,
        "fusion_mode": mode,
        "params": {} if params is None else params,
    }


def _opts(flags, payload=None, **extra):
    o = _base(flags)
    o["saya_dual"] = _payload() if payload is None else payload
    o.update(extra)
    return o


class _Spy:
    """Test-only entry in the core-owned fusion table that records what it receives."""

    def __init__(self, new, name="spy", result="main"):
        self.new, self.name, self.seen = new, name, {}
        self.result = result
        new.SAYA_FUSION_MODES[name] = (self.fusion, frozenset({"scale"}))

    def fusion(self, m, a, b, ma, mb, scale=None):
        self.seen = dict(m=m, a=a, b=b, ma=ma, mb=mb, scale=scale)
        return m

    def close(self):
        self.new.SAYA_FUSION_MODES.pop(self.name, None)


def _raises_with(c, fn, needle, label):
    try:
        fn()
    except RuntimeError as error:
        c.ok(needle in str(error), f"{label}: message {str(error)!r} lacks {needle!r}")
        return
    except Exception as error:
        c.failures.append(f"{label}: raised {type(error).__name__}, want RuntimeError")
        return
    c.failures.append(f"{label}: no exception")


def test_core_dual_off_bit_identical_to_head():
    new, b_new, b_head = _blocks()
    c = Check("core_dual_off_bit_identical_to_head")
    for flags in ([1, 0], [0], [0, 1]):
        x, ctx = _inputs(flags)
        base = _base(flags)
        ref = b_head(x, ctx, dict(base))
        c.ok(torch.equal(b_new(x, ctx, dict(base)), ref), f"payload absent, {flags}")
        c.ok(torch.equal(b_new(x, ctx, {**base, "saya_dual": None}), ref), f"payload None, {flags}")
    x, ctx = _inputs([0])
    replaced = lambda q, k, v, e: q * 2
    opts = {**_base([0]), "patches_replace": {"attn2": {("middle", 0, 0): replaced}}, "block": ("middle", 0)}
    c.ok(torch.equal(b_new(x, ctx, dict(opts)), b_head(x, ctx, dict(opts))), "historic attn2 replace path unchanged")
    override = lambda func, *a, **k: func(*a, **k)
    opts = {**_base([0]), "optimized_attention_override": override}
    c.ok(torch.equal(b_new(x, ctx, dict(opts)), b_head(x, ctx, dict(opts))), "override present + flag OFF unchanged")
    return c.report()


def test_core_dual_main_identity():
    new, b_new, b_head = _blocks()
    c = Check("core_dual_main_identity")
    for flags in ([1, 0], [0], [1], [0, 1]):
        x, ctx = _inputs(flags)
        ref = b_head(x, ctx, _base(flags))
        c.ok(torch.equal(b_new(x, ctx, _opts(flags)), ref), f"main_only is bit-identical to native, {flags}")
    return c.report()


def test_core_dual_cond_selection_and_operator():
    new, b_new, b_head = _blocks()
    c = Check("core_dual_cond_selection_and_operator")
    spy = _Spy(new)
    try:
        flags = [1, 0]
        x, ctx = _inputs(flags)
        payload = _payload(mode="spy", params={"scale": 2})
        b_new(x, ctx, _opts(flags, payload))
        seen = spy.seen
        c.eq(seen["scale"], 2, "allowed param reaches the core fusion")
        attn2 = b_new.attn2
        n = b_new.norm2(b_new.attn1(b_new.norm1(x)) + x)
        q = attn2.to_q(n)
        for key, tag in (("p1", "a"), ("p2", "b")):
            pc = payload[key]
            manual = attn2.to_out(new.optimized_attention(q[1:], attn2.to_k(pc).expand(1, -1, -1), attn2.to_v(pc).expand(1, -1, -1), HEADS,
                                                          attn_precision=attn2.attn_precision, transformer_options=_base(flags)))
            c.ok(torch.equal(seen[tag][1:], manual), f"{key} = to_out(independent attention with the block's own to_k/to_v)")
            c.ok(bool((seen[tag][:1] == 0).all()), f"{key} zero on the uncond chunk")
        c.ok(bool((seen["ma"][:1] == 0).all() and (seen["mb"][:1] == 0).all()), "masks zero on the uncond chunk")
        c.eq(tuple(seen["ma"].shape), (2, 16, 1), "mask layout [B,S,1]")
        grid = seen["ma"][1, :, 0].reshape(4, 4)
        c.ok(bool((grid[:, :2] == 1).all() and (grid[:, 2:] == 0).all()), "mask_1 nearest-exact resized to activations_shape")
        c.ok(bool(torch.equal(seen["ma"][1] + seen["mb"][1], torch.ones(16, 1))), "masks complementary on the cond chunk")
        first = dict(seen)
        b_new(x, ctx * 5, _opts(flags, _payload(p1=payload["p1"], p2=payload["p2"], mode="spy", params={"scale": 2})))
        c.ok(torch.equal(spy.seen["a"], first["a"]) and torch.equal(spy.seen["b"], first["b"]), "P1/P2 independent of the MAIN context")
        b_new(x, ctx, _opts(flags, _payload(p1=payload["p1"], p2=payload["p2"] * 3, mode="spy", params={"scale": 2})))
        c.ok(torch.equal(spy.seen["a"], first["a"]) and not torch.equal(spy.seen["b"], first["b"]), "P1 independent of P2")

        # cond selection is real: count projections and attention calls, and the q batch they receive
        counts = {"k": 0, "v": 0}
        calls = []
        real_k, real_v, real_attn = attn2.to_k.forward, attn2.to_v.forward, new.optimized_attention
        attn2.to_k.forward = lambda *a, **kw: (counts.__setitem__("k", counts["k"] + 1), real_k(*a, **kw))[1]
        attn2.to_v.forward = lambda *a, **kw: (counts.__setitem__("v", counts["v"] + 1), real_v(*a, **kw))[1]
        new.optimized_attention = lambda q_, *a, **kw: (calls.append(q_.shape[0]), real_attn(q_, *a, **kw))[1]
        try:
            def run(flags_):
                counts.update(k=0, v=0)
                calls.clear()
                xx, cc = _inputs(flags_)
                b_new(xx, cc, _opts(flags_, _payload(mode="spy")))
                return dict(counts), list(calls)  # attn1's self-attention runs in the core module, not counted here (M1)

            c.eq(run([1]), ({"k": 1, "v": 1}, [1]), "all-uncond batch: MAIN only, P1/P2 to_k/to_v/attention never called")
            c.eq(run([1, 1]), ({"k": 1, "v": 1}, [2]), "all-uncond batch of 2 chunks: MAIN only")
            c.eq(run([0]), ({"k": 3, "v": 3}, [1, 1, 1]), "cond-only batch: MAIN + P1 + P2")
            c.eq(run([1, 0]), ({"k": 3, "v": 3}, [2, 1, 1]), "[uncond, cond]: P1/P2 attention sees only the cond q")
            c.eq(run([0, 1, 0]), ({"k": 3, "v": 3}, [3, 2, 2]), "[cond, uncond, cond]: P1/P2 attention sees the 2 cond q")
        finally:
            attn2.to_k.forward, attn2.to_v.forward, new.optimized_attention = real_k, real_v, real_attn
        for flags_ in ([0, 1, 0], [1, 1]):
            xx, cc = _inputs(flags_)
            b_new(xx, cc, _opts(flags_, _payload(mode="spy")))
            i = [j for j, f in enumerate(flags_) if f == 1]
            for j in i:
                c.ok(bool((spy.seen["a"][j:j + 1] == 0).all() and (spy.seen["ma"][j:j + 1] == 0).all()), f"{flags_}: uncond chunk {j} untouched by P1")
    finally:
        spy.close()
    return c.report()


def test_core_dual_to_out_once():
    new, b_new, b_head = _blocks()
    c = Check("core_dual_to_out_once")
    seen = []
    real = b_new.attn2.to_out.forward
    b_new.attn2.to_out.forward = lambda x_, *a, **k: (seen.append(x_.shape[0]), real(x_, *a, **k))[1]
    try:
        for mode in ("main_only", "main_locked_delta"):
            for flags in ([1, 0], [0, 1, 0], [1, 1]):
                seen.clear()
                x, ctx = _inputs(flags)
                b_new(x, ctx, _opts(flags, _payload(mode=mode)))
                n_cond = flags.count(0)
                want = [len(flags)] + ([n_cond, n_cond] if n_cond else [])
                c.eq(seen, want, f"{mode} {flags}: MAIN to_out once on the full batch, then only PERSON to_out on the cond q; nothing after the Saya branch")
    finally:
        b_new.attn2.to_out.forward = real
    return c.report()


def test_core_dual_fusion_is_core_owned():
    new, b_new, b_head = _blocks()
    c = Check("core_dual_fusion_is_core_owned")
    flags = [1, 0]
    x, ctx = _inputs(flags)
    ok_payload = _payload()
    c.eq(sorted(new.SAYA_FUSION_MODES), ["main_locked_delta", "main_locked_delta_dynamic", "main_only"], "core table: main_only (identity), main_locked_delta, main_locked_delta_dynamic")
    p = dict(ok_payload); p["fusion"] = lambda *a: a[0]
    _raises_with(c, lambda: b_new(x, ctx, _opts(flags, p)), "saya_dual", "callable under a 'fusion' key")
    _raises_with(c, lambda: b_new(x, ctx, _opts(flags, _payload(mode=lambda *a: a[0]))), "fusion_mode must be a string", "callable as fusion_mode")
    _raises_with(c, lambda: b_new(x, ctx, _opts(flags, _payload(mode="unknown"))), "unknown fusion_mode", "unknown fusion_mode")
    _raises_with(c, lambda: b_new(x, ctx, _opts(flags, _payload(params={"gain": 1.0}))), "does not accept parameters", "unlisted parameter")
    _raises_with(c, lambda: b_new(x, ctx, _opts(flags, _payload(params=[1]))), "params a dict", "params not a dict")
    for bad in (1, "yes", [1]):
        _raises_with(c, lambda bad=bad: b_new(x, ctx, {**_opts(flags), "saya_dual": bad}), "must be a dict", f"non-dict payload {bad!r}")
    real = dict(new.SAYA_FUSION_MODES)
    new.SAYA_FUSION_MODES["bad_shape"] = (lambda m, a, b, ma, mb: m[:1], frozenset())
    new.SAYA_FUSION_MODES["boom"] = (lambda m, a, b, ma, mb: (_ for _ in ()).throw(ValueError("boom")), frozenset())
    try:
        _raises_with(c, lambda: b_new(x, ctx, _opts(flags, _payload(mode="bad_shape"))), "fusion returned", "fusion wrong shape")
        c.raises(ValueError, lambda: b_new(x, ctx, _opts(flags, _payload(mode="boom"))), "fusion exception propagates (no fallback)")
    finally:
        new.SAYA_FUSION_MODES.clear()
        new.SAYA_FUSION_MODES.update(real)
    return c.report()


def test_core_dual_forbidden_attn2_hooks():
    new, b_new, b_head = _blocks()
    c = Check("core_dual_forbidden_attn2_hooks")
    flags = [1, 0]
    x, ctx = _inputs(flags)
    ran = []
    foreign = lambda *a, **k: ran.append(1)
    cases = {
        "attn2_patch": {"patches": {"attn2_patch": [foreign]}},
        "attn2_output_patch": {"patches": {"attn2_output_patch": [foreign]}},
        "attn2 patches_replace": {"patches_replace": {"attn2": {("middle", 0, 0): foreign}}},
        "optimized_attention_override": {"optimized_attention_override": foreign},
    }
    for needle, extra in cases.items():
        _raises_with(c, lambda extra=extra: b_new(x, ctx, _opts(flags, **extra)), needle, f"forbidden {needle}")
    c.eq(ran, [], "no foreign hook was executed before the refusal")
    _raises_with(c, lambda: b_new(x, ctx, _opts(flags, **cases["optimized_attention_override"])),
                 "Saya forced attention: optimized_attention_override is forbidden", "override exact message")
    b_new.switch_temporal_ca_to_sa = True
    try:
        _raises_with(c, lambda: b_new(x, ctx, _opts(flags)), "switch_temporal_ca_to_sa", "forbidden switch_temporal_ca_to_sa block")
    finally:
        b_new.switch_temporal_ca_to_sa = False

    hit = []
    def pag(q, k, v, e):
        hit.append(1)
        return v
    ok = _opts([0], patches_replace={"attn1": {("middle", 0, 0): pag}}, block=("middle", 0))
    x1, c1 = _inputs([0])
    b_new(x1, c1, ok)
    c.eq(len(hit), 1, "attn1 replace (PAG) allowed and still runs")
    seen = []
    ok = _opts([0], patches={"attn1_patch": [lambda n, ctx_, v, e: (seen.append(1), (n, ctx_, v))[1]],
                             "attn1_output_patch": [lambda n, e: (seen.append(2), n)[1]],
                             "middle_patch": [lambda xx, e: (seen.append(3), xx)[1]]})
    b_new(x1, c1, ok)
    c.eq(sorted(seen), [1, 2, 3], "attn1_patch / attn1_output_patch / middle_patch allowed")
    return c.report()


def test_core_dual_fail_closed_shapes():
    new, b_new, b_head = _blocks()
    c = Check("core_dual_fail_closed_shapes")
    flags = [1, 0]
    x, ctx = _inputs(flags)

    def run(opts, label, xx=x, cc=ctx):
        c.raises(RuntimeError, lambda: b_new(xx, cc, opts), label)

    o = _opts(flags); del o["saya_dual"]
    c.ok(torch.equal(b_new(x, ctx, o), b_head(x, ctx, _base(flags))), "payload absent = no engine: stock output (Solo)")
    for key in ("p1", "p2", "mask_1", "mask_2", "fusion_mode", "params"):
        p = _payload(); del p[key]; run(_opts(flags, p), f"payload missing {key}")
    run(_opts(flags, _payload(p1=torch.zeros(2, 7, CTX))), "p1 batch != 1")
    run(_opts(flags, _payload(p1=torch.zeros(1, 7, CTX + 1))), "p1 channel mismatch")
    run(_opts(flags, _payload(p2=torch.zeros(7, CTX))), "p2 wrong ndim")
    run(_opts(flags, _payload(mask_1=torch.zeros(3, 8, 8))), "mask batch mismatch")
    run(_opts(flags, _payload(mask_2=torch.zeros(8))), "mask wrong ndim")
    run(_opts(flags, _payload(mask_1=torch.full((1, 8, 8), float("nan")))), "mask not finite")
    o = _opts(flags); del o["cond_or_uncond"]; run(o, "cond_or_uncond absent")
    o = _opts(flags); del o["activations_shape"]; run(o, "activations_shape absent")
    run(_opts([1, 2]), "cond_or_uncond flag 2")
    run(_opts([1, 0, 0]), "cond_or_uncond length does not divide the batch")
    run(_opts(flags, activations_shape=[2, DIM, 3, 4]), "activations_shape does not match the tokens")
    run(_opts(flags), "context batch != query batch", cc=ctx[:1])
    return c.report()


def test_core_dual_dtype():
    new, b_new, b_head = _blocks()
    c = Check("core_dual_dtype")
    x, ctx = _inputs([1, 0])
    b16 = b_new.bfloat16()
    out = b16(x.bfloat16(), ctx.bfloat16(), _opts([1, 0]))
    c.eq(out.dtype, torch.bfloat16, "dtype preserved")
    return c.report()


def _locked(flags=(1, 0), p1=None, p2=None, mask_1=None, mask_2=None, dtype=torch.float32, ctx_same=False, seed=1):
    """Runs main_locked_delta and returns (out, M, P1, P2, m1, m2) as seen by the fusion, plus the block."""
    new, b_new, b_head = _blocks()
    b_new = b_new.to(dtype)
    spy = _Spy(new, "spy")
    real = new.SAYA_FUSION_MODES["main_locked_delta"]
    grabbed = {}

    def capture(m, a, b, ma, mb):
        grabbed.update(m=m, a=a, b=b, ma=ma, mb=mb)
        return real[0](m, a, b, ma, mb)

    new.SAYA_FUSION_MODES["spy"] = (capture, frozenset())
    x, ctx = _inputs(list(flags), seed=seed)
    if ctx_same:
        ctx = ctx[:1].expand(len(flags), -1, -1).contiguous()
    try:
        out = b_new(x.to(dtype), ctx.to(dtype), _opts(list(flags), _payload(p1=p1, p2=p2, mask_1=mask_1, mask_2=mask_2, mode="spy")))
    finally:
        spy.close()
    return out, grabbed, b_new, x, ctx


def test_core_locked_delta_orthogonal_to_main():
    new, b_new, b_head = _blocks()
    c = Check("core_locked_delta_orthogonal_to_main")
    for flags in ([1, 0], [0], [0, 1, 0]):
        x, ctx = _inputs(flags)
        seen = {}
        fusion_real = new.SAYA_FUSION_MODES["main_locked_delta"][0]

        def capture(m, a, b, ma, mb):
            out = fusion_real(m, a, b, ma, mb)
            seen.update(m=m, out=out, a=a, ma=ma)
            return out

        new.SAYA_FUSION_MODES["spy"] = (capture, frozenset())
        try:
            b_new(x, ctx, _opts(flags, _payload(mode="spy")))
        finally:
            new.SAYA_FUSION_MODES.pop("spy")
        delta = seen["out"] - seen["m"]
        dot = (delta * seen["m"]).sum(-1)
        scale = delta.norm(dim=-1) * seen["m"].norm(dim=-1) + 1e-12
        c.ok(bool((dot.abs() / scale).max() < 1e-5), f"dot(OUT - M, M) ~ 0 per token, {flags}")
        cond = [i for i, f in enumerate(flags) if f == 0]
        c.ok(any(float(delta[i].abs().max()) > 1e-4 for i in cond), f"{flags}: the locked delta is not trivially zero")
        for i, f in enumerate(flags):
            if f == 1:
                c.ok(torch.equal(seen["out"][i], seen["m"][i]), f"{flags}: uncond chunk {i} is exactly M")
        c.ok(bool(torch.isfinite(seen["out"]).all()), f"{flags}: finite")
    return c.report()


def test_core_locked_delta_identity_cases():
    new, b_new, b_head = _blocks()
    c = Check("core_locked_delta_identity_cases")
    flags = [1, 0]
    x, ctx = _inputs(flags)
    ref = b_head(x, ctx, _base(flags))
    # mask1 = mask2 = 0 -> OUT == M == native
    zero = torch.zeros(1, 8, 8)
    out = b_new(x, ctx, _opts(flags, _payload(mask_1=zero, mask_2=zero, mode="main_locked_delta")))
    c.ok(torch.equal(out, ref), "mask1 = mask2 = 0: bit-identical to the native block")
    # P1 == P2 == MAIN (same context for every element) -> OUT == M up to rounding
    one = _inputs([0])[1][:1]
    ctx_same = one.expand(2, -1, -1).contiguous()
    ref_same = b_head(x, ctx_same, _base(flags))
    out = b_new(x, ctx_same, _opts(flags, _payload(p1=one.clone(), p2=one.clone(), mode="main_locked_delta")))
    c.ok(torch.allclose(out, ref_same, atol=1e-5, rtol=1e-5), f"P1 == P2 == MAIN: OUT == M to rounding (max diff {float((out - ref_same).abs().max()):.2e})")
    # main_only still bit-identical after to_out moved into the Saya path
    for fl in ([1, 0], [0], [1], [0, 1]):
        xx, cc = _inputs(fl)
        c.ok(torch.equal(b_new(xx, cc, _opts(fl)), b_head(xx, cc, _base(fl))), f"main_only bit-identical to native, {fl}")
    return c.report()


def test_core_locked_delta_person_independence():
    new, b_new, b_head = _blocks()
    c = Check("core_locked_delta_person_independence")
    flags = [1, 0]
    x, ctx = _inputs(flags)
    base = _payload(mode="spy")
    seen = {}

    def run(payload):
        spy = _Spy(new, "spy")
        try:
            b_new(x, ctx, _opts(flags, payload))
            return dict(spy.seen)
        finally:
            spy.close()

    a = run(base)
    changed = run(_payload(p1=base["p1"] * 3, p2=base["p2"], mode="spy"))
    c.ok(torch.equal(a["m"], changed["m"]), "modifying P1 leaves M unchanged")
    c.ok(torch.equal(a["b"], changed["b"]) and torch.equal(a["mb"], changed["mb"]), "modifying P1 leaves P2 (and mask_2) unchanged")
    c.ok(not torch.equal(a["a"], changed["a"]), "modifying P1 changes P1")
    # in the real law: with mask_1 = 0 the P1 context has no effect at all
    zero = torch.zeros(1, 8, 8)
    o1 = b_new(x, ctx, _opts(flags, _payload(mask_1=zero, mode="main_locked_delta")))
    o2 = b_new(x, ctx, _opts(flags, _payload(p1=base["p1"] * 7, mask_1=zero, mode="main_locked_delta")))
    c.ok(torch.equal(o1, o2), "mask_1 = 0: P1 has no effect on OUT")
    # P1 only acts inside mask_1, P2 only inside mask_2 (left/right halves)
    left_only = _payload(mask_2=zero, mode="main_locked_delta")
    ref = b_head(x, ctx, _base(flags))
    out = b_new(x, ctx, _opts(flags, left_only))
    diff = (out - ref).abs().amax(-1)[1].reshape(4, 4)
    c.ok(bool((diff[:, 2:] == 0).all()) and bool((diff[:, :2] > 0).any()), "mask_1 (left half) is the only region that moves when mask_2 = 0")
    return c.report()


def test_core_locked_delta_dtype_and_uncond():
    new, b_new, b_head = _blocks()
    c = Check("core_locked_delta_dtype_and_uncond")
    b16 = b_new.bfloat16()
    for flags in ([1, 0], [1, 1]):
        x, ctx = _inputs(flags)
        out = b16(x.bfloat16(), ctx.bfloat16(), _opts(flags, _payload(mode="main_locked_delta")))
        c.eq(out.dtype, torch.bfloat16, f"dtype preserved, {flags}")
        c.eq(out.device, x.device, f"device preserved, {flags}")
        c.ok(bool(torch.isfinite(out.float()).all()), f"finite, {flags}")
    counts = {"k": 0}
    attn2 = b16.attn2
    real_k = attn2.to_k.forward
    real_attn = new.optimized_attention
    calls = []
    attn2.to_k.forward = lambda *a, **kw: (counts.__setitem__("k", counts["k"] + 1), real_k(*a, **kw))[1]
    new.optimized_attention = lambda q_, *a, **kw: (calls.append(1), real_attn(q_, *a, **kw))[1]
    try:
        x, ctx = _inputs([1, 1])
        b16(x.bfloat16(), ctx.bfloat16(), _opts([1, 1], _payload(mode="main_locked_delta")))
    finally:
        attn2.to_k.forward, new.optimized_attention = real_k, real_attn
    c.eq((counts["k"], len(calls)), (1, 1), "all-uncond batch under main_locked_delta: no PERSON to_k / attention (MAIN only; attn1 is the core's)")
    _raises_with(c, lambda: b16(*[t.bfloat16() for t in _inputs([1, 0])], _opts([1, 0], _payload(mode="main_locked_delta", params={"gain": 1.0}))),
                 "does not accept parameters", "no gain parameter exists on main_locked_delta")
    return c.report()


def test_core_diff_is_isolated():
    """M1: the engine lives in the pack; the Saya copy of a block is a subclass of the stock class whose only addition is
    the hook check; nothing in the pack depends on a patched core (the rollback injection aside)."""
    new, b_new, b_head = _blocks()
    import comfy.ldm.modules.attention as attention
    from harness import PACK_ROOT
    c = Check("core_diff_is_isolated")
    c.ok(new.__file__.startswith(str(PACK_ROOT)), f"engine module in the pack ({new.__file__})")
    c.ok(type(b_new).saya_original_class is attention.BasicTransformerBlock and type(b_new).__mro__[1] is attention.BasicTransformerBlock,
         "Saya block = direct subclass of the stock block class")
    c.ok(b_new.saya_source[0].attn2 is b_new.attn2.saya_attn[0] and b_new.attn2.to_q is b_new.saya_source[0].attn2.to_q, "proxy shares the stock attn2 projections")
    c.eq(list(b_new.state_dict().keys()), list(b_head.state_dict().keys()), "state_dict keys identical to the stock block")
    return c.report()


def test_core_dynamic_ownership():
    new, b_new, _ = _blocks()
    c = Check("core_dynamic_ownership")
    flags = [1, 0]
    x, ctx = _inputs(flags)
    new._SAYA_OWNERSHIP_STATE.clear()
    g = torch.Generator().manual_seed(11)
    anchors = {"p1_anchor": (torch.randn(1, 6, CTX, generator=g), [1, 2]), "p2_anchor": (torch.randn(1, 6, CTX, generator=g), [1, 3]),
               "person_anchor": (torch.randn(1, 6, CTX, generator=g), [1])}
    static = _payload(mode="main_locked_delta")
    dynamic = dict(static, fusion_mode="main_locked_delta_dynamic", params={"start_sigma": 5.0, **anchors})
    no_anchor = dict(static, fusion_mode="main_locked_delta_dynamic", params={"start_sigma": 5.0})
    ref = b_new(x, ctx, _opts(flags, static))
    c.ok(torch.equal(b_new(x, ctx, _opts(flags, dynamic, sigmas=torch.tensor([9.0]))), ref), "sigma above start_sigma: bit-identical to main_locked_delta")
    c.ok(torch.equal(b_new(x, ctx, _opts(flags, no_anchor, sigmas=torch.tensor([3.0]))), ref), "no anchors: static split")
    c.ok(torch.equal(b_new(x, ctx, _opts(flags, dynamic, sigmas=torch.tensor([4.0]))), ref), "first step below start_sigma: map not ready, static split")
    out = b_new(x, ctx, _opts(flags, dynamic, sigmas=torch.tensor([3.0])))
    c.ok(bool(torch.isfinite(out).all()) and out.shape == ref.shape, "dynamic step: finite, same shape")
    own = next(iter(new._SAYA_OWNERSHIP_STATE.values()))["map"]
    c.ok(own is not None and own.shape == (1, 3, *HW), "ownership map [cond, 3, H, W] (P1 vote, person, background)")
    n = b_new.norm2(x)
    q = b_new.attn2.to_q(n)
    m1 = torch.cat([torch.ones(2, 8, 1), torch.zeros(2, 8, 1)], dim=1)
    masks = new.saya_dynamic_masks(b_new, n, q, dynamic, [1], 1, *HW, [m1, 1 - m1], {"sigmas": torch.tensor([3.0])})
    c.ok(torch.allclose(masks[0] + masks[1], torch.ones_like(m1)), "dynamic masks keep mask_1 + mask_2 == 1")
    c.ok(bool(((masks[0] == 0) | (masks[0] == 1)).all()), "no P1/P2 blend weight")
    c.ok(torch.equal(masks[0][0], m1[0]), "uncond element keeps the static mask")
    affinity, anchor = new.saya_ownership_evidence(b_new, n[1:], q[1:], (anchors["p1_anchor"], anchors["p2_anchor"], anchors["person_anchor"]))
    c.ok(affinity.shape == (1, 16, 16) and anchor.shape == (1, 3, 16), "evidence: affinity [B, N, N], anchors [B, 3, N]")
    labels, owner, confident, zone_owner, zone_role, _ = new.saya_ownership_zones(affinity, anchor, HW, {**new.SAYA_DYNAMIC_DEFAULTS, "zones": 3})
    c.ok(bool(((owner == 0) | (owner == 1)).all() & ((confident == 0) | (confident == 1)).all()), "zone ownership exclusively 0/1")
    c.ok(all(len(set(owner[0][labels[0] == z].tolist())) <= 1 for z in range(3)), "a whole zone has a single owner")
    one_sided = torch.stack([anchor[:, 0], anchor[:, 0] - 99, anchor[:, 2]], dim=1)
    _, _, confident, _, _, _ = new.saya_ownership_zones(affinity, one_sided, HW, {**new.SAYA_DYNAMIC_DEFAULTS, "zones": 3})
    c.ok(not bool(confident.any()), "gate: without an anchor zone for each person, static split everywhere")
    no_person = torch.stack([anchor[:, 0], anchor[:, 1], torch.full_like(anchor[:, 2], -1.0)], dim=1)
    _, _, _, _, zone_role, _ = new.saya_ownership_zones(affinity, no_person, HW, {**new.SAYA_DYNAMIC_DEFAULTS, "zones": 3})
    c.ok(not bool((zone_role == new.SAYA_ZONE_LINKED).any()), "no person presence: no zone is linked (background stays static)")
    # Link veto: zone 2 (half its pixels clearly P1) is tied to P2's head and body; zone 3 is P2's body.
    lab = torch.tensor([0, 0, 1, 1] * 2 + [2, 2, 3, 3] * 2)
    tied = torch.zeros(16, 16)
    for z in range(4):
        tied[(lab == z).nonzero()[:, 0][:, None], (lab == z).nonzero()[:, 0]] = 1.0
    for a, b in ((3, 1), (1, 3), (2, 3), (3, 2), (2, 1)):
        tied[(lab == a).nonzero()[:, 0][:, None], (lab == b).nonzero()[:, 0]] = 0.7
    tied = tied / tied.sum(-1, keepdim=True)
    lit_p1 = torch.where(lab == 0, 3.0, -1.0)
    lit_p1[(lab == 2).nonzero()[:2, 0]] = 2.5
    veto_anchor = torch.stack([lit_p1, torch.where(lab == 1, 3.0, torch.where(lab == 3, 0.5, -1.0)), torch.where(lab == 2, 0.3, 1.0)])[None]
    _, _, _, zone_owner, zone_role, _ = new.saya_ownership_zones(tied[None], veto_anchor, HW, {**new.SAYA_DYNAMIC_DEFAULTS, "zones": 4, "background_main": True})
    c.eq(zone_owner[0].tolist(), [0, 1, -1, 1], "link veto: a zone with >= contest_share pixels clearly the other person is not linked; the real body still is")
    split = torch.stack([torch.tensor([[3.0] * 8 + [-3.0] * 8]), torch.zeros(1, 16), torch.ones(1, 16)], dim=1)
    one_zone = torch.full((1, 16, 16), 1 / 16)
    _, _, confident, _, _, _ = new.saya_ownership_zones(one_zone, split, HW, {**new.SAYA_DYNAMIC_DEFAULTS, "zones": 1})
    c.ok(not bool(confident.any()), "a lone contested zone (clearly P1 and clearly P2 pixels) stays static")
    real_zones = new.saya_ownership_zones

    def hysteresis(script):
        """0 / 1 = person found; "m" = anchor zones present but this pixel unassigned (a miss); "x" = no anchor zone (no evidence)."""
        steps = iter(script)

        def scripted(affinity, anchor, grid, params):
            f = next(steps)
            owner = torch.full((1, 16), 1.0 if f == 0 else 0.0)
            return torch.zeros(1, 16, dtype=torch.long), owner, torch.full((1, 16), 1.0 if f in (0, 1) else 0.0), None, None, {"gate": [f != "x"]}
        new.saya_ownership_zones = scripted
        try:
            state = {"sigma": 1.0, "grid": [4, 4], "count": 1, "active": None, "affinity": torch.zeros(1, 16, 16), "anchor": torch.zeros(1, 3, 16)}
            seen = []
            for _ in script:
                new._saya_finish_ownership_step(state, new.SAYA_DYNAMIC_DEFAULTS)
                seen.append(int(state["active"][0, 0, 0]))
        finally:
            new.saya_ownership_zones = real_zones
        return seen
    c.eq(hysteresis([0, 0, 0, "m", 0, "m", "m", 1, 1, 1]), [-1, -1, 0, 0, 0, 0, -1, -1, -1, 1],
         "hysteresis: 3 confirmations to appear, 1 miss tolerated, 2 misses or the other person -> static, 3 confirmations again")
    c.eq(hysteresis([0, "x", 0, "x", 0, "x", "x", "x", 0, 1]), [-1, -1, -1, -1, 0, 0, 0, 0, 0, -1],
         "hysteresis: a step without anchor zones is skipped (no confirmation, no miss)")
    # zone_fallback: zone 0 = person zone across the split (75 % on the P1 side) -> whole zone P1;
    # zone 1 = background -> pixel-wise; zone 2 = contested -> pixel-wise; zone 3 = head anchor -> pixel-wise.
    labels = torch.tensor([[0] * 4 + [1] * 4 + [2] * 4 + [3] * 4])
    def staged(affinity, anchor, grid, params):
        info = {"contested": torch.tensor([[False, False, True, False]]), "person": torch.tensor([[True, False, True, True]]), "gate": [True]}
        return labels, torch.zeros(1, 16), torch.zeros(1, 16), torch.tensor([[-1, -1, -1, 0]]), torch.tensor([[0, 0, 0, new.SAYA_ZONE_ANCHOR]]), info
    new.saya_ownership_zones = staged
    try:
        side = torch.tensor([[1.0, 1.0, 1.0, 0.0] + [1.0] * 4 + [1.0, 1.0, 0.0, 0.0] + [0.0] * 4]).reshape(1, 4, 4)
        state = {"sigma": 1.0, "grid": [4, 4], "count": 1, "active": None, "affinity": torch.zeros(1, 16, 16), "anchor": torch.zeros(1, 3, 16), "static_grid": side}
        new._saya_finish_ownership_step(state, {**new.SAYA_DYNAMIC_DEFAULTS, "zone_fallback": True})
        got = state["block"].flatten().tolist()
    finally:
        new.saya_ownership_zones = real_zones
    c.eq(got, [0] * 4 + [-1] * 12, "zone_fallback: a person zone across the split goes whole to the majority side; background, contested and head zones stay pixel-wise")
    state = {"sigma": 1.0, "grid": [4, 4], "count": 1, "active": None, "affinity": torch.zeros(1, 16, 16), "anchor": torch.zeros(1, 3, 16), "static_grid": side, "block": None}
    new.saya_ownership_zones = staged
    try:
        new._saya_finish_ownership_step(state, new.SAYA_DYNAMIC_DEFAULTS)
    finally:
        new.saya_ownership_zones = real_zones
    c.ok(state["block"] is None, "zone_fallback off by default: no block map")
    # background_main: zone 1 (no person, no owner) becomes background after confirm_steps steps -> MAIN only.
    new.saya_ownership_zones = staged
    try:
        state = {"sigma": 1.0, "grid": [4, 4], "count": 1, "active": None, "affinity": torch.zeros(1, 16, 16), "anchor": torch.zeros(1, 3, 16), "static_grid": side, "block": None}
        for _ in range(3):
            new._saya_finish_ownership_step(state, {**new.SAYA_DYNAMIC_DEFAULTS, "background_main": True})
        bg = state["map"][0, 2].flatten().tolist()
        state_off = {"sigma": 1.0, "grid": [4, 4], "count": 1, "active": None, "affinity": torch.zeros(1, 16, 16), "anchor": torch.zeros(1, 3, 16), "static_grid": side, "block": None}
        for _ in range(3):
            new._saya_finish_ownership_step(state_off, new.SAYA_DYNAMIC_DEFAULTS)
    finally:
        new.saya_ownership_zones = real_zones
    c.eq(bg, [0.0] * 4 + [1.0] * 4 + [0.0] * 8, "background_main: only the person-free, unowned zone becomes background (after 3 confirmations)")
    c.ok(float(state_off["map"][0, 2].sum()) == 0, "background_main off by default: no background")
    new.saya_ownership_zones = staged
    try:
        person_anchor = torch.zeros(1, 3, 16)
        person_anchor[0, 2, 4:6] = 1.0  # two pixels of the background zone where the person anchor is above the mean
        state = {"sigma": 1.0, "grid": [4, 4], "count": 1, "active": None, "affinity": torch.zeros(1, 16, 16), "anchor": person_anchor, "static_grid": side, "block": None}
        for _ in range(3):
            new._saya_finish_ownership_step(state, {**new.SAYA_DYNAMIC_DEFAULTS, "background_main": True})
    finally:
        new.saya_ownership_zones = real_zones
    c.eq(state["map"][0, 2].flatten().tolist(), [0.0] * 6 + [1.0] * 2 + [0.0] * 8, "background_main: a person pixel inside a background zone keeps the static split")
    # main_scene: background pixels of cond elements take the scene-only MAIN attention, the rest is untouched.
    attn2 = b_new.attn2
    q = attn2.to_q(b_new.norm2(x))
    main_out = torch.randn(2, 16, DIM)
    scene = torch.randn(1, 5, CTX)
    key_payload = dict(dynamic, params={**dynamic["params"], "main_scene": scene})
    bg_map = torch.zeros(1, 3, 4, 4)
    bg_map[0, 2, :2] = 1.0
    new._SAYA_OWNERSHIP_STATE.clear()
    new._SAYA_OWNERSHIP_STATE[id(key_payload["p1"])] = {"map": bg_map}
    out = new.saya_scene_main(attn2, q, main_out, key_payload, [1], 1, 4, 4, {})
    background = bg_map[0, 2].flatten().bool()
    c.ok(torch.equal(out[0], main_out[0]), "main_scene: uncond element untouched")
    c.ok(torch.equal(out[1][~background], main_out[1][~background]), "main_scene: person / static pixels keep MAIN + ACTION")
    c.ok(not torch.allclose(out[1][background], main_out[1][background]), "main_scene: background pixels take the scene-only MAIN")
    no_scene = dict(dynamic, params=dict(dynamic["params"]))
    c.ok(new.saya_scene_main(attn2, q, main_out, no_scene, [1], 1, 4, 4, {}) is main_out, "no main_scene: MAIN unchanged")
    new._SAYA_OWNERSHIP_STATE.clear()
    new._SAYA_OWNERSHIP_STATE.clear()
    try:
        b_new(x, ctx, _opts(flags, dynamic))
        c.ok(False, "missing sigmas must raise")
    except RuntimeError:
        c.ok(True, "missing sigmas raises RuntimeError")
    new._SAYA_OWNERSHIP_STATE.clear()
    return c.report()


TESTS = (
    test_core_dynamic_ownership,
    test_core_dual_off_bit_identical_to_head,
    test_core_dual_main_identity,
    test_core_dual_cond_selection_and_operator,
    test_core_dual_to_out_once,
    test_core_dual_fusion_is_core_owned,
    test_core_dual_forbidden_attn2_hooks,
    test_core_dual_fail_closed_shapes,
    test_core_dual_dtype,
    test_core_locked_delta_orthogonal_to_main,
    test_core_locked_delta_identity_cases,
    test_core_locked_delta_person_independence,
    test_core_locked_delta_dtype_and_uncond,
    test_core_diff_is_isolated,
)

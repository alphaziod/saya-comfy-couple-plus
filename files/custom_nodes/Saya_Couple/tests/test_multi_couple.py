"""SayaMultiCouple: real independence of the patches, outputs, zero-PPM guard.

Independence is tested functionally: the patch stored in model 1's clone is
EXECUTED after a second model is coupled with irreconcilable conditionings.
A deliberate negative control (shared AttentionCouple instance) proves that
this test really discriminates the state leak the node is designed to
prevent.
"""

import copy
import re
import sys
from pathlib import Path
from types import SimpleNamespace

import torch
from torch import nn

from harness import COMFY_ROOT, PACK_ROOT, Check, load_pack

NODE_PATH = PACK_ROOT / "src" / "nodes" / "saya_multi_couple.py"


class _FakeProj:
    """Replaces to_k / to_v: identity, with a .weight for the dtype."""

    def __init__(self):
        self.weight = torch.zeros(1, 1)

    def __call__(self, x):
        return x


class _FakeAttn2:
    def __init__(self):
        self.to_k = _FakeProj()
        self.to_v = _FakeProj()


class _Block(nn.Module):
    def __init__(self):
        super().__init__()
        self.attn2 = _FakeAttn2()


class _Stage(nn.Module):
    def __init__(self):
        super().__init__()
        self.block = _Block()


class _FakeDiffusionModel:
    def __init__(self):
        self.input_blocks = [[_Stage()]]
        self.middle_block = []
        self.output_blocks = []


class _FakeModel:
    """Minimal ModelPatcher double: model_options clone independent of the original."""

    def __init__(self):
        self.model = SimpleNamespace(diffusion_model=_FakeDiffusionModel())
        self.model_options = {}

    def clone(self):
        new = _FakeModel.__new__(_FakeModel)
        new.model = self.model
        new.model_options = copy.deepcopy(self.model_options)
        return new


class _FakeClip:
    def tokenize(self, text):
        return [text]

    def encode_from_tokens_scheduled(self, tokens):
        return [[torch.zeros(1, 3, 8), {}]]


def _cond(fill, tokens=5):
    return [[torch.full((1, tokens, 8), float(fill)), {}]]


def _mask(fill):
    return torch.full((16, 16), float(fill))


def _attn2_patches(patched_model):
    to = patched_model.model_options["transformer_options"]
    return to["patches_replace"]["attn2"]


def _run_patch(patched_model):
    """Runs the first attn2 patch installed (simple batch cond)."""
    patch = next(iter(_attn2_patches(patched_model).values()))
    q = torch.zeros(1, 4, 8)
    extra = {"cond_or_uncond": [0], "original_shape": [1, 4, 16, 16], "n_heads": 1}
    return patch(q, None, None, extra)


def _regions(node, pos_1_fill, pos_2_fill):
    """Builds the same regions as SayaMultiCouple.apply (2 regions)."""
    from nodes import ConditioningCombine

    pos = ConditioningCombine().combine(
        node._masked(_cond(pos_1_fill), _mask(1), 1.0),
        node._masked(_cond(pos_2_fill), _mask(0), 1.0),
    )[0]
    neg = ConditioningCombine().combine(
        node._masked(_cond(0.3), _mask(1), 1.0),
        node._masked(_cond(-0.3), _mask(0), 1.0),
    )[0]
    return pos, neg


def _load_node():
    load_pack()
    from saya_couple.src.nodes.saya_multi_couple import SayaMultiCouple

    return SayaMultiCouple


def test_multi_couple_schema_and_outputs():
    node = _load_node()()

    c = Check("multi_couple_schema_and_outputs")
    schema = node.INPUT_TYPES()
    c.eq(
        list(schema["required"]),
        ["model_1", "clip", "mask_1", "mask_2", "pos_1", "neg_1", "pos_2", "neg_2", "strength_1", "strength_2"],
        "required input order",
    )
    c.eq(list(schema["optional"]), ["model_2", "main", "solo", "dual_attention_enabled"], "model_2/main/solo/dual optional")
    c.eq(node.RETURN_TYPES, ("MODEL", "MODEL", "CONDITIONING", "CONDITIONING"), "return types")
    c.eq(node.RETURN_NAMES, ("MODEL_1_PATCHED", "MODEL_2_PATCHED", "CONDITIONING", "NEGATIVE"), "return names")

    model_1 = _FakeModel()
    neg_global = _cond(0.3)
    m1p, m2p, coupled_pos, negative = node.apply(
        model_1, _FakeClip(), _mask(1), _mask(0), _cond(1.0), neg_global, _cond(2.0), _cond(-0.3),
        strength_1=1.0, strength_2=1.0, model_2=None,
    )
    c.ok(m2p is None, "model_2 absent -> MODEL_2_PATCHED None")
    c.ok(m1p is not model_1, "original model not patched (clone)")
    c.ok("transformer_options" not in model_1.model_options, "original model_options intact")
    c.ok(len(_attn2_patches(m1p)) == 1, "attn2 patch installed on the clone")
    c.ok(isinstance(coupled_pos, list) and len(coupled_pos) == 1, "coupled CONDITIONING present")
    c.eq(tuple(coupled_pos[0][0].shape), (1, 3, 8), "coupled conditioning shape (empty encode)")
    c.ok(negative is neg_global, "NEGATIVE = caller's global negative, unchanged")
    return c.report()


def test_multi_couple_patch_independence():
    node = _load_node()()
    clip = _FakeClip()

    c = Check("multi_couple_patch_independence")
    # Two separate applies, irreconcilable positive conditionings (+1 vs -1).
    model_a, model_b = _FakeModel(), _FakeModel()
    ma, _, _, _ = node.apply(model_a, clip, _mask(1), _mask(0), _cond(1.0), _cond(0.3), _cond(2.0), _cond(-0.3), model_2=None)
    mb, _, _, _ = node.apply(model_b, clip, _mask(1), _mask(0), _cond(-1.0), _cond(0.3), _cond(-2.0), _cond(-0.3), model_2=None)

    patches_a = _attn2_patches(ma)
    patches_b = _attn2_patches(mb)
    c.ok(patches_a is not patches_b, "distinct patches_replace between clones")
    c.ok(
        all(pa is not pb for pa, pb in zip(patches_a.values(), patches_b.values())),
        "distinct patch functions (independent closures)",
    )

    # Run AFTER both couplings: A must stay A, B must stay B.
    out_a = _run_patch(ma)
    out_b = _run_patch(mb)
    out_a2 = _run_patch(ma)
    c.ok(float(out_a.mean()) > 0.5, f"model A patch observes +1 (got {float(out_a.mean()):+.3f})")
    c.ok(float(out_b.mean()) < -0.5, f"model B patch observes -1 (got {float(out_b.mean()):+.3f})")
    c.ok(float(out_a2.mean()) > 0.5, "re-running patch A is still +1 (no leak)")
    return c.report()


def test_multi_couple_main_conditioning_merge():
    """MAIN must be present in both regional CONDITIONING lanes."""
    node = _load_node()()
    c = Check("multi_couple_main_conditioning_merge")

    # Capture the conditioning immediately before AttentionCouple patches the
    # model: this verifies the real merge + mask path, not a mock API shape.
    captured = {}
    def capture(model, clip, positive, negative):
        captured["positive"] = positive
        return model, positive, negative
    node._couple = capture

    node.apply(
        _FakeModel(), _FakeClip(), _mask(1), _mask(0),
        _cond(1.0), _cond(0.3), _cond(2.0), _cond(-0.3),
        model_2=None, main=_cond(9.0),
    )
    fills = [float(item[0].mean()) for item in captured["positive"]]
    c.eq(fills, [9.0, 1.0, 9.0, 2.0], "MAIN + P1 / MAIN + P2 before masks")
    weights = [round(item[1]["mask_strength"], 6) for item in captured["positive"]]
    from saya_couple.src.nodes.couple_imprint_v2 import DEFAULT_ATTENTION_PARAMS as P
    base, person = round(P["base_weight"], 6), round(P["person_weight"], 6)
    c.eq(weights, [base, person, base, person], "MAIN weighted as base, persons as person (like Phase 2+)")
    return c.report()


def test_multi_couple_positive_carries_main():
    """The sampler positive is MAIN (for the SDXL pooled vector), not the empty placeholder."""
    node = _load_node()()
    c = Check("multi_couple_positive_carries_main")
    main = _cond(9.0)
    node._couple = lambda model, clip, positive, negative: (model, [[None, {}]], negative)
    _, _, positive, _ = node.apply(
        _FakeModel(), _FakeClip(), _mask(1), _mask(0),
        _cond(1.0), _cond(0.3), _cond(2.0), _cond(-0.3), model_2=None, main=main,
    )
    c.ok(positive is main, "positive is MAIN")
    return c.report()


def test_multi_couple_shared_instance_negative_control():
    """Negative control: a shared AttentionCouple instance MUST leak.

    Proves that the independence test above really discriminates the shared
    state bug (raw_positive/raw_negative overwritten on the second call).
    """
    load_pack()
    from custom_nodes.MultiMaskCouple.attention_couple import AttentionCouple

    c = Check("multi_couple_shared_instance_negative_control")
    node = _load_node()()
    clip = _FakeClip()
    shared = AttentionCouple()

    model_a, model_b = _FakeModel(), _FakeModel()
    pos_a, neg_a = _regions(node, 1.0, 2.0)
    ma, _, _ = shared.attention_couple(model=model_a, clip=clip, positive=pos_a, negative=neg_a, mode="Attention")
    pos_b, neg_b = _regions(node, -1.0, -2.0)
    mb, _, _ = shared.attention_couple(model=model_b, clip=clip, positive=pos_b, negative=neg_b, mode="Attention")

    out_a = _run_patch(ma)
    c.ok(
        float(out_a.mean()) < -0.5,
        f"shared instance: A's patch observes B (leak expected, got {float(out_a.mean()):+.3f})",
    )
    return c.report()


def test_multi_couple_zero_ppm_guard():
    c = Check("multi_couple_zero_ppm_guard")
    src = NODE_PATH.read_text(encoding="utf-8")
    c.ok("ppm" not in src.lower(), "no 'ppm' reference in saya_multi_couple.py")
    c.ok(
        re.search(r"^from custom_nodes\.MultiMaskCouple\.attention_couple import AttentionCouple$", src, re.M)
        is not None,
        "single import = MultiMaskCouple library",
    )
    c.ok("SayaAttentionCouplePPM" not in src, "no reuse of the pack's old PPM code")

    node = _load_node()()
    import saya_couple.src.nodes.saya_multi_couple as module

    c.ok(Path(module.__file__) == NODE_PATH, "module resolved from the Saya pack")
    dep_mod = sys.modules.get(module.AttentionCouple.__module__)
    dep_file = getattr(dep_mod, "__file__", "") or ""
    c.ok("MultiMaskCouple" in dep_file and dep_file.endswith("attention_couple.py"),
         f"AttentionCouple comes from the MultiMaskCouple pack ({dep_file})")
    return c.report()


def test_multi_couple_solo():
    """Solo returns both models unpatched and MAIN + pos_1 only; pos_2 never participates."""
    node = _load_node()()
    clip = _FakeClip()
    c = Check("multi_couple_solo")
    model_1, model_2 = _FakeModel(), _FakeModel()
    neg_1 = _cond(0.3)
    m1, m2, positive, negative = node.apply(
        model_1, clip, _mask(1), _mask(0), _cond(1.0), neg_1, _cond(2.0), _cond(-0.3),
        model_2=model_2, main=_cond(9.0), solo=True,
    )
    c.ok(m1 is model_1 and m2 is model_2, "models returned unpatched")
    fills = sorted(float(entry[0].mean()) for entry in positive)
    c.eq(fills, [1.0, 9.0], "positive = MAIN + pos_1 only (pos_2 absent)")
    c.ok(negative is neg_1, "negative = neg_1")
    return c.report()


TESTS = (
    test_multi_couple_schema_and_outputs,
    test_multi_couple_main_conditioning_merge,
    test_multi_couple_patch_independence,
    test_multi_couple_shared_instance_negative_control,
    test_multi_couple_zero_ppm_guard,
    test_multi_couple_solo,
    test_multi_couple_positive_carries_main,
)

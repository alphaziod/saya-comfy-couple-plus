"""MultiMaskCouple core and SayaMultiCouple: patch independence, outputs, zero-PPM guard.

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
    # activations_shape: set by ComfyUI's SpatialTransformer for every block (4 tokens = a 2x2 grid).
    extra = {"cond_or_uncond": [0], "original_shape": [1, 4, 16, 16], "activations_shape": [1, 320, 2, 2], "n_heads": 1}
    return patch(q, None, None, extra)


def _regions(module, pos_1_fill, pos_2_fill):
    """Builds the same two regions as apply_multimask_couple without MAIN."""
    from nodes import ConditioningCombine

    pos = ConditioningCombine().combine(
        module._masked_cond(_cond(pos_1_fill), _mask(1), 1.0),
        module._masked_cond(_cond(pos_2_fill), _mask(0), 1.0),
    )[0]
    neg = ConditioningCombine().combine(
        module._masked_cond(_cond(0.3), _mask(1), 1.0),
        module._masked_cond(_cond(-0.3), _mask(0), 1.0),
    )[0]
    return pos, neg


def _module():
    load_pack()
    import saya_couple.src.nodes.saya_multi_couple as module

    return module


def _load_node():
    return _module().SayaMultiCouple


def _couple(module, model, pos_1, pos_2, main=None, **kwargs):
    """apply_multimask_couple with the Phase 1 weights, as Sampler 2 and the reconstruct call it."""
    return module.apply_multimask_couple(
        model, _FakeClip(), _mask(1), _mask(0), pos_1, _cond(0.3), pos_2, _cond(-0.3),
        1.0, 1.0, 0.65, 0.35, main=main, **kwargs,
    )


def test_multi_couple_schema_and_outputs():
    node = _load_node()()

    c = Check("multi_couple_schema_and_outputs")
    schema = node.INPUT_TYPES()
    c.eq(
        list(schema["required"]),
        ["model_1", "clip", "mask_1", "mask_2", "pos_1", "neg_1", "pos_2", "neg_2", "strength_1", "strength_2"],
        "required input order",
    )
    c.eq(list(schema["optional"]), ["model_2", "main", "action", "solo", "ownership", "dynamic_start_sigma", "background_main", "zone_fallback", "anchor_tokens", "p1_anchors", "p2_anchors", "p1_text", "p2_text", "person_anchor"], "model_2/main/solo/ownership/dynamic_start_sigma/anchor_tokens/anchors/texts/person_anchor optional (new entries at the end only)")
    c.eq(schema["optional"]["anchor_tokens"][1]["default"], "phrase", "anchor_tokens defaults to the validated phrase mode")
    c.eq(schema["optional"]["ownership"][1]["default"], "static_split", "ownership defaults to the historic static split")
    c.eq(node.RETURN_TYPES, ("MODEL", "MODEL", "CONDITIONING", "CONDITIONING", "SAYA_COUPLE_RECIPE"), "return types")
    c.eq(node.RETURN_NAMES, ("MODEL_1_PATCHED", "MODEL_2_PATCHED", "CONDITIONING", "NEGATIVE", "COUPLE_RECIPE"), "return names")

    model = _FakeModel()
    patched = _couple(_module(), model, _cond(1.0), _cond(2.0))
    c.ok(patched is not model, "original model not patched (clone)")
    c.ok("transformer_options" not in model.model_options, "original model_options intact")
    c.ok(len(_attn2_patches(patched)) == 1, "attn2 patch installed on the clone")
    return c.report()


def test_multi_couple_patch_independence():
    module = _module()

    c = Check("multi_couple_patch_independence")
    # Two separate couplings, irreconcilable positive conditionings (+1 vs -1).
    ma = _couple(module, _FakeModel(), _cond(1.0), _cond(2.0))
    mb = _couple(module, _FakeModel(), _cond(-1.0), _cond(-2.0))

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
    c = Check("multi_couple_main_conditioning_merge")

    # Capture the conditioning immediately before AttentionCouple patches the
    # model: this verifies the real merge + mask path, not a mock API shape.
    captured = {}
    def capture(model, clip, positive, negative):
        captured["positive"] = positive
        return model, positive, negative

    _couple(_module(), _FakeModel(), _cond(1.0), _cond(2.0), main=_cond(9.0), couple_fn=capture)
    fills = [float(item[0].mean()) for item in captured["positive"]]
    c.eq(fills, [9.0, 1.0, 9.0, 2.0], "MAIN + P1 / MAIN + P2 before masks")
    weights = [round(item[1]["mask_strength"], 6) for item in captured["positive"]]
    c.eq(weights, [0.65, 0.35, 0.65, 0.35], "MAIN weighted as base, persons as person (DEFAULT_ATTENTION_PARAMS)")
    return c.report()


def test_multi_couple_shared_instance_negative_control():
    """Negative control: a shared AttentionCouple instance MUST leak.

    Proves that the independence test above really discriminates the shared
    state bug (raw_positive/raw_negative overwritten on the second call).
    """
    load_pack()
    from custom_nodes.MultiMaskCouple.attention_couple import AttentionCouple

    c = Check("multi_couple_shared_instance_negative_control")
    module = _module()
    clip = _FakeClip()
    shared = AttentionCouple()

    model_a, model_b = _FakeModel(), _FakeModel()
    pos_a, neg_a = _regions(module, 1.0, 2.0)
    ma, _, _ = shared.attention_couple(model=model_a, clip=clip, positive=pos_a, negative=neg_a, mode="Attention")
    pos_b, neg_b = _regions(module, -1.0, -2.0)
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
        len(re.findall(r"^\s*from custom_nodes\.MultiMaskCouple\.attention_couple import AttentionCouple$", src, re.M)) == 1,
        "single import = MultiMaskCouple library (lazy, inside _multimask_couple, since the Fable audit)",
    )
    c.ok("SayaAttentionCouplePPM" not in src, "no reuse of the pack's old PPM code")

    module = _module()

    c.ok(Path(module.__file__) == NODE_PATH, "module resolved from the Saya pack")
    dep_mod = sys.modules.get(module._multimask_couple().__module__)
    dep_file = getattr(dep_mod, "__file__", "") or ""
    c.ok("MultiMaskCouple" in dep_file and dep_file.endswith("attention_couple.py"),
         f"AttentionCouple comes from the MultiMaskCouple pack ({dep_file})")
    return c.report()


def test_multi_couple_solo():
    """Solo returns both models unpatched and one conditioning MAIN ++ ACTION ++ pos_1; pos_2 never participates."""
    node = _load_node()()
    clip = _FakeClip()
    c = Check("multi_couple_solo")
    model_1, model_2 = _FakeModel(), _FakeModel()
    neg_1 = _cond(0.3)
    m1, m2, positive, negative, _recipe = node.apply(
        model_1, clip, _mask(1), _mask(0), _cond(1.0), neg_1, _cond(2.0), _cond(-0.3),
        model_2=model_2, main=_cond(9.0), solo=True,
    )
    c.ok(m1 is model_1 and m2 is model_2, "models returned unpatched")
    c.eq(len(positive), 1, "solo: one conditioning, never a Combine (which averages MAIN alone and P1)")
    c.eq((tuple(positive[0][0].shape), round(float(positive[0][0].mean()), 3)), ((1, 10, 8), 5.0), "positive = MAIN ++ pos_1 (pos_2 absent)")
    c.ok(negative is neg_1, "negative = neg_1")

    # ACTION in Solo: concatenated to MAIN (as for the persons in Couple), so the pose is not lost in Phase 1.
    m1, m2, positive, negative, _recipe = node.apply(
        model_1, clip, _mask(1), _mask(0), _cond(1.0), neg_1, _cond(2.0), _cond(-0.3),
        model_2=model_2, main=_cond(9.0), action=_cond(4.0, tokens=3), solo=True,
    )
    c.ok(m1 is model_1 and m2 is model_2, "solo + action: models still unpatched")
    c.eq(len(positive), 1, "solo + action: one conditioning")
    c.eq((tuple(positive[0][0].shape), round(float(positive[0][0].mean()), 3)), ((1, 13, 8), round((9.0 * 5 + 4.0 * 3 + 1.0 * 5) / 13, 3)),
         "solo + action: positive = MAIN ++ ACTION ++ pos_1, same order as the rebuilt phases (pos_2 absent)")
    c.ok(negative is neg_1, "solo + action: negative = neg_1")

    return c.report()


TESTS = (
    test_multi_couple_schema_and_outputs,
    test_multi_couple_main_conditioning_merge,
    test_multi_couple_patch_independence,
    test_multi_couple_shared_instance_negative_control,
    test_multi_couple_zero_ppm_guard,
    test_multi_couple_solo,
)

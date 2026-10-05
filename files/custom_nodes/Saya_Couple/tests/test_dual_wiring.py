"""SayaMultiCouple -> engine wiring (object patches, M1): Couple is always dual, Solo never, exact payload, fail-closed."""

import ast
import copy
from types import SimpleNamespace

import torch
from torch import nn

from harness import PACK_ROOT, Check, load_pack
from test_multi_couple import _FakeClip, _FakeModel as _OldFakeModel, _attn2_patches, _cond, _mask, _run_patch

DIM, HEADS, DHEAD, CTX = 16, 2, 8, 8
NODES = PACK_ROOT / "src" / "nodes"


class _Holder(nn.Module):
    def __init__(self, block):
        super().__init__()
        self.transformer_blocks = nn.ModuleList([block])


class _RawModel:
    """ModelPatcher double: real BasicTransformerBlock modules, ModelPatcher-like clone()."""

    def __init__(self, block_class=None):
        load_pack()
        import comfy.ldm.modules.attention as attention
        import comfy.ops

        from comfy.ldm.modules.diffusionmodules.openaimodel import UNetModel

        unet = UNetModel(image_size=8, in_channels=4, model_channels=32, out_channels=4, num_res_blocks=[1, 1], dropout=0, channel_mult=(1, 2),
                         use_spatial_transformer=True, transformer_depth=[0, 1], transformer_depth_output=[0, 0, 1, 1], transformer_depth_middle=1,
                         context_dim=CTX, num_head_channels=8, use_linear_in_transformer=True, operations=comfy.ops.disable_weight_init)
        if block_class is not None:
            for module in unet.modules():
                if isinstance(module, attention.BasicTransformerBlock):
                    module.__class__ = block_class
        self.model = SimpleNamespace(diffusion_model=unet)
        self.model_options = {"transformer_options": {}}
        self.object_patches = {}
        self.object_patches_backup = {}
        self.callbacks = {}
        self.wrappers = {}

    def clone(self):
        new = _RawModel.__new__(_RawModel)
        new.model = self.model
        new.model_options = copy.deepcopy(self.model_options)
        new.object_patches = dict(self.object_patches)
        new.object_patches_backup = {}
        new.callbacks = {k: {k1: list(v1) for k1, v1 in v.items()} for k, v in self.callbacks.items()}
        new.wrappers = copy.deepcopy(self.wrappers)
        return new

    def add_object_patch(self, name, obj):
        self.object_patches[name] = obj

    def add_callback(self, call_type, callback):
        self.callbacks.setdefault(call_type, {}).setdefault(None, []).append(callback)

    def blocks(self):
        return [m for m in self.model.diffusion_model.modules() if hasattr(m, "attn2") and hasattr(m, "norm2")]


def _node():
    load_pack()
    import saya_couple.src.nodes.saya_multi_couple as module
    return module, module.SayaMultiCouple()


def _inputs():
    return dict(clip=_FakeClip(), mask_1=_mask(1), mask_2=_mask(0), pos_1=_cond(1.0), neg_1=_cond(0.3), pos_2=_cond(2.0), neg_2=_cond(-0.3), main=_cond(9.0))


def _leaves(obj):
    if isinstance(obj, dict):
        for k, v in obj.items():
            yield from _leaves(k)
            yield from _leaves(v)
    else:
        yield obj


def test_solo_never_reaches_dual():
    module, node = _node()
    c = Check("solo_never_reaches_dual")
    called = []
    original = module.enable_dual_attention
    module.enable_dual_attention = lambda *a, **k: called.append(1)
    try:
        model_1, model_2 = _RawModel(), _OldFakeModel()
        m1, m2, _positive, _negative, _recipe = node.apply(model_1, model_2=model_2, solo=True, **_inputs())
    finally:
        module.enable_dual_attention = original
    c.eq(called, [], "Solo never reaches the dual engine")
    c.ok(m1 is model_1 and m2 is model_2, "Solo returns both models unpatched")
    return c.report()


def test_dual_on_positive_flag_payload():
    module, node = _node()
    c = Check("dual_on_positive_flag_payload")
    args = _inputs()
    model = _RawModel()
    m1, m2, positive, negative, _recipe = node.apply(model, **args)
    c.ok(positive is args["main"], "positive for Sampler 1 is exactly MAIN")
    c.ok(negative is args["neg_1"], "NEGATIVE unchanged")
    c.ok(m2 is None, "no model_2 -> MODEL_2_PATCHED None")
    options = m1.model_options["transformer_options"]
    c.ok("saya_dual_mode" not in options, "MODEL_1 carries no core flag (object-patch injection)")
    copies = [v for k, v in m1.object_patches.items() if k.startswith("diffusion_model.")]
    c.ok(len(copies) == len(model.blocks()) > 0 and all(getattr(v, "saya_source", None) for v in copies), "one Saya block copy per cross-attention block")
    c.ok(all(v.saya_source[0] is b for v, b in zip(copies, model.blocks())), "each copy wraps its own stock block")
    c.ok(m1.callbacks.get("on_detach_after"), "ON_DETACH restore callback registered")
    c.ok("saya_dual_mode" not in model.model_options["transformer_options"] and "saya_dual" not in model.model_options["transformer_options"]
         and not model.object_patches and not model.callbacks, "raw model untouched (clone)")
    c.ok("patches_replace" not in options and "patches" not in options, "no historic couple patch on MODEL_1")
    payload = options["saya_dual"]
    c.eq(set(payload), {"p1", "p2", "mask_1", "mask_2", "fusion_mode", "params"}, "payload keys are exactly the contract")
    c.eq(payload["fusion_mode"], "main_locked_delta", "fusion_mode")
    c.eq(payload["params"], {}, "params empty")
    c.ok(payload["p1"] is args["pos_1"][0][0] and payload["p2"] is args["pos_2"][0][0], "P1/P2 are the independent CLIP conditionings")
    c.ok(payload["mask_1"] is args["mask_1"] and payload["mask_2"] is args["mask_2"], "masks transported untouched (swap/geometry as given)")
    c.ok(not any(callable(v) for v in _leaves(options["saya_dual"])), "no callable anywhere in the payload")
    ctx = {id(payload["p1"]), id(payload["p2"]), id(args["main"][0][0])}
    ptrs = {payload["p1"].data_ptr(), payload["p2"].data_ptr(), args["main"][0][0].data_ptr()}
    c.eq((len(ctx), len(ptrs)), (3, 3), "MAIN, P1, P2 are three separate tensors")
    return c.report()


def test_dual_on_builds_no_historic_region_for_model_1():
    module, node = _node()
    c = Check("dual_on_builds_no_historic_region_for_model_1")
    trace = []
    masked, couple = module._masked_cond, module._attention_couple_patch
    module._masked_cond = lambda *a, **k: trace.append("masked")
    module._attention_couple_patch = lambda *a, **k: trace.append("couple")
    try:
        node.apply(_RawModel(), **_inputs())
    finally:
        module._masked_cond, module._attention_couple_patch = masked, couple
    c.eq(trace, [], "no ConditioningSetMask region and no AttentionCouple for MODEL_1")
    return c.report()


def test_dual_on_model_2_is_historic():
    module, node = _node()
    c = Check("dual_on_model_2_is_historic")
    args = _inputs()
    reference = module.apply_multimask_couple(
        _OldFakeModel(), args["clip"], args["mask_1"], args["mask_2"], args["pos_1"], args["neg_1"],
        args["pos_2"], args["neg_2"], 1.0, 1.0, 0.65, 0.35, main=args["main"],
    )
    m1, m2_on, positive, negative, _recipe = node.apply(_RawModel(), model_2=_OldFakeModel(), **args)
    c.eq(len(_attn2_patches(m2_on)), 1, "MODEL_2 has the MultiMaskCouple attn2 replace")
    c.ok(torch.equal(_run_patch(m2_on), _run_patch(reference)), "MODEL_2 patch output = apply_multimask_couple")
    c.ok("saya_dual" not in m2_on.model_options["transformer_options"] and not getattr(m2_on, "object_patches", {}), "MODEL_2 has no engine payload nor object patch")
    c.ok(positive is args["main"] and negative is args["neg_1"], "other outputs unchanged")
    return c.report()


def test_dual_fail_closed_inputs():
    module, node = _node()
    c = Check("dual_fail_closed_inputs")

    def run(label, **override):
        args = _inputs()
        args.update(override)
        c.raises(RuntimeError, lambda: node.apply(_RawModel(), **args), label)

    run("main missing", main=None)
    run("pos_1 missing", pos_1=None)
    run("pos_2 missing", pos_2=None)
    run("mask_1 missing", mask_1=None)
    run("mask_2 wrong type", mask_2=[1, 2])
    run("mask wrong ndim", mask_1=torch.zeros(4))
    run("pos_1 with two entries", pos_1=_cond(1.0) + _cond(2.0))
    run("pos_2 not 3-D", pos_2=[[torch.zeros(5, 8), {}]])
    args = _inputs(); args["pos_1"] = args["main"]
    c.raises(RuntimeError, lambda: node.apply(_RawModel(), **args), "pos_1 is MAIN itself (not separate)")
    args = _inputs(); args["pos_2"] = args["pos_1"]
    c.raises(RuntimeError, lambda: node.apply(_RawModel(), **args), "pos_2 is pos_1 itself (not separate)")
    c.raises(RuntimeError, lambda: node.apply(None, **_inputs()), "MODEL_1 missing")
    return c.report()


def test_dual_fail_closed_model():
    module, node = _node()
    c = Check("dual_fail_closed_model")

    def refused(model, label, needle=None):
        try:
            node.apply(model, **_inputs())
        except RuntimeError as error:
            c.ok(needle is None or needle in str(error), f"{label}: message {str(error)!r} lacks {needle!r}")
            return
        c.failures.append(f"{label}: no RuntimeError")

    m = _RawModel(); m.object_patches["diffusion_model.input_blocks.1.1.transformer_blocks.0.__class__"] = object
    refused(m, "object patch __class__ (ReSDPatcher style)", "__class__")
    m = _RawModel(); m.object_patches["diffusion_model.__class__"] = object
    refused(m, "object patch on diffusion_model.__class__", "__class__")
    import comfy.ldm.modules.attention as attention

    class ReBlock(attention.BasicTransformerBlock):
        pass
    refused(_RawModel(block_class=ReBlock), "block class already replaced", "ReBlock")
    empty = _RawModel()
    for module in empty.model.diffusion_model.modules():
        if hasattr(module, "attn2") and hasattr(module, "norm2"):
            del module.norm2
    refused(empty, "no transformer block", "no cross-attention")

    m = _RawModel(); m.model_options["transformer_options"]["patches_replace"] = {"attn2": {("middle", 0, 0): lambda *a: None}}
    refused(m, "historic attn2 replace already present", "attn2 patches_replace")
    m = _RawModel(); m.model_options["transformer_options"]["patches"] = {"attn2_patch": [lambda *a: None]}
    refused(m, "attn2_patch already present", "attn2_patch")
    m = _RawModel(); m.model_options["transformer_options"]["patches"] = {"attn2_output_patch": [lambda *a: None]}
    refused(m, "attn2_output_patch already present", "attn2_output_patch")

    # a real historic AttentionCouple already installed on the model
    from custom_nodes.MultiMaskCouple.attention_couple import AttentionCouple
    m = _RawModel()
    coupled, _, _ = AttentionCouple().attention_couple(model=m, clip=_FakeClip(), positive=_cond(1.0), negative=_cond(0.3), mode="Attention")
    c.ok(bool(coupled.model_options["transformer_options"]["patches_replace"]["attn2"]), "precondition: real AttentionCouple installed patches")
    refused(coupled, "MODEL_1 that already received the historic AttentionCouple", "historic couple")

    m = _RawModel(); m.model_options["transformer_options"]["saya_dual"] = {}
    refused(m, "already carries the payload", "already carries")
    from saya_couple.src.nodes.saya_dual_attention import saya_block, saya_block_names
    m = _RawModel(); first, first_block = saya_block_names(m)[0]
    m.add_object_patch(first, object())
    refused(m, f"another pack's object patch on a transformer block ({first})", "another object patch")
    m = _RawModel(); m.add_object_patch(first, saya_block(first_block))
    refused(m, "already carries a Saya block copy", "already carries")
    m = _RawModel(); m.model_options["transformer_options"]["optimized_attention_override"] = lambda *a: None
    refused(m, "optimized_attention_override", "optimized_attention_override")

    # wrappers: potentially able to rewrite context / transformer_options -> refused
    m = _RawModel(); m.model_options["model_function_wrapper"] = lambda *a: None
    refused(m, "model_function_wrapper", "model_function_wrapper")
    m = _RawModel(); m.model_options["sampler_calc_cond_batch_function"] = lambda *a: None
    refused(m, "sampler_calc_cond_batch_function", "sampler_calc_cond_batch_function")
    for kind in ("diffusion_model", "apply_model", "calc_cond_batch", "predict_noise", "outer_sample", "sampler_sample", "prepare_sampling"):
        m = _RawModel(); m.wrappers = {kind: {"some.key": [lambda *a: None]}}
        refused(m, f"WrappersMP.{kind.upper()} on the patcher", kind)
        m = _RawModel(); m.model_options["transformer_options"]["wrappers"] = {kind: {None: [lambda *a: None]}}
        refused(m, f"WrappersMP.{kind.upper()} in model_options", kind)

    # orthogonal: empty wrapper containers and prediction-side sampler functions stay allowed
    m = _RawModel()
    m.wrappers = {"diffusion_model": {}, "apply_model": {"k": []}}
    m.model_options.update(sampler_post_cfg_function=[lambda a: a], sampler_pre_cfg_function=[lambda a: a], sampler_cfg_function=lambda a: a)
    out = node.apply(m, **_inputs())
    c.ok("saya_dual" in out[0].model_options["transformer_options"] and out[0].object_patches, "empty wrappers and sampler_*_cfg functions (APG/CFGZeroStar/Epsilon/PAG) allowed")
    return c.report()


def test_dual_no_legacy_fallback_in_wiring():
    module, node = _node()
    c = Check("dual_no_legacy_fallback_in_wiring")
    tree = ast.parse((NODES / "saya_dual_attention.py").read_text())
    c.eq([n for n in ast.walk(tree) if isinstance(n, ast.Try)], [], "no try/except in the dual wiring module")
    text = (NODES / "saya_dual_attention.py").read_text()
    c.ok("AttentionCouple" not in text and "ConditioningSetMask" not in text and "_couple" not in text, "dual wiring never references the historic couple")
    apply = next(n for n in ast.walk(ast.parse((NODES / "saya_multi_couple.py").read_text())) if isinstance(n, ast.FunctionDef) and n.name == "apply")
    c.eq([n for n in ast.walk(apply) if isinstance(n, ast.Try)], [], "no try/except in SayaMultiCouple.apply")
    return c.report()


TESTS = (
    test_solo_never_reaches_dual,
    test_dual_on_positive_flag_payload,
    test_dual_on_builds_no_historic_region_for_model_1,
    test_dual_on_model_2_is_historic,
    test_dual_fail_closed_inputs,
    test_dual_fail_closed_model,
    test_dual_no_legacy_fallback_in_wiring,
)

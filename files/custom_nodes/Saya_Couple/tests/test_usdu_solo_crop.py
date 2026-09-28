"""SayaCoupleUSDUPass: the couple crop follows the global Couple/Solo mode only.

Old API prompts may still carry ``couple_crop``; they go through ComfyUI's real
``execution.get_input_data`` here, exactly like at runtime.
"""

from __future__ import annotations

import os

from harness import Check, load_pack

ENV = "SAYA_USDU_COUPLE_CROP"


def _node():
    load_pack()
    from saya_couple.src.duo_geometry import usdu_pass_node

    return usdu_pass_node


class _FakeModel:
    def clone(self):
        return self


def _run(module, legacy_couple_crop, solo, masks=True):
    """Run the node from an API-style input dict; returns the crop gate seen by the engine."""
    import torch
    from execution import get_input_data

    seen = {}

    class _Engine:
        def upscale(self, **kwargs):
            seen["crop"] = os.environ.get(ENV) == "1"
            return (kwargs["image"],)

    image = torch.zeros(1, 64, 64, 3)
    inputs = {
        "image": image, "model": _FakeModel(), "positive": [], "negative": [], "vae": object(),
        "seed": 0, "pass_id": "usdu_1", "upscale_by": 1.0, "tile_size": 512, "padding": 32,
        "mask_blur": 8, "cfg": 1.0, "steps": 4, "sampler_name": "euler", "scheduler": "normal",
        "denoise": 0.1, "structure_preservation": 0.75, "restore_to_base": False, "solo": solo,
    }
    if legacy_couple_crop is not None:
        inputs["couple_crop"] = legacy_couple_crop
    if masks:
        inputs.update(mask_base=torch.ones(1, 8, 8), mask_p1=torch.ones(1, 8, 8), mask_p2=torch.zeros(1, 8, 8))
    data, _missing, _v3 = get_input_data(inputs, module.SayaCoupleUSDUPass, "1")
    kwargs = {key: value[0] for key, value in data.items()}
    original = module._resolve_upscale_node
    module._resolve_upscale_node = lambda: _Engine
    try:
        module.SayaCoupleUSDUPass().upscale(**kwargs)
    finally:
        module._resolve_upscale_node = original
    return seen["crop"], "couple_crop" in kwargs


def _with_global_env(value, fn):
    """Run ``fn`` with the gate variable exported globally (as the user's shell does) or absent."""
    previous = os.environ.get(ENV)
    if value is None:
        os.environ.pop(ENV, None)
    else:
        os.environ[ENV] = value
    try:
        result = fn()
        return result, os.environ.get(ENV)
    finally:
        if previous is None:
            os.environ.pop(ENV, None)
        else:
            os.environ[ENV] = previous


def test_couple_forces_crop():
    module = _node()
    c = Check("usdu_couple_forces_crop")
    for global_env in (None, "1", "0"):
        for legacy in (None, False, True):
            (crop, leaked), after = _with_global_env(global_env, lambda: _run(module, legacy, solo=False))
            c.eq(crop, True, f"Couple: crop ON (global env={global_env}, legacy couple_crop={legacy})")
            c.eq(leaked, False, "legacy couple_crop never reaches the node")
            c.eq(after, global_env, "global env restored after the call")
    return c.report()


def test_solo_disables_crop():
    module = _node()
    c = Check("usdu_solo_disables_crop")
    for global_env in (None, "1"):
        for legacy in (None, True, False):
            (crop, leaked), after = _with_global_env(global_env, lambda: _run(module, legacy, solo=True))
            c.eq(crop, False, f"Solo: crop OFF (global env={global_env}, legacy couple_crop={legacy})")
            c.eq(leaked, False, "legacy couple_crop never reaches the node")
            c.eq(after, global_env, "global env restored after the call")
    (crop, _), _ = _with_global_env("1", lambda: _run(module, None, solo=True, masks=False))
    c.eq(crop, False, "Solo needs no couple masks")
    return c.report()


def test_couple_needs_masks():
    module = _node()
    c = Check("usdu_couple_needs_masks")
    c.raises(module.SayaUSDUConfigError, lambda: _run(module, None, solo=False, masks=False),
             "Couple without masks is a hard error")
    return c.report()


def test_signature():
    module = _node()
    c = Check("usdu_solo_signature")
    types = module.SayaCoupleUSDUPass.INPUT_TYPES()
    c.ok("couple_crop" not in types["required"] and "couple_crop" not in types["optional"],
         "no couple_crop control left")
    c.eq(types["required"]["solo"], ("BOOLEAN", types["required"]["solo"][1]), "solo is a required BOOLEAN")
    c.eq(types["required"]["solo"][1].get("forceInput"), True, "solo is a link, never a widget")
    widgets = [name for name, spec in types["required"].items()
               if isinstance(spec[0], list) or spec[0] in ("INT", "FLOAT", "BOOLEAN", "STRING")
               and not spec[1].get("forceInput")]
    c.eq(widgets[-2:], ["structure_preservation", "restore_to_base"],
         "widget order: restore_to_base directly follows structure_preservation (JS migration index 13)")
    return c.report()


TESTS = [
    test_couple_forces_crop,
    test_solo_disables_crop,
    test_couple_needs_masks,
    test_signature,
]

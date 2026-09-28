"""Guards for paths removed in v0.3.0: they must not come back.

* manual switches that duplicated the global Couple/Solo mode
  (``couple_crop`` on USDU, ``dual_attention_enabled`` on SayaMultiCouple);
* the three-region HiDream path (``ClownRegionalConditioning3``,
  ``conditioning_unmasked``);
* helpers that had no caller left.

The only place allowed to mention ``couple_crop`` as a user input is the
frontend shim that migrates old saved workflows.
"""

from harness import PACK_ROOT, Check, load_pack

REMOVED_INPUTS = {"couple_crop", "dual_attention_enabled"}
LEGACY_SHIM = PACK_ROOT / "web" / "saya_usdu_couple_crop_migration.js"


def test_no_removed_node_inputs_or_outputs():
    pack = load_pack()
    c = Check("removed_node_inputs_outputs")
    for name, cls in pack.NODE_CLASS_MAPPINGS.items():
        schema = cls.INPUT_TYPES()
        inputs = set(schema.get("required", {})) | set(schema.get("optional", {}))
        c.eq(inputs & REMOVED_INPUTS, set(), f"{name}: removed switch is back")
        c.ok("conditioning_unmasked" not in getattr(cls, "RETURN_NAMES", ()), f"{name}: three-region output is back")
    return c.report()


def test_no_three_region_or_manual_switch_in_runtime_code():
    c = Check("removed_runtime_references")
    files = [p for p in (PACK_ROOT / "src").rglob("*.py")] + [PACK_ROOT / "registry.py"]
    files += [p for p in (PACK_ROOT / "web").glob("*.js") if p != LEGACY_SHIM]
    for path in files:
        text = path.read_text(encoding="utf-8")
        for needle in ("ClownRegionalConditioning3", "conditioning_unmasked", "dual_attention_enabled", "_apply_dual"):
            c.ok(needle not in text, f"{path.relative_to(PACK_ROOT)} mentions removed {needle!r}")
    c.ok("couple_crop" in LEGACY_SHIM.read_text(encoding="utf-8"), "legacy shim still migrates old couple_crop values")
    return c.report()


def test_removed_helpers_are_gone():
    load_pack()
    from saya_couple.src.duo_geometry import usdu_engine_config
    from saya_couple.src.nodes import region_masks, saya_multi_couple

    c = Check("removed_helpers")
    gone = {
        region_masks: ("Orientation", "downsample_nearest", "tile_windows", "TileWindow", "mask_for_tile",
                       "mask_for_tile_work", "padded_tile_window"),
        usdu_engine_config: ("pass_defaults_usdu1", "pass_defaults_usdu2", "assert_usdu_continuation",
                             "tqdm_round_grid", "LIVE_GRID_BEHAVIOR"),
    }
    for module, names in gone.items():
        for name in names:
            c.ok(not hasattr(module, name), f"{module.__name__}.{name} is back")
    node = saya_multi_couple.SayaMultiCouple
    for name in ("_apply_dual", "_masked", "_couple"):
        c.ok(not hasattr(node, name), f"SayaMultiCouple.{name} is back")
    c.ok(not hasattr(usdu_engine_config.ConfigPass, "validated"), "ConfigPass.validated is back")
    return c.report()


TESTS = (
    test_no_removed_node_inputs_or_outputs,
    test_no_three_region_or_manual_switch_in_runtime_code,
    test_removed_helpers_are_gone,
)

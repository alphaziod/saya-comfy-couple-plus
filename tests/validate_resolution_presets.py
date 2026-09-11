"""Validate SayaResolutionScaleCalculator's presets without ComfyUI or a GPU."""

import importlib.util
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "src/nodes/saya_resolution_scale.py"
WORKFLOWS = [
    ROOT / "workflows/Ilust-Simple-V1.json",
]

RATIO_PATTERN = re.compile(r"(\d+):(\d+)")


def load_module():
    spec = importlib.util.spec_from_file_location("saya_resolution_scale", MODULE_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ResolutionPresetTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.calculator_cls = load_module().SayaResolutionScaleCalculator
        cls.presets = cls.calculator_cls.FIXED_RESOLUTION_PRESETS

    def base_kwargs(self, **overrides):
        kwargs = dict(
            resolution_preset="Landscape 16:9 · 1344x768",
            no_scale=False,
            scale_from_image=False,
            aspect_preset_when_not_image="16:9 - Landscape",
            swap_aspect_when_not_image=False,
            custom_aspect_width=16,
            custom_aspect_height=9,
            mode="WAN/LTX (Div32)",
            custom_divisor=8,
        )
        kwargs.update(overrides)
        return kwargs

    def test_no_duplicate_labels(self):
        names = list(self.presets.keys())
        self.assertEqual(len(names), len(set(names)))

    def test_every_preset_is_divisible_by_64(self):
        # Div-64 covers FLUX/SDXL (div-8) and WAN/LTX (div-32) generation grids.
        bad = [(k, v) for k, v in self.presets.items() if v[0] % 64 or v[1] % 64]
        self.assertEqual(bad, [], f"non div-64 presets: {bad}")

    def test_every_preset_dimension_is_positive(self):
        for name, (w, h) in self.presets.items():
            self.assertGreater(w, 0, name)
            self.assertGreater(h, 0, name)

    def test_aspect_preset_ratio_tokens_match_a_preset_family(self):
        # web/saya_resolution_ratio_filter.js matches the plain "A:B" text
        # embedded in aspect_preset_when_not_image against the same text
        # embedded in resolution_preset names. Every real ratio option must
        # have at least one matching preset, or the filter would empty the
        # list for that choice.
        preset_ratio_tokens = {
            RATIO_PATTERN.search(name).group(0) for name in self.presets
        }
        for aspect_name in self.calculator_cls.ASPECT_PRESETS:
            if aspect_name in ("All (no filter)", "CUSTOM"):
                continue
            match = RATIO_PATTERN.search(aspect_name)
            self.assertIsNotNone(match, aspect_name)
            with self.subTest(aspect=aspect_name):
                self.assertIn(match.group(0), preset_ratio_tokens)

    def test_no_filter_and_custom_have_no_ratio_token(self):
        # The JS side relies on these two NOT matching RATIO_PATTERN to tell
        # "show everything" (All) and "use custom_aspect_width/height"
        # (CUSTOM) apart from a real "A:B" ratio choice.
        self.assertIsNone(RATIO_PATTERN.search("All (no filter)"))
        self.assertIsNone(RATIO_PATTERN.search("CUSTOM"))

    def test_required_widget_order_is_backward_compatible(self):
        # ComfyUI stores widgets_values as a positional array in older saved
        # workflows/graphs, not a name-keyed dict. No widget may be inserted,
        # removed, or reordered here without shifting every later value in
        # those saved graphs.
        order = list(self.calculator_cls.INPUT_TYPES()["required"].keys())
        self.assertEqual(order, [
            "resolution_preset", "no_scale", "scale_from_image",
            "aspect_preset_when_not_image", "swap_aspect_when_not_image",
            "custom_aspect_width", "custom_aspect_height", "mode", "custom_divisor",
        ])

    def test_default_preset_is_selectable(self):
        input_types = self.calculator_cls.INPUT_TYPES()
        default = input_types["required"]["resolution_preset"][1]["default"]
        self.assertIn(default, self.presets)
        default_aspect = input_types["required"]["aspect_preset_when_not_image"][1]["default"]
        self.assertIn(default_aspect, self.calculator_cls.ASPECT_PRESETS)

    def test_calculate_returns_exact_preset_dimensions(self):
        calculator = self.calculator_cls()
        for name, (w, h) in self.presets.items():
            with self.subTest(preset=name):
                result = calculator.calculate(**self.base_kwargs(resolution_preset=name))
                self.assertEqual(result, (w, h, float(w), float(h)))

    def test_swap_flips_width_and_height(self):
        calculator = self.calculator_cls()
        result = calculator.calculate(**self.base_kwargs(swap_aspect_when_not_image=True))
        self.assertEqual(result, (768, 1344, 768.0, 1344.0))

    def test_saved_workflows_reference_existing_presets(self):
        pattern = re.compile(r'"resolution_preset"\s*:\s*"([^"]+)"')
        for workflow_path in WORKFLOWS:
            if not workflow_path.exists():
                continue
            text = workflow_path.read_text()
            used_presets = set(pattern.findall(text))
            for preset in used_presets:
                with self.subTest(workflow=workflow_path.name, preset=preset):
                    self.assertIn(preset, self.presets)

    def test_workflow_json_is_valid(self):
        for workflow_path in WORKFLOWS:
            if not workflow_path.exists():
                continue
            json.loads(workflow_path.read_text())


if __name__ == "__main__":
    unittest.main()

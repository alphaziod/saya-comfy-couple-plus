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

    def test_ratio_filter_families_match_preset_prefixes(self):
        families_in_names = {name.split(" · ")[0] for name in self.presets}
        self.assertEqual(families_in_names, set(self.calculator_cls.RATIO_FILTERS[1:]))
        self.assertEqual(self.calculator_cls.RATIO_FILTERS[0], "All")

    def test_new_widgets_are_appended_not_inserted(self):
        # ComfyUI stores widgets_values as a positional array in older saved
        # workflows/graphs, not a name-keyed dict. Inserting a new required
        # widget anywhere but the end shifts every later value in those saved
        # graphs (e.g. no_scale would silently receive resolution_preset's old
        # value). ratio_filter must stay last.
        order = list(self.calculator_cls.INPUT_TYPES()["required"].keys())
        legacy_order = [
            "resolution_preset", "no_scale", "scale_from_image",
            "aspect_preset_when_not_image", "swap_aspect_when_not_image",
            "custom_aspect_width", "custom_aspect_height", "mode", "custom_divisor",
        ]
        self.assertEqual(order[: len(legacy_order)], legacy_order)
        self.assertEqual(order[len(legacy_order):], ["ratio_filter"])

    def test_default_preset_is_selectable(self):
        input_types = self.calculator_cls.INPUT_TYPES()
        default = input_types["required"]["resolution_preset"][1]["default"]
        self.assertIn(default, self.presets)
        default_filter = input_types["required"]["ratio_filter"][1]["default"]
        self.assertIn(default_filter, self.calculator_cls.RATIO_FILTERS)

    def test_calculate_returns_exact_preset_dimensions(self):
        calculator = self.calculator_cls()
        for name, (w, h) in self.presets.items():
            with self.subTest(preset=name):
                result = calculator.calculate(
                    ratio_filter="All",
                    resolution_preset=name,
                    no_scale=False,
                    scale_from_image=False,
                    aspect_preset_when_not_image="16:9 - Landscape",
                    swap_aspect_when_not_image=False,
                    custom_aspect_width=16,
                    custom_aspect_height=9,
                    mode="WAN/LTX (Div32)",
                    custom_divisor=8,
                )
                self.assertEqual(result, (w, h, float(w), float(h)))

    def test_swap_flips_width_and_height(self):
        calculator = self.calculator_cls()
        result = calculator.calculate(
            ratio_filter="All",
            resolution_preset="Landscape 16:9 · 1344x768",
            no_scale=False,
            scale_from_image=False,
            aspect_preset_when_not_image="16:9 - Landscape",
            swap_aspect_when_not_image=True,
            custom_aspect_width=16,
            custom_aspect_height=9,
            mode="WAN/LTX (Div32)",
            custom_divisor=8,
        )
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

    def test_ratio_filter_json_is_valid_for_every_workflow(self):
        for workflow_path in WORKFLOWS:
            if not workflow_path.exists():
                continue
            json.loads(workflow_path.read_text())


if __name__ == "__main__":
    unittest.main()

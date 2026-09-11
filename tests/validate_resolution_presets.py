"""Validate SayaResolutionScaleCalculator's presets without ComfyUI or a GPU."""

import importlib.util
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "src/nodes/saya_resolution_scale.py"
JS_PATH = ROOT / "web/saya_resolution_ratio_filter.js"
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

    def test_no_resolution_was_added_or_changed(self):
        # Pins the exact preset set the user validated. A fix for the ratio
        # filter must never add, remove, or resize a resolution — that's a
        # frontend widget-sync bug, not a resolutions problem.
        expected = {
            "Square 1:1 · 768x768": (768, 768),
            "Square 1:1 · 1024x1024": (1024, 1024),
            "Square 1:1 · 1280x1280": (1280, 1280),
            "Landscape 4:3 · 1024x768": (1024, 768),
            "Landscape 4:3 · 1280x960": (1280, 960),
            "Landscape 4:3 · 1536x1152": (1536, 1152),
            "Portrait 3:4 · 768x1024": (768, 1024),
            "Portrait 3:4 · 960x1280": (960, 1280),
            "Portrait 3:4 · 1152x1536": (1152, 1536),
            "Landscape 3:2 · 960x640": (960, 640),
            "Landscape 3:2 · 1152x768": (1152, 768),
            "Landscape 3:2 · 1216x832": (1216, 832),
            "Landscape 3:2 · 1344x896": (1344, 896),
            "Portrait 2:3 · 640x960": (640, 960),
            "Portrait 2:3 · 768x1152": (768, 1152),
            "Portrait 2:3 · 832x1216": (832, 1216),
            "Portrait 2:3 · 896x1344": (896, 1344),
            "Landscape 16:9 · 896x512": (896, 512),
            "Landscape 16:9 · 1152x640": (1152, 640),
            "Landscape 16:9 · 1344x768": (1344, 768),
            "Landscape 16:9 · 1600x896": (1600, 896),
            "Portrait 9:16 · 512x896": (512, 896),
            "Portrait 9:16 · 640x1152": (640, 1152),
            "Portrait 9:16 · 768x1344": (768, 1344),
            "Portrait 9:16 · 896x1600": (896, 1600),
            "Ultrawide 21:9 · 1344x576": (1344, 576),
            "Ultrawide 21:9 · 1600x704": (1600, 704),
            "Ultrawide 21:9 · 1792x768": (1792, 768),
            "Ultrawide Portrait 9:21 · 576x1344": (576, 1344),
            "Ultrawide Portrait 9:21 · 704x1600": (704, 1600),
            "Ultrawide Portrait 9:21 · 768x1792": (768, 1792),
        }
        self.assertEqual(self.presets, expected)

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
        # calculate() must honour resolution_preset verbatim: the ratio
        # filter is a frontend-only concern, never a backend override. This
        # is the guard against reintroducing a backend "authoritative
        # aspect" that silently swaps the chosen resolution.
        calculator = self.calculator_cls()
        for name, (w, h) in self.presets.items():
            with self.subTest(preset=name):
                result = calculator.calculate(**self.base_kwargs(
                    resolution_preset=name,
                    # Deliberately mismatched aspect: calculate() must not
                    # care, since aspect filtering lives purely in the JS.
                    aspect_preset_when_not_image="9:21 - Ultrawide Portrait",
                ))
                self.assertEqual(result, (w, h, float(w), float(h)))

    def test_calculate_ignores_aspect_preset_for_computation(self):
        # aspect_preset_when_not_image must be deleted (unused), not read,
        # inside calculate()'s BODY -- prevents a backend ratio override
        # creeping back in. Ratio filtering belongs entirely to the frontend.
        import inspect
        calculator = self.calculator_cls()
        source = inspect.getsource(calculator.calculate)
        body = source[source.index("):") + 2:]  # drop the signature
        mentions = [
            line for line in body.splitlines()
            if "aspect_preset_when_not_image" in line
            and not line.strip().startswith("#")
            and "del " not in line
        ]
        self.assertEqual(mentions, [], f"calculate() reads aspect_preset_when_not_image: {mentions}")

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
            if workflow_path.exists():
                json.loads(workflow_path.read_text())


class RatioFilterJsTests(unittest.TestCase):
    """Structural guardrails on the JS fix -- no polling / global scanning."""

    @classmethod
    def setUpClass(cls):
        cls.js = JS_PATH.read_text()
        # Code with // line comments stripped, so prose explaining what NOT
        # to do (or citing an example preset name) can't false-positive the
        # structural checks below.
        cls.code = "\n".join(
            line for line in cls.js.splitlines() if not line.strip().startswith("//")
        )

    def test_no_polling_or_timers(self):
        for forbidden in ("setInterval(", "setTimeout("):
            with self.subTest(pattern=forbidden):
                self.assertNotIn(forbidden, self.code)

    def test_no_global_dom_observers_or_click_hooks(self):
        for forbidden in (
            "MutationObserver",
            'addEventListener("pointerdown"',
            "addEventListener('pointerdown'",
            "addEventListener(\"click\"",
            "document.addEventListener",
            "window.addEventListener",
        ):
            with self.subTest(pattern=forbidden):
                self.assertNotIn(forbidden, self.code)

    def test_no_full_graph_scanning(self):
        for forbidden in ("app.graph", "_nodes", "scanGraph"):
            with self.subTest(pattern=forbidden):
                self.assertNotIn(forbidden, self.code)

    def test_uses_the_targeted_widget_promoted_event(self):
        self.assertIn("widget-promoted", self.code)

    def test_matches_widgets_by_name_not_node_class(self):
        # The interactive combo lives on a subgraph proxy node whose type is
        # a per-workflow UUID -- there is no stable class name to hook via
        # beforeRegisterNodeDef for it.
        self.assertNotIn("beforeRegisterNodeDef", self.code)
        self.assertIn('"aspect_preset_when_not_image"', self.code)
        self.assertIn('"resolution_preset"', self.code)

    def test_no_hardcoded_duplicate_resolution_table(self):
        # No second, hand-maintained copy of the preset list that could
        # drift from src/nodes/saya_resolution_scale.py.
        self.assertNotIn("1024x1024", self.code)
        self.assertNotIn("1344x768", self.code)

    def test_direction_is_resolution_to_aspect_only(self):
        # resolution_preset is the source of truth: the fix must derive and
        # write aspect_preset_when_not_image, and must never write to
        # resolution_preset's value or filter its option list.
        self.assertIn("deriveAspectLabel", self.code)
        self.assertIn("aspectWidget.value =", self.code)
        self.assertNotIn("presetWidget.value =", self.code)
        self.assertNotIn("presetWidget.options.values =", self.code)


if __name__ == "__main__":
    unittest.main()

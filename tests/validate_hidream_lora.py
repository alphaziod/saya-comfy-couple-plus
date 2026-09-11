"""CPU integration checks. Run with ComfyUI/.venv/bin/python from ComfyUI."""

import asyncio
import importlib
import importlib.util
import os
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.argv = [sys.argv[0], "--cpu"]
ROOT = Path(__file__).resolve().parents[1]
COMFYUI = Path(os.environ.get("COMFYUI_PATH", Path.cwd())).resolve()
sys.path.insert(0, str(COMFYUI))
import comfy.options
comfy.options.enable_args_parsing()
import server
server.PromptServer(asyncio.new_event_loop())
sys.path.insert(0, str(COMFYUI / "custom_nodes"))
spec = importlib.util.spec_from_file_location("comfy_saya_couple", ROOT / "__init__.py", submodule_search_locations=[str(ROOT)])
plugin = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = plugin
spec.loader.exec_module(plugin)
import torch
import comfy.model_base
import comfy.model_patcher
import comfy_saya_couple as plugin
from comfy_saya_couple.src.nodes import couple_conditioning_hidream as hd
from comfy_saya_couple.src.nodes.hidream_lora import SayaHiDreamLoraLoader, SayaHiDreamLoraSettings
from comfy_saya_couple.src.nodes.text_and_model_routing import DualClipTextEncoderNode
from comfy_saya_couple.src.services import hidream_cache


class TinyHiDream(comfy.model_base.HiDream):
    """Small CPU fixture using the real ComfyUI patcher API."""
    def __init__(self):
        torch.nn.Module.__init__(self)
        self.model_config = SimpleNamespace(unet_config={})
        self.diffusion_model = torch.nn.Module()
        self.diffusion_model.proj = torch.nn.Linear(32, 32, bias=False)


class RecordingClip:
    def __init__(self):
        self.texts = []

    def tokenize(self, text):
        self.texts.append(text)
        return text

    def encode_from_tokens_scheduled(self, tokens):
        return [[torch.ones(1, 2, 4), {
            "pooled_output": torch.ones(1, 4),
            "conditioning_llama3": torch.ones(1, 2, 2, 4),
        }]]


class HiDreamTests(unittest.TestCase):
    def test_registry(self):
        for name in ("SayaHiDreamLoraSettings", "SayaHiDreamLoraLoader"):
            cls = plugin.NODE_CLASS_MAPPINGS[name]
            self.assertIn(name, plugin.NODE_DISPLAY_NAME_MAPPINGS)
            self.assertTrue(cls.INPUT_TYPES()["required"])
            self.assertEqual(len(cls.RETURN_TYPES), len(cls.RETURN_NAMES))

    def test_boundary_offload_is_opt_in(self):
        from comfy_saya_couple.src.nodes import image_phases as phases
        from comfy_saya_couple.src.services import image_phases as storage
        for enabled in (False, True):
            with patch.object(phases, "save_candidate"), \
                 patch.object(phases, "promote_candidate", return_value={}), \
                 patch.object(phases, "checkpoint_paths", return_value=SimpleNamespace(validated_image=Path("image.png"), validated_manifest=Path("manifest.json"))), \
                 patch.object(storage, "atomic_write_json"), \
                 patch.object(phases, "emit_phase_complete", return_value=True), \
                 patch.object(phases, "unload_everything", return_value={}) as unload:
                phases.SayaImagePhase2Stop().stop(image=[], seed=42, positive_prompt="Base",
                    negative_prompt="Negative", models_json="[]", vaes_json="[]", samplers_json="{}",
                    unload_after_phase=enabled)
                self.assertEqual(unload.call_count, int(enabled))

    def test_stock_usdu_support(self):
        import nodes
        from comfy_saya_couple.forge.usdu_bridge import SayaUSDU1IdentitySafe
        class StockUSDU:
            @classmethod
            def INPUT_TYPES(cls):return {"required":{"image":("IMAGE",)}}
            def upscale(self, **kwargs):return (kwargs,)
        with patch.dict(nodes.NODE_CLASS_MAPPINGS, {"UltimateSDUpscaleCustomSample":StockUSDU}):
            bridge = SayaUSDU1IdentitySafe()
            self.assertEqual(bridge.INPUT_TYPES()["optional"]["structure_preservation"][1]["default"],0)
            self.assertEqual(bridge.upscale(image="image", structure_preservation=0), ({"image":"image"},))
            with self.assertRaisesRegex(ValueError,"Identity Safe"):
                bridge.upscale(image="image", structure_preservation=0.75)

    def test_trigger(self):
        inject = hd.inject_hidream_trigger
        self.assertEqual(inject("Base", ""), "Base")
        self.assertEqual(inject("Base", "exampleStyle"), "exampleStyle, Base")
        self.assertEqual(inject("exampleStyle, Base", "exampleStyle"), "exampleStyle, Base")
        self.assertEqual(inject("Base, exampleStyle, detail", "exampleStyle"), "exampleStyle, Base, detail")
        self.assertEqual(inject("exampleStyleExtra", "exampleStyle"), "exampleStyle, exampleStyleExtra")

    def test_disabled_and_empty_trigger(self):
        settings = SayaHiDreamLoraSettings()
        for enabled, name, strength in [(False, "x", 1), (True, "none", 1), (True, "x", 0)]:
            cfg, trigger = settings.configure(enabled, name, strength, "style")
            self.assertFalse(cfg["enabled"])
            self.assertEqual(trigger, "")
        cfg, trigger = settings.configure(True, "x", 1, "")
        self.assertTrue(cfg["enabled"])
        self.assertEqual(trigger, "")

    def test_phase1_no_hidream_load(self):
        primary, natural = RecordingClip(), RecordingClip()
        with patch.object(hd, "_load_quad_clip", side_effect=AssertionError("Phase 1 loaded TE")):
            result = DualClipTextEncoderNode().encode(primary, "Original", naturalize_clip=natural,
                                                     naturalize_text="Natural original")
        self.assertEqual(result[1], [])
        self.assertEqual(primary.texts, ["Original"])
        self.assertEqual(natural.texts, ["Natural original"])

    def test_phase3_cache_and_negative(self):
        bundle = json.dumps(dict(base_prompt="Base", person_1_prompt="P1",
                                 person_2_prompt="P2", negative_prompt="Negative"))
        clip = RecordingClip()
        node = hd.SayaComfyCoupleHiDreamCopy()
        args = dict(main_positive=[], person_1_positive=[], person_2_positive=[], negative=[],
                    latent={"samples": torch.zeros(1, 4, 8, 8)}, couple_config={},
                    prompt_bundle_json=bundle)
        with tempfile.TemporaryDirectory() as directory, \
             patch.object(hidream_cache, "_cache_dir", return_value=directory), \
             patch.object(hd, "_load_quad_clip", return_value=clip) as load, \
             patch.object(hd, "_unload_self_loaded_clip") as unload:
            result = node.run_copy(**args, hidream_trigger="exampleStyle")
            self.assertEqual(clip.texts, ["exampleStyle, Base", "exampleStyle, P1",
                                          "exampleStyle, P2", "Negative"])
            self.assertEqual(len(result[0]), 2)
            load.assert_called_once()
            unload.assert_called_once_with(clip)
            node.run_copy(**args, hidream_trigger="exampleStyle")
            self.assertEqual(load.call_count, 1)
            node.run_copy(**args, hidream_trigger="anotherStyle")
            self.assertEqual(load.call_count, 2)
            node.run_copy(**args, hidream_trigger="")
            self.assertEqual(clip.texts[-4:], ["Base", "P1", "P2", "Negative"])
            self.assertEqual(load.call_count, 3)
            self.assertEqual(len(list(Path(directory).glob("*.pt"))), 3)
        self.assertEqual(json.loads(bundle)["base_prompt"], "Base")

    def test_native_and_gguf_patcher(self):
        gg = importlib.import_module("ComfyUI-GGUF.nodes")
        for patcher_type in (comfy.model_patcher.ModelPatcher, gg.GGUFModelPatcher):
            original = patcher_type(TinyHiDream(), torch.device("cpu"), torch.device("cpu"))
            lora = {"diffusion_model.proj.lora_A.weight": torch.ones(2, 32),
                    "diffusion_model.proj.lora_B.weight": torch.ones(32, 2)}
            cfg, _ = SayaHiDreamLoraSettings().configure(True, "fixture.safetensors", 0.5, "style")
            loader = SayaHiDreamLoraLoader()
            with patch("folder_paths.get_full_path_or_raise", return_value="fixture.safetensors"), \
                 patch("comfy.utils.load_torch_file", return_value=lora):
                result = loader.load_lora(original, cfg)[0]
            self.assertIsNot(result, original)
            self.assertIs(type(result), patcher_type)
            self.assertEqual(original.patches, {})
            self.assertEqual(len(result.patches), 1)
            weight = original.model.diffusion_model.proj.weight.detach().clone()
            patched = comfy.lora.calculate_weight(result.patches["diffusion_model.proj.weight"],
                                                  weight.clone(), "diffusion_model.proj.weight")
            torch.testing.assert_close(patched, weight + 1)
            cfg["enabled"] = False
            self.assertIs(loader.load_lora(original, cfg)[0], original)
        with self.assertRaisesRegex(ValueError, "HiDream MODEL"):
            loader.load_lora(SimpleNamespace(model=torch.nn.Linear(1, 1)), cfg)

    def test_quantized_weight_application(self):
        import gguf
        import numpy as np
        gg = importlib.import_module("ComfyUI-GGUF.nodes")
        ops = importlib.import_module("ComfyUI-GGUF.ops")
        model = TinyHiDream()
        quant = gguf.quants.quantize(np.ones((32, 32), dtype=np.float32), gguf.GGMLQuantizationType.Q8_0)
        weight = ops.GGMLTensor(torch.from_numpy(quant), tensor_type=gguf.GGMLQuantizationType.Q8_0,
                                tensor_shape=torch.Size([32, 32]))
        model.diffusion_model.proj.weight = torch.nn.Parameter(weight, requires_grad=False)
        original = gg.GGUFModelPatcher(model, torch.device("cpu"), torch.device("cpu"))
        cfg, _ = SayaHiDreamLoraSettings().configure(True, "fixture", 0.5, "style")
        lora = {"diffusion_model.proj.lora_A.weight": torch.ones(2, 32),
                "diffusion_model.proj.lora_B.weight": torch.ones(32, 2)}
        with patch("folder_paths.get_full_path_or_raise", return_value="fixture"), \
             patch("comfy.utils.load_torch_file", return_value=lora):
            patched = SayaHiDreamLoraLoader().load_lora(original, cfg)[0]
        patched.patch_weight_to_device("diffusion_model.proj.weight", device_to=torch.device("cpu"))
        actual = ops.GGMLLayer().get_weight(model.diffusion_model.proj.weight, torch.float32)
        torch.testing.assert_close(actual, torch.full((32, 32), 2.), atol=0.001, rtol=0.001)
        patched.unpatch_model()
        restored = ops.GGMLLayer().get_weight(model.diffusion_model.proj.weight, torch.float32)
        torch.testing.assert_close(restored, torch.ones(32, 32), atol=0.001, rtol=0.001)

    def test_installed_workflow(self):
        workflow = json.loads((ROOT / "workflows/Ilust-Simple-V1.json").read_text())
        graph = next(g for g in workflow["definitions"]["subgraphs"] if g["id"] == "saya-auto-phase-3")
        nodes = {n["id"]: n for n in graph["nodes"]}
        links = {l["id"]: l for l in graph["links"]}
        for link in links.values():
            self.assertIn(link["id"], nodes[link["origin_id"]]["outputs"][link["origin_slot"]]["links"])
            if link["target_id"] != -20:
                self.assertEqual(nodes[link["target_id"]]["inputs"][link["target_slot"]]["link"], link["id"])
        model_node = nodes[10143]
        for expected in ("ModelSamplingSD3", "SayaHiDreamLoraLoader", "UnetLoaderGGUF"):
            source = links[model_node["inputs"][0]["link"]]["origin_id"]
            model_node = nodes[source]
            self.assertEqual(model_node["type"], expected)
        trigger_input = next(i for i in nodes[10383]["inputs"] if i["name"] == "hidream_trigger")
        self.assertEqual(nodes[links[trigger_input["link"]]["origin_id"]]["type"], "SayaHiDreamLoraSettings")
        prompts = [n for n in workflow["nodes"] if n["type"] == "SayaDualCLIPTextEncode"]
        self.assertEqual(len(prompts), 4)
        for prompt in prompts:
            self.assertFalse(next(i for i in prompt["inputs"] if i["name"] == "hidream_clip")["link"])


if __name__ == "__main__":
    unittest.main(argv=[sys.argv[0]], verbosity=2)

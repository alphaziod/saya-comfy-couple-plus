"""Validate the public distribution graph without ComfyUI or model weights."""

import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / "workflows/Ilust-Simple-V1.json"


def strings(value):
    if isinstance(value, str):yield value
    elif isinstance(value, dict):
        for v in value.values():yield from strings(v)
    elif isinstance(value, list):
        for v in value:yield from strings(v)


class PublicWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.workflow = json.loads(WORKFLOW.read_text())
        cls.graphs = [cls.workflow, *cls.workflow["definitions"]["subgraphs"]]
        cls.nodes = [n for g in cls.graphs for n in g["nodes"]]

    def test_all_link_endpoints_and_reciprocal_references(self):
        for graph in self.graphs:
            nodes = {n["id"]: n for n in graph["nodes"]}
            self.assertEqual(len(nodes), len(graph["nodes"]))
            links = {}
            for raw in graph["links"]:
                link = raw if isinstance(raw, dict) else dict(zip(("id","origin_id","origin_slot","target_id","target_slot","type"), raw))
                self.assertNotIn(link["id"], links)
                links[link["id"]] = link
                if link["origin_id"] == -10:
                    port = graph["inputs"][link["origin_slot"]]
                    self.assertIn(link["id"], port["linkIds"])
                else:
                    port = nodes[link["origin_id"]]["outputs"][link["origin_slot"]]
                    self.assertIn(link["id"], port["links"])
                if link["target_id"] == -20:
                    port = graph["outputs"][link["target_slot"]]
                    self.assertIn(link["id"], port["linkIds"])
                else:
                    port = nodes[link["target_id"]]["inputs"][link["target_slot"]]
                    self.assertEqual(link["id"], port["link"])
            for node in nodes.values():
                for port in node["inputs"]:
                    if port.get("link") is not None:self.assertIn(port["link"], links)
                for port in node["outputs"]:
                    for link_id in port.get("links", []):self.assertIn(link_id, links)

    def test_subgraphs_and_phase_boundaries(self):
        definitions = {g["id"]: g for g in self.graphs[1:]}
        self.assertEqual(len(definitions), len(self.graphs)-1)
        for node in self.nodes:
            if node["type"] in definitions:
                graph = definitions[node["type"]]
                self.assertEqual(len(node["outputs"]), len(graph["outputs"]), node["id"])
        for phase in range(2, 7):
            self.assertIn(f"saya-auto-phase-{phase}", definitions)
            stop = next(n for n in self.nodes if n["type"] == f"SayaImagePhase{phase}Stop")
            self.assertEqual(stop["mode"], 0)
            self.assertTrue(stop["widgets_values_named"]["unload_after_phase"])
        for graph in self.graphs:
            parents = {}
            for raw in graph["links"]:
                origin,target = (raw["origin_id"],raw["target_id"]) if isinstance(raw,dict) else (raw[1],raw[3])
                parents.setdefault(target, []).append(origin)
            done,active = set(),set()
            def visit(node):
                self.assertNotIn(node,active,"graph cycle")
                if node in done:return
                active.add(node)
                for parent in parents.get(node,[]):visit(parent)
                active.remove(node);done.add(node)
            for node in parents:visit(node)

    def test_no_private_data_or_model_selection(self):
        text = WORKFLOW.read_text()
        for pattern in (r"/home/",r"[A-Z]:\\\\",r"_temp_",r"/api/view",r"lastAccepted",
                        r"textSnapshot",r"m3ssy",r"Mklan",r"nyalia",r"Chastity",r"futanari",
                        r"new-half",r"\bNSFW\b",r"\bnude\b",r"\bvulva\b",r"\bpenis\b",r"\banus\b"):
            self.assertIsNone(re.search(pattern,text,re.I),pattern)
        for value in strings(self.workflow):
            if re.search(r"\.(?:safetensors|ckpt|gguf|pt|pth)$",value):
                self.assertIn("SELECT_", value)
        loras = [n for n in self.nodes if n["type"] == "Lora Loader (LoraManager)"]
        self.assertEqual(len(loras),5)
        for node in loras:
            self.assertEqual(node["widgets_values_named"]["text"], "")
            self.assertEqual(node["widgets_values_named"]["loras"], [])
        settings = next(n for n in self.nodes if n["type"] == "SayaHiDreamLoraSettings")
        self.assertEqual(settings["widgets_values"], [False,"none",1.0,""])

    def test_prompts_and_optional_defaults(self):
        nodes = {n["id"]:n for n in self.nodes}
        self.assertIn("exactly two adults",nodes[144]["widgets_values"][0])
        self.assertIn("one adult woman",nodes[807]["widgets_values"][0])
        self.assertIn("one adult man",nodes[808]["widgets_values"][0])
        self.assertIn("bad anatomy",nodes[145]["widgets_values"][0])
        detailers = [n for n in self.nodes if n["type"] == "SayaDetailerForEach"]
        self.assertEqual({n["title"] for n in detailers},{f"Detailer {i}" for i in range(1,14)})
        self.assertTrue(all(n["mode"]==4 for n in detailers))
        switch = next(n for n in self.nodes if n.get("title")=="Enable HiDream")
        self.assertFalse(switch["widgets_values"][0])
        for node in self.nodes:
            if node["type"] == "SayaDualCLIPTextEncode":
                port = next((p for p in node["inputs"] if p["name"]=="hidream_clip"),None)
                if port:self.assertIsNone(port["link"])

    def test_public_docs(self):
        readme = (ROOT/"README.md").read_text()
        self.assertTrue(readme.startswith("# Saya Comfy Couple Plus"))
        self.assertIn("workflows/Ilust-Simple-V1.json", readme)
        self.assertIsNone(re.search(r"\bWIP\b|work in progress|not considered finished",readme,re.I))
        self.assertEqual((ROOT/"VERSION.txt").read_text().strip(),"1.0.0")
        self.assertEqual(list((ROOT/"workflows").glob("*.json")),[WORKFLOW])
        self.assertFalse((ROOT/"workflow/Saya_Comfy_Couple_Demo.json").exists())


if __name__ == "__main__":unittest.main(verbosity=2)

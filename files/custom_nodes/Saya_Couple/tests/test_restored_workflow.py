"""Structural regression checks for the restored user workflow."""

from __future__ import annotations

import json
import sys
from pathlib import Path


def main(path: Path) -> int:
    workflow = json.loads(path.read_text(encoding="utf-8"))
    nodes = {node["id"]: node for node in workflow["nodes"]}
    review = next(node for node in nodes.values() if node.get("type") == "SayaImageGenerationReview")
    seed = next(node for node in nodes.values() if node.get("properties", {}).get("saya_master_seed") is True)
    imprint_metadata = next(node for node in nodes.values() if node.get("title") == "Couple Imprint · Metadata")
    inputs = {item["name"]: item for item in review["inputs"]}
    assert inputs["seed"]["link"] is not None, "review seed must be linked"
    assert inputs["imprint_json"]["link"] is not None, "review imprint_json must be linked"
    links = {link[0]: link for link in workflow["links"]}
    assert links[inputs["seed"]["link"]][1] == seed["id"], "review seed must come from Master Seed"
    imprint_source = links[inputs["imprint_json"]["link"]][1:3]
    metadata_value = next(item for item in imprint_metadata["inputs"] if item["name"] == "value")
    assert links[metadata_value["link"]][1:3] == imprint_source, "checkpoint and metadata saver must use same imprint"

    subgraphs = workflow.get("definitions", {}).get("subgraphs", workflow.get("subgraphs", []))
    couple = next(graph for graph in subgraphs if graph.get("name") == "Couple")
    producer = next(node for node in couple["nodes"] if node["id"] == 100044)
    assert producer["type"] == "SayaCoupleImprintPackV2", "Phase 1 must use native v2 producer"
    producer_inputs = {item["name"]: item for item in producer["inputs"]}
    for name in ("negative_prompt", "reference_width", "reference_height"):
        assert producer_inputs[name]["link"] is not None, f"v2 producer {name} must be linked"

    parent = nodes[imprint_source[0]]
    parent_inputs = {item["name"]: item for item in parent["inputs"]}
    assert links[parent_inputs["negative_prompt"]["link"]][1] == 145
    assert links[parent_inputs["reference_width"]["link"]][1:3] == [100042, 1]
    assert links[parent_inputs["reference_height"]["link"]][1:3] == [100042, 2]
    print("PASS workflow links native v2 imprint and live Phase-1 inputs into Review")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(Path(sys.argv[1])))

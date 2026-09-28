"""Migrate a Saya Couple 0.2.0 workflow (Full or Demo, even customised) to 0.3.0.

    python3 tools/migrate_workflow_v030.py old.json new.json [INDENT]

The Demo only needs step 3; the Full gets all of them.

1. Phase 3 HiDream: ClownRegionalConditioning3 (P1 / P2 / empty MAIN region)
   -> ClownRegionalConditioning_AB (P1 / P2). The reconstruct's third output is
   renamed conditioning_solo (it only feeds the Solo select).
2. Phase 2 USDU 1/2 (SayaCoupleUSDUPass): the couple_crop widget is removed and
   the required `solo` input is wired from the Phase 2 subgraph `solo` input.
3. Phase 1 SayaMultiCouple: the dual_attention_enabled switch is removed (dual
   attention now always follows Couple mode).
4. Dead parts: the unused Phase 6 `selected_identifier` subgraph input, the
   orphan Phase 6 GetNode, stale notes and migration markers nobody reads.

Every step asserts the exact shape it expects, so running it on an
unexpected workflow fails instead of writing a half-migrated file.
"""

import json
import sys

PHASE_2 = "23a92ba3-1ca2-4326-aeb9-d51aa4c48c54"
PHASE_3 = "saya-auto-phase-3"
PHASE_6 = "saya-auto-phase-6"
COUPLE = "cc1b8cff-65c2-4b93-8a59-5ef17907bab2"
REGIONAL_NODE = 100086
HIDREAM_RECONSTRUCT = 100085
MULTI_COUPLE = 100043
USDU_NODES = (11004, 11005)
ORPHAN_GET_NODE = 10306

#: Node properties read by code (phase controller / backend) or still-accurate notes.
KEPT_PROPERTIES = {"saya_phase", "saya_original_mode", "saya_shared_base", "saya_note", "saya_policy"}
STALE_NOTES = {10146, 10153}
NOTE_REWRITES = {
    12401: "HiDream Quad CLIP (text encoders only). Loaded only on a HiDream conditioning "
           "cache miss; the HiDream MODEL stays in Phase 3.",
}


def _subgraphs(workflow):
    return workflow.get("definitions", {}).get("subgraphs", [])


def _subgraph(workflow, sid):
    return next(s for s in _subgraphs(workflow) if s["id"] == sid)


def _node(graph, nid):
    return next(n for n in graph["nodes"] if n["id"] == nid)


def _root_link(workflow, link_id):
    return next(l for l in workflow["links"] if l[0] == link_id)


def _new_link_id(workflow):
    workflow["last_link_id"] += 1
    for sub in _subgraphs(workflow):
        sub["state"]["lastLinkId"] = workflow["last_link_id"]
    return workflow["last_link_id"]


def _drop_subgraph_link(graph, link_id):
    link = next(l for l in graph["links"] if l["id"] == link_id)
    graph["links"].remove(link)
    if link["origin_id"] == -10:
        graph["inputs"][link["origin_slot"]]["linkIds"].remove(link_id)
    else:
        _node(graph, link["origin_id"])["outputs"][link["origin_slot"]]["links"].remove(link_id)
    return link


def migrate_phase_3(workflow):
    graph = _subgraph(workflow, PHASE_3)
    node = _node(graph, REGIONAL_NODE)
    assert node["type"] == "ClownRegionalConditioning3", node["type"]
    names = [i["name"] for i in node["inputs"]]
    dropped = names.index("conditioning_unmasked")
    link_id = node["inputs"][dropped]["link"]
    del node["inputs"][dropped]
    if link_id is not None:
        _drop_subgraph_link(graph, link_id)
    for link in graph["links"]:
        if link["target_id"] == REGIONAL_NODE and link["target_slot"] > dropped:
            link["target_slot"] -= 1
    node["type"] = "ClownRegionalConditioning_AB"
    node["properties"]["Node name for S&R"] = "ClownRegionalConditioning_AB"
    node["title"] = "Phase 3 · Regional Conditioning · P1 / P2"

    reconstruct = _node(graph, HIDREAM_RECONSTRUCT)
    output = reconstruct["outputs"][2]
    assert output["name"] == "conditioning_unmasked", output["name"]
    output["name"] = output["localized_name"] = "conditioning_solo"
    reconstruct["title"] = "Phase 3 · HiDream Reconstruct · P1 / P2 / Solo"


def migrate_usdu(workflow):
    graph = _subgraph(workflow, PHASE_2)
    solo_input = next(i for i in graph["inputs"] if i["name"] == "solo")
    solo_slot = graph["inputs"].index(solo_input)
    for nid in USDU_NODES:
        node = _node(graph, nid)
        assert node["type"] == "SayaCoupleUSDUPass", node["type"]
        values = node["widgets_values"]
        assert len(values) == 15 and isinstance(values[13], bool), values
        del values[13]
        node.get("widgets_values_named", {}).pop("couple_crop", None)
        index = [i["name"] for i in node["inputs"]].index("couple_crop")
        assert all(i.get("link") is None for i in node["inputs"][index:]), "linked slot after couple_crop"
        del node["inputs"][index]
        link_id = _new_link_id(workflow)
        node["inputs"].append({"localized_name": "solo", "name": "solo", "type": "BOOLEAN", "link": link_id})
        graph["links"].append({"id": link_id, "origin_id": -10, "origin_slot": solo_slot,
                               "target_id": nid, "target_slot": len(node["inputs"]) - 1, "type": "BOOLEAN"})
        solo_input["linkIds"].append(link_id)


def migrate_dual_switch(workflow):
    for graph in [workflow, *_subgraphs(workflow)]:
        for node in (n for n in graph["nodes"] if n["type"] == "SayaMultiCouple"):
            names = [i["name"] for i in node["inputs"]]
            if "dual_attention_enabled" not in names:
                continue
            assert names[-1] == "dual_attention_enabled", names
            assert len(node["widgets_values"]) == 4, node["widgets_values"]  # strength_1, strength_2, solo, dual
            link_id = node["inputs"].pop()["link"]
            node["widgets_values"].pop()
            node.get("widgets_values_named", {}).pop("dual_attention_enabled", None)
            if link_id is not None:
                link = _drop_subgraph_link(graph, link_id)
                assert link["origin_id"] == -10, "dual switch fed by a node, not by the subgraph input"
                _drop_subgraph_input(workflow, graph, "dual_attention_enabled")


def _drop_subgraph_input(workflow, graph, name):
    """Remove an unused LAST subgraph input and the matching (unlinked) widget on every instance."""
    assert graph["inputs"][-1]["name"] == name and not graph["inputs"][-1]["linkIds"], name
    graph["inputs"].pop()
    for instance in (n for n in workflow["nodes"] if n["type"] == graph["id"]):
        assert instance["inputs"][-1]["name"] == name and instance["inputs"][-1]["link"] is None
        instance["inputs"].pop()
        instance["widgets_values"].pop()
        instance.get("widgets_values_named", {}).pop(name, None)


def remove_phase_6_dead_parts(workflow):
    graph = _subgraph(workflow, PHASE_6)
    orphan = next((n for n in graph["nodes"] if n["id"] == ORPHAN_GET_NODE), None)
    if orphan is not None:
        assert orphan["type"] == "GetNode" and not orphan.get("inputs") and not orphan["outputs"][0]["links"]
        graph["nodes"].remove(orphan)

    names = [i["name"] for i in graph["inputs"]]
    if "selected_identifier" not in names:
        return
    dropped = names.index("selected_identifier")
    assert not graph["inputs"][dropped]["linkIds"], "selected_identifier is used inside Phase 6"
    del graph["inputs"][dropped]
    for link in graph["links"]:
        if link["origin_id"] == -10 and link["origin_slot"] > dropped:
            link["origin_slot"] -= 1

    for instance in (n for n in workflow["nodes"] if n["type"] == PHASE_6):
        slot = [i["name"] for i in instance["inputs"]].index("selected_identifier")
        link_id = instance["inputs"].pop(slot)["link"]
        if link_id is not None:
            link = _root_link(workflow, link_id)
            workflow["links"].remove(link)
            origin = next(n for n in workflow["nodes"] if n["id"] == link[1])
            origin["outputs"][link[2]]["links"].remove(link_id)
        for link in workflow["links"]:
            if link[3] == instance["id"] and link[4] > slot:
                link[4] -= 1


def remove_stale_metadata(workflow):
    for graph in [workflow, *_subgraphs(workflow)]:
        for node in graph["nodes"]:
            props = node.get("properties") or {}
            for key in [k for k in props if k.startswith("saya_") and k not in KEPT_PROPERTIES]:
                del props[key]
            if node["id"] in STALE_NOTES:
                props.pop("saya_note", None)
            if node["id"] in NOTE_REWRITES:
                props["saya_note"] = NOTE_REWRITES[node["id"]]
    extra = workflow["extra"]
    for key in [k for k in extra if k.startswith("saya_")]:
        del extra[key]


def main(src, dst, indent=None):
    with open(src, encoding="utf-8") as handle:
        workflow = json.load(handle)
    if workflow.get("extra", {}).get("saya_couple_version") == "0.3.0":
        sys.exit(f"{src} is already a Saya Couple 0.3.0 workflow; nothing to do.")
    full = any(s["id"] == PHASE_3 for s in _subgraphs(workflow))
    if full:  # the Demo workflow is Phase 1 only
        migrate_phase_3(workflow)
        migrate_usdu(workflow)
        remove_phase_6_dead_parts(workflow)
    migrate_dual_switch(workflow)
    remove_stale_metadata(workflow)
    workflow.setdefault("extra", {})["saya_couple_version"] = "0.3.0"
    with open(dst, "w", encoding="utf-8") as handle:
        if indent is None:
            json.dump(workflow, handle, ensure_ascii=False, separators=(",", ":"))
        else:
            handle.write(json.dumps(workflow, indent=indent, ensure_ascii=False))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else None)

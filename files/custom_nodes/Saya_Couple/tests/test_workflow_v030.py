"""Structure of a workflow migrated to v0.3.0 (HiDream 2 regions + USDU solo crop).

Opt-in like test_workflow_compat: SAYA_TEST_EXTERNAL_WORKFLOWS=1 and
SAYA_TEST_WORKFLOW_PATHS listing real workflow files. Only files carrying the
``extra.saya_couple_version`` marker are checked by the v0.3.0 routing tests;
link integrity is checked on every listed file.
"""

import json

from harness import Check, load_pack
from test_workflow_compat import _opted_in_workflow_files

SKIP = "SAYA_TEST_EXTERNAL_WORKFLOWS not set — opt-in integration check, not part of the autonomous suite"
PHASE_2 = "23a92ba3-1ca2-4326-aeb9-d51aa4c48c54"
PHASE_3 = "saya-auto-phase-3"
COUPLE_TOGGLE = "saya-couple-mode-toggle"
USDU_PASS = "saya-usdu-couple-pass"
USDU_POSITION = "saya-usdu-position"
# Nodes that install a regional attention patch on a MODEL.
REGIONAL_PATCHERS = {"ReHiDreamPatcher", "ReHiDreamPatcherAdvanced", "SayaCoupleReconstruct", "SayaMultiCouple",
                     "SayaAttentionCouplePPM", "ClownRegionalConditioning3", "ClownRegionalConditioning_ABC"}


def _graphs(workflow):
    yield "root", workflow, workflow["links"]
    for sub in workflow.get("definitions", {}).get("subgraphs", []):
        yield sub["name"], sub, sub["links"]


def _link(entry):
    if isinstance(entry, list):
        return {"id": entry[0], "origin_id": entry[1], "origin_slot": entry[2],
                "target_id": entry[3], "target_slot": entry[4], "type": entry[5]}
    return entry


def _workflows():
    files = _opted_in_workflow_files()
    if files is None:
        return None
    return [(path, json.loads(path.read_text(encoding="utf-8"))) for path in files if path.is_file()]


def _check_links(c, name, graph, links):
    nodes = {n["id"]: n for n in graph["nodes"]}
    ids = set()
    for raw in links:
        link = _link(raw)
        ids.add(link["id"])
        origin, target = link["origin_id"], link["target_id"]
        if target == -20:
            outputs = graph.get("outputs", [])
            c.ok(link["target_slot"] < len(outputs) and link["id"] in outputs[link["target_slot"]].get("linkIds", []),
                 f"{name}: link {link['id']} -> subgraph output slot")
        else:
            node = nodes.get(target)
            c.ok(node is not None, f"{name}: link {link['id']} targets missing node {target}")
            if node is not None:
                slot = link["target_slot"]
                c.ok(slot < len(node["inputs"]) and node["inputs"][slot].get("link") == link["id"],
                     f"{name}: link {link['id']} target slot {slot} on {node['type']} {target} does not point back")
        if origin == -10:
            inputs = graph.get("inputs", [])
            c.ok(link["origin_slot"] < len(inputs) and link["id"] in inputs[link["origin_slot"]].get("linkIds", []),
                 f"{name}: link {link['id']} <- subgraph input slot")
        else:
            node = nodes.get(origin)
            c.ok(node is not None, f"{name}: link {link['id']} from missing node {origin}")
            if node is not None:
                slot = link["origin_slot"]
                c.ok(slot < len(node.get("outputs", [])) and link["id"] in (node["outputs"][slot].get("links") or []),
                     f"{name}: link {link['id']} origin slot {slot} on {node['type']} {origin} does not list it")
    for node in graph["nodes"]:
        for i, slot in enumerate(node.get("inputs", [])):
            if slot.get("link") is not None:
                c.ok(slot["link"] in ids, f"{name}: {node['type']} {node['id']} input {slot['name']} -> dangling link {slot['link']}")
        for slot in node.get("outputs", []):
            for link_id in slot.get("links") or []:
                c.ok(link_id in ids, f"{name}: {node['type']} {node['id']} output {slot['name']} -> dangling link {link_id}")


def test_links_integrity():
    c = Check("workflow_v030_links_integrity")
    workflows = _workflows()
    if workflows is None:
        c.skip(SKIP)
        return c.report()
    for path, workflow in workflows:
        for name, graph, links in _graphs(workflow):
            _check_links(c, f"{path.name}/{name}", graph, links)
    return c.report()


def _upstream(graph, node_id, input_name):
    """(node, origin_slot) feeding ``input_name`` of ``node_id`` (None if unlinked or from the subgraph input)."""
    nodes = {n["id"]: n for n in graph["nodes"]}
    links = {_link(l)["id"]: _link(l) for l in graph["links"]}
    slot = next(i for i in nodes[node_id]["inputs"] if i["name"] == input_name)
    link = links.get(slot.get("link"))
    if link is None:
        return None, None
    if link["origin_id"] == -10:
        return graph["inputs"][link["origin_slot"]], None
    return nodes[link["origin_id"]], link["origin_slot"]


def _model_chain(graph, node_id, input_name="model"):
    chain = []
    node, _ = _upstream(graph, node_id, input_name)
    while node is not None and "type" in node and node["type"] != node.get("id"):
        if "inputs" not in node or not any(i["name"] in ("model", "MODEL") for i in node["inputs"]):
            chain.append(node["type"])
            break
        chain.append(node["type"])
        name = next(i["name"] for i in node["inputs"] if i["name"] in ("model", "MODEL"))
        node, _ = _upstream(graph, node["id"], name)
    return chain


def _migrated():
    workflows = _workflows()
    if workflows is None:
        return None
    return [(p, w) for p, w in workflows if w.get("extra", {}).get("saya_couple_version")]


def test_phase_3_routing():
    c = Check("workflow_v030_phase_3_routing")
    workflows = _migrated()
    if workflows is None:
        c.skip(SKIP)
        return c.report()
    load_pack()
    for path, workflow in workflows:
        graph = next(s for s in workflow["definitions"]["subgraphs"] if s["id"] == PHASE_3)
        types = [n["type"] for n in graph["nodes"]]
        c.eq(types.count("ClownRegionalConditioning_AB"), 1, f"{path.name}: one two-region node")
        c.eq(types.count("ClownRegionalConditioning3"), 0, f"{path.name}: no three-region node left")
        regional = next(n for n in graph["nodes"] if n["type"] == "ClownRegionalConditioning_AB")
        reconstruct = next(n for n in graph["nodes"] if n["type"] == "SayaCoupleHiDreamReconstruct")
        wiring = {name: _upstream(graph, regional["id"], name) for name in ("conditioning_A", "conditioning_B", "mask_A", "mask_B")}
        c.eq({k: (n["id"], s) for k, (n, s) in wiring.items()},
             {"conditioning_A": (reconstruct["id"], 0), "conditioning_B": (reconstruct["id"], 1),
              "mask_A": (reconstruct["id"], 4), "mask_B": (reconstruct["id"], 5)},
             f"{path.name}: P1/P2 conditionings and masks come from the reconstruct")
        c.eq(regional["widgets_values"][6], "boolean", f"{path.name}: boolean region masks")

        select = next(n for n in graph["nodes"] if n["type"] == "SayaLazyBooleanSelect")
        sampler = next(n for n in graph["nodes"] if n["type"] == "ClownsharKSampler_Beta")
        c.eq(_upstream(graph, sampler["id"], "positive")[0]["id"], select["id"], f"{path.name}: sampler positive = Solo select")
        c.eq(_upstream(graph, select["id"], "on_false")[0]["id"], regional["id"], f"{path.name}: Couple -> regional P1/P2")
        c.eq(_upstream(graph, select["id"], "on_true"), (reconstruct, 2), f"{path.name}: Solo -> global conditioning")
        c.eq(_upstream(graph, select["id"], "switch")[0]["name"], "solo", f"{path.name}: select driven by Phase 3 solo")
        c.eq(_upstream(graph, reconstruct["id"], "solo")[0]["name"], "solo", f"{path.name}: reconstruct driven by Phase 3 solo")
        patcher = next(n for n in graph["nodes"] if n["type"] == "ReHiDreamPatcher")
        c.eq(_upstream(graph, patcher["id"], "enable"), (reconstruct, 6), f"{path.name}: patcher enable = regional_enabled")
        chain = _model_chain(graph, sampler["id"])
        c.eq([t for t in chain if t in REGIONAL_PATCHERS], ["ReHiDreamPatcher"],
             f"{path.name}: exactly one regional hook on the HiDream model ({chain})")
    return c.report()


def test_usdu_solo_wiring():
    c = Check("workflow_v030_usdu_solo_wiring")
    workflows = _migrated()
    if workflows is None:
        c.skip(SKIP)
        return c.report()
    for path, workflow in workflows:
        subgraphs = {s["id"]: s for s in workflow["definitions"]["subgraphs"]}
        # Since the USDU position switch the pair lives in its own subgraph (placed in Phase 2 by default).
        graph = subgraphs.get(USDU_PASS) or subgraphs[PHASE_2]
        usdu = [n for n in graph["nodes"] if n["type"] == "SayaCoupleUSDUPass"]
        c.eq(len(usdu), 2, f"{path.name}: USDU 1 + USDU 2")
        for node in usdu:
            c.ok(all(i["name"] != "couple_crop" for i in node["inputs"]), f"{path.name}: {node['id']} has no couple_crop slot")
            c.eq(len(node["widgets_values"]), 14, f"{path.name}: {node['id']} widget count")
            c.eq(_upstream(graph, node["id"], "solo")[0]["name"], "solo", f"{path.name}: {node['id']} solo = Phase 2 solo")
            chain = _model_chain(graph, node["id"])
            c.ok(sum(t in REGIONAL_PATCHERS for t in chain) <= 1, f"{path.name}: {node['id']} single couple hook ({chain})")
        restore = {n["id"]: n["widgets_values"][13] for n in usdu}
        c.eq(restore, {11004: False, 11005: True}, f"{path.name}: restore_to_base preserved (USDU1 continues into USDU2)")
        phase_2 = next(n for n in workflow["nodes"] if n["type"] == PHASE_2)
        solo_in = next(i for i in phase_2["inputs"] if i["name"] == "solo")
        origin = next(_link(l) for l in workflow["links"] if _link(l)["id"] == solo_in["link"])
        toggle = next(n for n in workflow["nodes"] if n["id"] == origin["origin_id"])
        c.eq(toggle["type"], COUPLE_TOGGLE, f"{path.name}: Phase 2 solo comes from the global Couple/Solo toggle")
    return c.report()


def test_no_removed_paths_in_workflow():
    c = Check("workflow_v030_no_removed_paths")
    workflows = _migrated()
    if workflows is None:
        c.skip(SKIP)
        return c.report()
    removed = ("ClownRegionalConditioning3", "conditioning_unmasked", "couple_crop", "dual_attention_enabled",
               "selected_identifier", "SayaDualCLIPTextEncode")
    for path, workflow in workflows:
        text = json.dumps(workflow)
        for needle in removed:
            c.ok(needle not in text, f"{path.name}: removed {needle!r} still present")
        kept = {k for graph in [workflow, *workflow["definitions"]["subgraphs"]] for n in graph["nodes"]
                for k in (n.get("properties") or {}) if k.startswith("saya_")}
        c.ok(kept <= {"saya_phase", "saya_original_mode", "saya_shared_base", "saya_note", "saya_policy"},
             f"{path.name}: unread saya_* node properties {sorted(kept)}")
    return c.report()


def test_usdu_position_switch():
    """The Hires Fix 1 -> USDU 1 + 2 -> Hires Fix 3 block runs where MAIN · USDU Position says: one block per
    candidate phase, each gated by its own boolean of the selector, and Phase 2 (the historical place) is the default."""
    c = Check("workflow_usdu_position_switch")
    workflows = _migrated()
    if workflows is None:
        c.skip(SKIP)
        return c.report()
    for path, workflow in workflows:
        subgraphs = {s["id"]: s for s in workflow["definitions"]["subgraphs"]}
        if USDU_PASS not in subgraphs:
            continue
        selector = next((n for n in workflow["nodes"] if n["type"] == USDU_POSITION), None)
        if selector is None:  # workflow without the movable USDU block (fixed placement): nothing to check here
            continue
        c.eq(selector["widgets_values"], ["Phase 2"], f"{path.name}: default position is Phase 2")
        pas = subgraphs[USDU_PASS]
        switch = next(n for n in pas["nodes"] if n["type"] == "ComfySwitchNode")
        names = {i["name"]: k for k, i in enumerate(switch["inputs"])}
        into = {_link(l)["target_slot"]: _link(l) for l in pas["links"] if _link(l)["target_id"] == switch["id"]}
        c.eq(pas["inputs"][into[names["on_false"]]["origin_slot"]]["name"], "image",
             f"{path.name}: switched off, the pass returns its input image")
        c.eq(pas["inputs"][into[names["switch"]]["origin_slot"]]["name"], "enable", f"{path.name}: switch = enable")
        c.ok(into[names["on_true"]]["origin_id"] != -10 and next(n for n in pas["nodes"] if n["id"] == into[names["on_true"]]["origin_id"]).get("title", "").startswith("Hires Fix 3"),
             f"{path.name}: switched on, the block returns Hires Fix 3 (Hires 1 -> USDU -> Hires 3 move together)")
        root_links = {_link(l)["id"]: _link(l) for l in workflow["links"]}
        outputs = {}
        for sid, sub in subgraphs.items():
            passes = [n for n in sub["nodes"] if n["type"] == USDU_PASS]
            if not passes:
                continue
            c.eq(len(passes), 1, f"{path.name}: {sub['name']} has one USDU pass")
            enable = next(i for i in passes[0]["inputs"] if i["name"] == "enable")
            origin = next(_link(l) for l in sub["links"] if _link(l)["id"] == enable["link"])
            c.eq(sub["inputs"][origin["origin_slot"]]["name"], "usdu_here", f"{path.name}: {sub['name']} enable = usdu_here")
            # Every placement picks Model 1 / Model 2 per pass from the one MAIN · USDU Model Routing: the block's
            # choice inputs are the phase's choice inputs, linked at the root from that hub.
            instance = next(n for n in workflow["nodes"] if n["type"] == sid)
            for choice in ("USDU 1 Choice", "USDU 2 Choice", "Hires Fix 1 Choice", "Hires Fix 3 Choice"):
                slot = next(i for i in passes[0]["inputs"] if i["name"] == choice)
                src = next(_link(l) for l in sub["links"] if _link(l)["id"] == slot["link"])
                c.eq(src["origin_id"] == -10 and sub["inputs"][src["origin_slot"]]["name"], choice,
                     f"{path.name}: {sub['name']} {choice} is a phase input")
                root_in = next(i for i in instance["inputs"] if i["name"] == choice)
                hub = next(n for n in workflow["nodes"] if n["id"] == root_links[root_in["link"]]["origin_id"])
                c.eq(hub.get("title"), "Hub USDU · Model Routing", f"{path.name}: {sub['name']} {choice} from the hub")
            if "Phase 6" in sub["name"]:
                # End: the block works at the Phase 5 size, then the final resize makes the 2K image.
                out = next(_link(l) for l in sub["links"] if _link(l)["origin_id"] == passes[0]["id"])
                resize = next(n for n in sub["nodes"] if n["id"] == out["target_id"])
                c.eq(resize.get("title"), "Phase 6 · Final Resize", f"{path.name}: End block feeds the final resize")
            instance = next(n for n in workflow["nodes"] if n["type"] == sid)
            here = next(i for i in instance["inputs"] if i["name"] == "usdu_here")
            source = root_links[here["link"]]
            c.eq(source["origin_id"], selector["id"], f"{path.name}: {sub['name']} usdu_here comes from the selector")
            outputs[sub["name"]] = source["origin_slot"]
        c.eq(sorted(outputs.values()), [0, 1, 2, 3], f"{path.name}: each selector output drives exactly one phase {outputs}")
    return c.report()


TESTS = (test_links_integrity, test_phase_3_routing, test_usdu_solo_wiring, test_no_removed_paths_in_workflow,
         test_usdu_position_switch)

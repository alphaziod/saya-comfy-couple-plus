"""Migrate a Saya Couple 2.1 workflow (Full, even customised) to the 2.2 positive order.

    python3 tools/migrate_workflow_v220.py old.json new.json [INDENT]

2.2 reads the positive as QUALITY -> ACTION -> P1 / P2 -> BACKGROUND (negative apart):

1. SayaMainPrompt (or the French node) gains its `prefix` and `scene` outputs; the Phase 1 MAIN encode
   reads `scene` (background + tags + extra, no prefix) instead of `main_prompt`. Every other consumer of
   `main_prompt` (imprint, review gate) keeps the full MAIN text, so later phases rebuild exactly as before.
2. A new CLIPTextEncode encodes `prefix` (quality tags + LoRA triggers) on the same CLIP and feeds the new
   `quality` input of SayaMultiCouple, through a new `quality` input of the subgraph holding it.
3. P1 / P2 text assembly: the anatomy text becomes `string_a` and the identity text `string_b`, so the
   organ is encoded in the same CLIP chunk as the body it belongs to.
4. Imprint: SayaCoupleImprintPackV2 receives `scene` as its main_prompt and the `prefix` on its new
   `quality_prompt` input, so every later phase (Hires, HiDream, Refine, Detailers, Upscale) rebuilds
   QUALITY first and the background last, like Phase 1.

Idempotent: a workflow already migrated is written back unchanged. Every step asserts the shape it expects.
"""

import copy
import json
import sys
import uuid

MAIN_PROMPT_TYPES = ("SayaMainPrompt", "SayaMainPromptFR")
NEW_OUTPUTS = {"SayaMainPrompt": ("prefix", "scene"), "SayaMainPromptFR": ("prefixe", "scene")}
PREFIX_SLOT, SCENE_SLOT = 4, 5
QUALITY_TITLE = "CLIP · Quality + LoRA triggers (read first)"


def _ids(wf):
    links = [l[0] for l in wf["links"]] + [l["id"] for sg in wf["definitions"]["subgraphs"] for l in sg["links"]]
    return max(links)


def migrate(wf):
    root = {n["id"]: n for n in wf["nodes"]}
    links = {l[0]: l for l in wf["links"]}
    subs = wf["definitions"]["subgraphs"]
    next_id = [_ids(wf)]

    def new_link():
        next_id[0] += 1
        return next_id[0]

    # 3. anatomy first in the P1 / P2 assembly
    for sg in subs:
        sl = {l["id"]: l for l in sg["links"]}
        for n in sg["nodes"]:
            if n["type"] != "StringConcatenate" or "identity + anatomy" not in (n.get("title") or ""):
                continue
            a = next(i for i in n["inputs"] if i["name"] == "string_a")
            b = next(i for i in n["inputs"] if i["name"] == "string_b")
            ia, ib = n["inputs"].index(a), n["inputs"].index(b)
            sl[a["link"]]["target_slot"], sl[b["link"]]["target_slot"] = ib, ia
            a["link"], b["link"] = b["link"], a["link"]
            n["title"] = n["title"].replace("identity + anatomy", "anatomy + identity")

    couple_sg = next(sg for sg in subs if any(n["type"] == "SayaMultiCouple" for n in sg["nodes"]))
    multi = next(n for n in couple_sg["nodes"] if n["type"] == "SayaMultiCouple")
    instance = next(n for n in wf["nodes"] if n["type"] == couple_sg["id"])
    if not any(i["name"] == "quality" for i in multi["inputs"]):
        _quality_and_scene(wf, root, links, couple_sg, multi, instance, new_link)
    _imprint_quality(wf, root, links, couple_sg, instance, new_link)
    wf["last_link_id"] = max(wf.get("last_link_id", 0), next_id[0])
    for sg in subs:
        state = sg.get("state")
        if isinstance(state, dict) and "lastLinkId" in state:
            state["lastLinkId"] = max(state["lastLinkId"], next_id[0])
    return wf


def _main_prompt_node(wf):
    return next(n for n in wf["nodes"] if n["type"] in MAIN_PROMPT_TYPES)


def _imprint_quality(wf, root, links, couple_sg, instance, new_link):
    """4. the imprint stores QUALITY apart and MAIN = scene."""
    pack = next(n for n in couple_sg["nodes"] if n["type"] == "SayaCoupleImprintPackV2")
    if any(i["name"] == "quality_prompt" for i in pack["inputs"]):
        return
    mp = _main_prompt_node(wf)
    inner = {l["id"]: l for l in couple_sg["links"]}
    main_in = next(i for i in pack["inputs"] if i["name"] == "main_prompt")
    sub_slot = inner[main_in["link"]]["origin_slot"]
    sub_name = couple_sg["inputs"][sub_slot]["name"]
    feed = links[next(i["link"] for i in instance["inputs"] if i["name"] == sub_name)]
    assert feed[1] == mp["id"] and feed[2] == 0, "imprint main_prompt is not fed by main_prompt"
    feed[2] = SCENE_SLOT
    mp["outputs"][0]["links"].remove(feed[0])
    mp["outputs"][SCENE_SLOT]["links"].append(feed[0])
    li, lo = new_link(), new_link()
    pack["inputs"].append({"localized_name": "quality_prompt", "name": "quality_prompt", "shape": 7, "type": "STRING", "link": li})
    last = couple_sg["inputs"][-1]
    couple_sg["inputs"].append({"id": str(uuid.uuid4()), "name": "quality_text", "type": "STRING", "linkIds": [li],
                                "localized_name": "quality_text", "pos": [last["pos"][0], last["pos"][1] + 20]})
    couple_sg["links"].append({"id": li, "origin_id": -10, "origin_slot": len(couple_sg["inputs"]) - 1,
                               "target_id": pack["id"], "target_slot": len(pack["inputs"]) - 1, "type": "STRING"})
    instance["inputs"].append({"localized_name": "quality_text", "name": "quality_text", "type": "STRING", "link": lo})
    mp["outputs"][PREFIX_SLOT]["links"].append(lo)
    wf["links"].append([lo, mp["id"], PREFIX_SLOT, instance["id"], len(instance["inputs"]) - 1, "STRING"])


def _quality_and_scene(wf, root, links, couple_sg, multi, instance, new_link):
    """1. and 2."""

    # 1. MAIN prompt: prefix / scene outputs, Phase 1 MAIN encode reads scene
    mp = _main_prompt_node(wf)
    assert len(mp["outputs"]) == 4, f"{mp['type']}: expected the 4 outputs of 2.1, got {len(mp['outputs'])}"
    mp["outputs"] += [{"name": name, "type": "STRING", "links": []} for name in NEW_OUTPUTS[mp["type"]]]
    main_links = [links[lid] for lid in mp["outputs"][0]["links"] or []]
    enc_links = [l for l in main_links if root[l[3]]["type"] == "CLIPTextEncode"]
    assert len(enc_links) == 1, f"expected one MAIN encode fed by main_prompt, got {len(enc_links)}"
    to_enc = enc_links[0]
    main_enc = root[to_enc[3]]
    to_enc[2] = SCENE_SLOT
    mp["outputs"][0]["links"].remove(to_enc[0])
    mp["outputs"][SCENE_SLOT]["links"].append(to_enc[0])
    main_enc["title"] = "CLIP · Main / Background only (read last)"

    # 2. QUALITY encode -> subgraph `quality` input -> SayaMultiCouple.quality
    clip_link = links[next(i["link"] for i in main_enc["inputs"] if i["name"] == "clip")]
    quality = copy.deepcopy(main_enc)
    quality["id"] = max(max(root), wf.get("last_node_id", 0)) + 1
    quality["title"] = QUALITY_TITLE
    quality["pos"] = [main_enc["pos"][0], main_enc["pos"][1] - 120]
    lc, lt, lo, inner = new_link(), new_link(), new_link(), new_link()
    next(i for i in quality["inputs"] if i["name"] == "clip")["link"] = lc
    next(i for i in quality["inputs"] if i["name"] == "text")["link"] = lt
    quality["outputs"][0]["links"] = [lo]
    wf["nodes"].append(quality)
    root[clip_link[1]]["outputs"][clip_link[2]]["links"].append(lc)
    mp["outputs"][PREFIX_SLOT]["links"].append(lt)

    multi["inputs"].append({"localized_name": "quality", "name": "quality", "shape": 7, "type": "CONDITIONING", "link": inner})
    last = couple_sg["inputs"][-1]
    couple_sg["inputs"].append({"id": str(uuid.uuid4()), "name": "quality", "type": "CONDITIONING", "linkIds": [inner],
                                "localized_name": "quality", "pos": [last["pos"][0], last["pos"][1] + 20]})
    couple_sg["links"].append({"id": inner, "origin_id": -10, "origin_slot": len(couple_sg["inputs"]) - 1,
                               "target_id": multi["id"], "target_slot": len(multi["inputs"]) - 1, "type": "CONDITIONING"})
    instance["inputs"].append({"localized_name": "quality", "name": "quality", "type": "CONDITIONING", "link": lo})
    wf["links"] += [[lc, clip_link[1], clip_link[2], quality["id"], 0, "CLIP"],
                    [lt, mp["id"], PREFIX_SLOT, quality["id"], 1, "STRING"],
                    [lo, quality["id"], 0, instance["id"], len(instance["inputs"]) - 1, "CONDITIONING"]]
    wf["last_node_id"] = max(wf.get("last_node_id", 0), quality["id"])


def main():
    src, dst = sys.argv[1], sys.argv[2]
    indent = int(sys.argv[3]) if len(sys.argv) > 3 else 1
    wf = migrate(json.load(open(src, encoding="utf-8")))
    open(dst, "w", encoding="utf-8").write(json.dumps(wf, ensure_ascii=False, indent=indent))
    print("wrote", dst)


if __name__ == "__main__":
    main()

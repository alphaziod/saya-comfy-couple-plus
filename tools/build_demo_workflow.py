"""Builds workflows/Saya_Couple_Demo.json (+ the API prompt Saya_Couple_Demo_api.json) from the maintainer's validated workflow.

    python3 tools/build_demo_workflow.py --source /path/to/validated_workflow.json

Only the Shark dual sampling core is kept. Sampler values are READ from the source workflow (promoted
subgraph widgets override the inner node values, exactly like ComfyUI does at queue time), never typed by
hand. Prompts come from workflows/demo_prompt.json. Nothing personal is copied: node schemas and sampler
settings only.
"""

import argparse
import copy
import json
import os
import urllib.parse
import urllib.request

PKG = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
S1_NAME = "Phase 1 · Sampler 1 · Base Generation"
S2_NAME = "Phase 1 · Sampler 2 · Refinement"
CLOWN_WIDGETS = ["eta", "sampler_name", "scheduler", "steps", "steps_to_run", "denoise", "cfg", "seed", "control_after_generate", "sampler_mode", "bongmath"]
# Neutral placeholders: ComfyUI shows them as "not in list" until the user picks a model in the loader.
CKPT_1 = "SELECT_YOUR_MODEL.safetensors"
CKPT_2 = "SELECT_YOUR_REFINER_MODEL.safetensors"


def all_nodes(wf):
    yield from wf["nodes"]
    for sg in wf["definitions"]["subgraphs"]:
        yield from sg["nodes"]


def subgraph(wf, name):
    return next(sg for sg in wf["definitions"]["subgraphs"] if sg["name"] == name)


def effective_sampler(wf, name):
    """Inner ClownsharKSampler widgets overridden by the promoted widgets of the subgraph instance."""
    sg = subgraph(wf, name)
    inner = next(n for n in sg["nodes"] if n["type"] == "ClownsharKSampler_Beta")
    values = dict(zip(CLOWN_WIDGETS, inner["widgets_values"]))
    inst = next(n for n in wf["nodes"] if n["type"] == sg["id"])
    promoted = [i["widget"]["name"] for i in inst["inputs"] if i.get("widget")]
    values.update(dict(zip(promoted, inst["widgets_values"])))
    return values, sg, inner


def write_api_prompt(wf, url, out):
    """The same graph as an API prompt (/prompt format): widget values keyed by input name, links as [node, slot].
    Input order per node type comes from a running ComfyUI's /object_info (frontend-only nodes are resolved)."""
    nodes = {n["id"]: n for n in wf["nodes"]}
    links = {l[0]: l for l in wf["links"]}
    prompt = {}
    for n in wf["nodes"]:
        if n["type"] in ("MarkdownNote", "PrimitiveNode", "PrimitiveStringMultiline"):
            continue
        info = json.load(urllib.request.urlopen(f"{url}/object_info/{urllib.parse.quote(n['type'])}", timeout=15))[n["type"]]["input"]
        order = list(info.get("required", {})) + list(info.get("optional", {}))
        spec = dict(info.get("required", {}), **info.get("optional", {}))
        inputs, widgets = {}, list(n.get("widgets_values") or [])
        linked = {i["name"]: i["link"] for i in n["inputs"] if i.get("link") is not None}
        for name in order:
            kind = spec[name][0]
            if name in linked:
                l = links[linked[name]]
                src = nodes[l[1]]
                if src["type"] in ("PrimitiveNode", "PrimitiveStringMultiline"):  # frontend-only: inline the value
                    inputs[name] = src["widgets_values"][0]
                else:
                    inputs[name] = [str(l[1]), l[2]]
            elif isinstance(kind, list) or kind in ("INT", "FLOAT", "STRING", "BOOLEAN"):
                if widgets:
                    inputs[name] = widgets.pop(0)
                    if name == "seed" and widgets and widgets[0] in ("fixed", "randomize", "increment", "decrement"):
                        widgets.pop(0)
        prompt[str(n["id"])] = {"class_type": n["type"], "inputs": inputs}
    json.dump(prompt, open(out, "w", encoding="utf-8"), indent=1, ensure_ascii=False)
    print("wrote", out, f"({len(prompt)} nodes)")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", required=True)
    ap.add_argument("--object-info", default="http://127.0.0.1:8188", help="running ComfyUI, used to write the API prompt (input order per node type)")
    a = ap.parse_args()
    src = json.load(open(a.source, encoding="utf-8"))
    prompts = json.load(open(os.path.join(PKG, "workflows", "demo_prompt.json"), encoding="utf-8"))

    s1, sg1, _ = effective_sampler(src, S1_NAME)
    s2, _, _ = effective_sampler(src, S2_NAME)
    by_type = {}
    for n in all_nodes(src):
        by_type.setdefault(n["type"], n)
    chain = {n["type"]: n for n in sg1["nodes"]}  # the MODEL_1 guidance chain lives inside Sampler 1's subgraph
    couple = next(n for n in all_nodes(src) if n["type"] == "SayaMultiCouple")
    split = next(n for n in all_nodes(src) if n["type"] == "SayaSplitMask")

    nodes, links = [], []
    order = [0]

    def node(nid, ntype, pos, size, widgets=None, title=None, template=None, color=None):
        t = copy.deepcopy(template) if template else {"inputs": [], "outputs": []}
        n = {"id": nid, "type": ntype, "pos": pos, "size": size, "flags": {}, "order": order[0], "mode": 0,
             "inputs": [dict(i, link=None) for i in t.get("inputs", []) if i["name"] not in ("person_until",)],
             "outputs": [dict(o, links=[]) for o in t.get("outputs", [])],
             "properties": {"Node name for S&R": ntype}, "widgets_values": widgets if widgets is not None else []}
        if title:
            n["title"] = title
        if color:
            n["color"], n["bgcolor"] = color
        order[0] += 1
        nodes.append(n)
        return n

    def link(a_node, a_slot, b_node, b_input):
        lid = len(links) + 1
        out = a_node["outputs"][a_slot]
        inp = next(i for i in b_node["inputs"] if i["name"] == b_input)
        out["links"].append(lid)
        inp["link"] = lid
        links.append([lid, a_node["id"], a_slot, b_node["id"], b_node["inputs"].index(inp), out["type"]])

    GREEN, BLUE, RED, PURPLE = ("#233", "#355"), ("#223", "#335"), ("#322", "#533"), ("#323", "#535")
    head = node(1, "MarkdownNote", [-40, -260], [560, 200], ["# SELECT YOUR SDXL / ILLUSTRIOUS MODEL HERE\n# THEN CLICK GENERATE\n\nPick a checkpoint in both loaders once (MODEL_1 = main model, MODEL_2 = refiner; the same model in both works). Random seed every run."])
    node(2, "MarkdownNote", [560, -260], [900, 200], [
        "**MAIN** = the world (scene, background, colours).  **P1** / **P2** = the two characters.\n\n"
        "**ACTION** = what they do (count, framing, who is LEFT / RIGHT, pose): the characters receive MAIN + ACTION, the background MAIN only.\n\n"
        "Saya keeps MAIN native in cross-attention and adds each character inside its region as a *locked delta* (gain 0.78, in the pack's engine). "
        "With `ownership = dynamic` each pixel's owner is read from the model's own attention (anchors = the distinctive tags of P1 / P2).\n\n"
        "Needs: custom nodes **Saya_Couple** (2.0: no ComfyUI core modification), **MultiMaskCouple**, **RES4LYF**. "
        "No model is included: choose any SDXL / Illustrious checkpoint in the two loaders."])
    ck1 = node(3, "CheckpointLoaderSimple", [-40, 0], [420, 100], [CKPT_1], "MODEL_1 · main checkpoint (Saya dual path)", by_type["CheckpointLoaderSimple"])
    ck2 = node(4, "CheckpointLoaderSimple", [-40, 150], [420, 100], [CKPT_2], "MODEL_2 · refiner checkpoint", by_type["CheckpointLoaderSimple"])
    enc = by_type["CLIPTextEncode"]
    # 2.0: MAIN = background only, built by SayaMainPrompt (prefix + a background from the stock, or the free text below);
    # ACTION = what the two characters do (encoded apart, received by the characters only); P1 / P2 = appearance.
    mp = node(5, "SayaMainPrompt", [420, -40], [520, 420], [prompts["PREFIX"], "free", 0, "fixed", "new", False] + ["none"] * 7 + [prompts["SCENE"], ""],
              "MAIN · prefix + background (free text, or pick a category + seed)",
              {"inputs": [], "outputs": [{"name": k, "type": "STRING"} for k in ("main_prompt", "background", "short_name", "info", "prefix", "scene")]}, GREEN)
    mp["inputs"] = []  # all widgets
    # 2.2: QUALITY (prefix + LoRA triggers) encoded apart and read first; MAIN encodes the scene only, read last.
    qual = node(26, "CLIPTextEncode", [420, 320], [520, 60], [""], "QUALITY · prefix + LoRA triggers (read first)", enc, GREEN)
    main = node(22, "CLIPTextEncode", [420, 400], [520, 60], [""], "MAIN · background only (read last)", enc, GREEN)
    act = node(23, "CLIPTextEncode", [420, 480], [520, 150], [prompts["ACTION"]], "ACTION · count, framing, LEFT / RIGHT, pose", enc, GREEN)
    p1s = node(24, "PrimitiveStringMultiline", [-40, 620], [420, 150], [prompts["P1"]], "P1 · RIGHT character (text)",
               {"inputs": [], "outputs": [{"name": "STRING", "type": "STRING"}]}, BLUE)
    p2s = node(25, "PrimitiveStringMultiline", [-40, 800], [420, 150], [prompts["P2"]], "P2 · LEFT character (text)",
               {"inputs": [], "outputs": [{"name": "STRING", "type": "STRING"}]}, RED)
    p1 = node(6, "CLIPTextEncode", [420, 660], [520, 60], [""], "P1 · encode", enc, BLUE)
    p2 = node(7, "CLIPTextEncode", [420, 740], [520, 60], [""], "P2 · encode", enc, RED)
    neg = node(8, "CLIPTextEncode", [420, 830], [520, 110], [prompts["NEGATIVE"]], "NEGATIVE", enc)
    for n in (act, neg):
        n["inputs"] = [i for i in n["inputs"] if i["name"] != "text"]
    for n in (qual, main, p1, p2):  # text comes from a link: keep the input, no widget value
        n["widgets_values"] = []
        for i in n["inputs"]:
            if i["name"] == "text":
                i["widget"] = {"name": "text"}
    lat = node(9, "EmptyLatentImage", [-40, 300], [420, 110], [832, 1216, 1], "Latent 832x1216",
               {"inputs": [], "outputs": [{"name": "LATENT", "type": "LATENT"}]})
    spl = node(10, "SayaSplitMask", [-40, 450], [420, 130], list(split["widgets_values"]), "Saya Split Mask (P1 | P2)", split, PURPLE)
    cw = list(couple["widgets_values"])  # strength_1, strength_2, solo, ownership, dynamic_start_sigma, background_main, zone_fallback, anchor_tokens, p1_anchors, p2_anchors, person_anchor
    cw[2] = False
    cpl = node(11, "SayaMultiCouple", [980, 0], [400, 420], cw + ["woman"] * (11 - len(cw)),
               "Saya Multi Couple · dynamic ownership", couple, PURPLE)
    if not any(i["name"] == "quality" for i in cpl["inputs"]):  # source saved before 2.2
        cpl["inputs"].append({"localized_name": "quality", "name": "quality", "shape": 7, "type": "CONDITIONING", "link": None})
    eps = node(12, "Epsilon Scaling", [1420, 0], [300, 60], list(chain["Epsilon Scaling"]["widgets_values"]), "Epsilon Scaling", chain["Epsilon Scaling"])
    czs = node(13, "CFGZeroStar", [1420, 100], [300, 30], [], "CFGZeroStar", chain["CFGZeroStar"])
    apg = node(14, "APG", [1420, 170], [300, 110], list(chain["APG"]["widgets_values"]), "APG", chain["APG"])
    pag = node(15, "PerturbedAttentionGuidance", [1420, 320], [300, 60], list(chain["PerturbedAttentionGuidance"]["widgets_values"]), "PAG", chain["PerturbedAttentionGuidance"])
    dbo = node(16, "ClownOptions_DetailBoost_Beta", [1420, 420], [300, 180], list(chain["ClownOptions_DetailBoost_Beta"]["widgets_values"]), "Detail Boost (Sampler 1 options)", chain["ClownOptions_DetailBoost_Beta"])
    clown = by_type["ClownsharKSampler_Beta"]
    w1 = [s1[k] for k in CLOWN_WIDGETS]
    w2 = [s2[k] for k in CLOWN_WIDGETS]
    for w in (w1, w2):
        w[CLOWN_WIDGETS.index("control_after_generate")] = "randomize"
    k1 = node(17, "ClownsharKSampler_Beta", [1760, 0], [340, 420], w1, "Sampler 1 · Shark (MODEL_1, base)", clown)
    k2 = node(18, "ClownsharKSampler_Beta", [2140, 0], [340, 420], w2, "Sampler 2 · Shark (MODEL_2, refine)", clown)
    seed = node(19, "PrimitiveNode", [1760, 460], [340, 90], [123456789, "randomize"], "SEED · random every run (shared by both samplers)",
                {"inputs": [], "outputs": [{"name": "INT", "type": "INT", "widget": {"name": "seed"}}]})
    seed["properties"] = {"Run widget replace on values": False}
    dec = node(20, "VAEDecode", [2520, 0], [220, 50], [], "VAE Decode", by_type["VAEDecode"])
    save = node(21, "SaveImage", [2520, 100], [460, 620], ["SayaCouple/demo"], "Result",
                {"inputs": [{"name": "images", "type": "IMAGE"}], "outputs": []})

    for n in (main, act, p1, p2, neg, qual):
        link(ck1, 1, n, "clip")
    link(mp, 5, main, "text"); link(mp, 4, qual, "text"); link(qual, 0, cpl, "quality"); link(p1s, 0, p1, "text"); link(p2s, 0, p2, "text")
    link(lat, 0, spl, "latent")
    link(ck1, 0, cpl, "model_1"); link(ck1, 1, cpl, "clip")
    link(spl, 0, cpl, "mask_1"); link(spl, 1, cpl, "mask_2")
    link(p1, 0, cpl, "pos_1"); link(neg, 0, cpl, "neg_1"); link(p2, 0, cpl, "pos_2"); link(neg, 0, cpl, "neg_2")
    link(ck2, 0, cpl, "model_2"); link(main, 0, cpl, "main"); link(act, 0, cpl, "action")
    link(p1s, 0, cpl, "p1_text"); link(p2s, 0, cpl, "p2_text")
    link(cpl, 0, eps, "model"); link(eps, 0, czs, "model"); link(czs, 0, apg, "model"); link(apg, 0, pag, "model")
    link(pag, 0, k1, "model"); link(cpl, 2, k1, "positive"); link(cpl, 3, k1, "negative"); link(lat, 0, k1, "latent_image"); link(dbo, 0, k1, "options")
    link(cpl, 1, k2, "model"); link(cpl, 2, k2, "positive"); link(neg, 0, k2, "negative"); link(k1, 0, k2, "latent_image")
    link(seed, 0, k1, "seed"); link(seed, 0, k2, "seed")
    link(k2, 0, dec, "samples"); link(ck1, 2, dec, "vae"); link(dec, 0, save, "images")

    wf = {"id": "saya-couple-demo", "revision": 0, "last_node_id": max(n["id"] for n in nodes), "last_link_id": len(links), "nodes": nodes, "links": links,
          "groups": [
              {"id": 1, "title": "1 · Models, latent, character texts", "bounding": [-60, -40, 460, 1000], "color": "#3f789e", "flags": {}},
              {"id": 2, "title": "2 · Prompts  MAIN / ACTION / P1 / P2", "bounding": [400, -60, 560, 1020], "color": "#8A8", "flags": {}},
              {"id": 3, "title": "3 · Saya Couple", "bounding": [960, -40, 440, 400], "color": "#a1309b", "flags": {}},
              {"id": 4, "title": "4 · Shark dual sampler (validated Saya setup)", "bounding": [1400, -40, 1100, 620], "color": "#b06634", "flags": {}},
              {"id": 5, "title": "5 · Output", "bounding": [2500, -40, 500, 780], "color": "#444", "flags": {}}],
          "config": {}, "extra": {"ds": {"scale": 0.55, "offset": [120, 330]}}, "version": 0.4}
    out = os.path.join(PKG, "workflows", "Saya_Couple_Demo.json")
    json.dump(wf, open(out, "w", encoding="utf-8"), indent=1, ensure_ascii=False)
    write_api_prompt(wf, a.object_info, os.path.join(PKG, "workflows", "Saya_Couple_Demo_api.json"))

    print("Sampler 1 (source effective -> demo):")
    for k in CLOWN_WIDGETS:
        print(f"  {k:24} {s1[k]!r:28} -> {w1[CLOWN_WIDGETS.index(k)]!r}")
    print("Sampler 2 (source effective -> demo):")
    for k in CLOWN_WIDGETS:
        print(f"  {k:24} {s2[k]!r:28} -> {w2[CLOWN_WIDGETS.index(k)]!r}")
    for name in ("Epsilon Scaling", "APG", "PerturbedAttentionGuidance", "ClownOptions_DetailBoost_Beta"):
        print(f"  {name:30} {chain[name]['widgets_values']}")
    bypassed = [n["type"] for n in sg1["nodes"] if n.get("mode") == 4]
    print("bypassed in source (not kept):", bypassed)
    print("wrote", out)


if __name__ == "__main__":
    main()

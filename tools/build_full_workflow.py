"""Builds workflows/Saya_Couple_Full.json: the maintainer's complete 6-phase workflow, made public-safe.

    python3 tools/build_full_workflow.py --source /path/to/validated_full_workflow.json

Structure, wiring and every sampler/pass setting are kept. Only these change:
- prompts -> workflows/demo_prompt.json (adult, clothed, non-sexual);
- personal checkpoints / VAEs / LoRAs / detector models -> SELECT_* placeholders, LoRA stacks emptied;
- the 13 detailer slots are named "Detailer 01" ... "Detailer 13" (titles, labels, rgthree group toggles);
  internal socket names are untouched so the graph keeps working;
- private notes in the workflow metadata are dropped.
"""

import argparse
import copy
import json
import os
import re

PKG = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DETAILERS = ["Body", "Head & Hair", "Face", "Full Eyes", "Eyes · One by One", "Breasts", "Hands", "Feet", "Buttocks", "Anus", "Vulva", "Penis", "NSFW"]
ALIASES = {"Eyes": "Eyes · One by One", "Hair": "Head & Hair", "Breast": "Breasts", "Ass": "Buttocks", "Pussy": "Vulva"}
# Public, well-known component files keep their names; every other model file becomes a SELECT_* placeholder.
PUBLIC_MODELS = {"RealESRGAN_x4plus_anime_6B.safetensors", "clip_l_hidream.safetensors", "clip_g_hidream.safetensors",
                 "t5xxl_fp8_e4m3fn.safetensors", "llama_3.1_8b_instruct_fp8_scaled.safetensors", "hidream-i1-full-Q5_K_M.gguf",
                 "sam_vit_b_01ec64.pth", "4x-UltraSharpV2_Lite.safetensors", "4x_foolhardy_Remacri.pth"}
FILE_RX = re.compile(r"[^\"\[\],]*?([^\"\[\],/]+\.(?:safetensors|gguf|ckpt|pth|pt))(?![A-Za-z0-9])")
MODEL_MAP = {}  # filled from the loaders of the source workflow (main/refiner checkpoints, VAEs)
DETECTOR = "segm/SELECT_DETECTOR_MODEL.pt"


def slot(name):
    name = ALIASES.get(name, name)
    return f"Detailer {DETAILERS.index(name) + 1:02d}"


# Longest names first so "Eyes · One by One" / "Full Eyes" / "Head & Hair" win over "Eyes" / "Hair".
_NAMES = sorted(set(DETAILERS) | set(ALIASES), key=len, reverse=True)
_RX = re.compile(r"(?<![\w])(" + "|".join(re.escape(n) for n in _NAMES) + r")(?![\w])")


def neutral_label(text):
    if not isinstance(text, str):
        return text
    if text.startswith("Detailer · "):  # group titles / rgthree matchTitle: "Detailer · Anus" -> "Detailer 10"
        return slot(text[len("Detailer · "):])
    return _RX.sub(lambda m: slot(m.group(1)), text)


def _placeholder(m):
    full, base = m.group(0), m.group(1)
    if base in PUBLIC_MODELS:
        return full
    if full in MODEL_MAP or base in MODEL_MAP:
        return MODEL_MAP.get(full, MODEL_MAP.get(base))
    return DETECTOR if base.endswith((".pt", ".pth")) else "SELECT_YOUR_MODEL.safetensors"


def scrub_models(value):
    if isinstance(value, str):
        return FILE_RX.sub(_placeholder, value)
    if isinstance(value, list):
        return [scrub_models(v) for v in value]
    if isinstance(value, dict):
        return {k: scrub_models(v) for k, v in value.items()}
    return value


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--source", required=True)
    a = ap.parse_args()
    wf = copy.deepcopy(json.load(open(a.source, encoding="utf-8")))
    prompts = json.load(open(os.path.join(PKG, "workflows", "demo_prompt.json"), encoding="utf-8"))
    # 2.0 grammar: MAIN (SayaMainPrompt node: prefix + background) / ACTION / P1 / P2 (identity + a separate anatomy text, emptied here) / NEGATIVE.
    by_title = {"Prompt · Base Scene": "MAIN", "Prompt · Person 1": "P1", "Prompt · Person 2": "P2", "Prompt · Negative": "NEGATIVE",
                "Prompt · Action": "ACTION", "Prompt · Anatomy P1": "", "Prompt · Anatomy P2": ""}
    MAIN_NODE_TITLE = "Prompt · MAIN (prefix + background + tags)"
    graphs = [wf] + wf["definitions"]["subgraphs"]
    ckpts = [n["widgets_values"][0] for g in graphs for n in g.get("nodes", []) if n["type"] == "CheckpointLoaderSimple"]
    for name, rep in zip(ckpts, ("SELECT_YOUR_MODEL.safetensors", "SELECT_YOUR_REFINER_MODEL.safetensors")):
        MODEL_MAP[name] = rep
    vaes = [(n.get("title", ""), n["widgets_values"][0]) for g in graphs for n in g.get("nodes", []) if n["type"] == "VAELoader"]
    k = 0
    for title, name in vaes:
        if "HiDream" in title:
            MODEL_MAP[name] = "SELECT_YOUR_HIDREAM_VAE.safetensors"
        elif name not in MODEL_MAP:
            k += 1
            MODEL_MAP[name] = f"SELECT_YOUR_VAE_{k}.safetensors"
    changed = {"prompts": 0, "lora_stacks": 0, "detectors": 0, "labels": 0}

    for g in graphs:
        for grp in g.get("groups", []):
            new = neutral_label(grp.get("title"))
            changed["labels"] += new != grp.get("title")
            grp["title"] = new
        for io in g.get("inputs", []) + g.get("outputs", []):
            if _RX.search(io.get("name", "")) or _RX.search(io.get("label", "") or ""):
                io["label"] = neutral_label(io.get("label") or io["name"])
                changed["labels"] += 1
        for n in g.get("nodes", []):
            props = n.setdefault("properties", {})
            for k in ("cnr_id", "aux_id"):
                if props.get(k) == "Saya Couple Upated":
                    props[k] = "Saya_Couple"
            if "title" in n:
                new = neutral_label(n["title"])
                changed["labels"] += new != n["title"]
                n["title"] = new
            if n["type"] == "Fast Groups Bypasser (rgthree)" and "matchTitle" in props:
                props["matchTitle"] = neutral_label(props["matchTitle"])
            for s in n.get("inputs", []) + n.get("outputs", []):
                if s.get("label") and _RX.search(s["label"]):
                    s["label"] = neutral_label(s["label"])
                    changed["labels"] += 1
            wv = n.get("widgets_values")
            if n.get("title") in by_title:
                key = by_title[n["title"]]
                n["widgets_values"] = [prompts[key] if key else ""]
                changed["prompts"] += 1
            elif n["type"] in ("SayaMainPromptFR", "SayaMainPrompt") and isinstance(wv, list):
                # The maintainer uses the French labels; the public workflow ships the English node, same engine.
                n["type"] = "SayaMainPrompt"
                n["title"] = MAIN_NODE_TITLE
                props["Node name for S&R"] = "SayaMainPrompt"
                names = ["prefix", "background", "seed", "history", "ultra_detailed", "lighting", "time", "weather", "atmosphere", "palette", "detail", "rating", "custom_background", "extra"]
                for inp, name in zip(n.get("inputs", []), names):
                    inp["name"] = name
                    inp["widget"] = {"name": name}
                for o, name in zip(n.get("outputs", []), ("main_prompt", "background", "short_name", "info")):
                    o["name"] = name
                n["widgets_values"] = [prompts["PREFIX"], "free", 0, "fixed", "new", False] + ["none"] * 7 + [prompts["SCENE"], ""]
                changed["prompts"] += 1
            elif n["type"] == "Lora Loader (LoraManager)":
                n["widgets_values"] = [{"version": 1, "textWidgetName": "text"}, "", []]
                changed["lora_stacks"] += 1
            elif n["type"] == "UltralyticsDetectorProvider":
                n["widgets_values"] = [DETECTOR]
                changed["detectors"] += 1
            elif n["type"] == "SayaHiDreamLoraSettings" and isinstance(wv, list):
                n["widgets_values"] = [False, "SELECT_HIDREAM_STYLE_LORA.safetensors", wv[2], ""]
            elif n["type"] == "SayaImageGenerationReview" and isinstance(wv, list):
                n["widgets_values"] = scrub_models([wv[0], wv[1], prompts["MAIN"]] + wv[3:])
                changed["prompts"] += 1
            elif n["type"] == "SayaCoupleImprintPackV2" and isinstance(wv, list):
                n["widgets_values"] = [("Saya_Couple_Full.json" if isinstance(v, str) and v.endswith(".json") else v) for v in wv]
            elif n["type"] == "SayaMultiCouple" and isinstance(wv, list) and len(wv) < 11:
                n["widgets_values"] = list(wv) + ["woman"] * (11 - len(wv))  # person_anchor (2.0): historic default
            if n.get("widgets_values") is not None:
                n["widgets_values"] = scrub_models(n["widgets_values"])

    # Socket names, localized names, promoted widget names: rename consistently (links use slot indexes).
    def rename_sockets(o):
        if isinstance(o, dict):
            for k in ("name", "localized_name", "label"):
                if isinstance(o.get(k), str) and _RX.search(o[k]):
                    o[k] = neutral_label(o[k])
                    changed["labels"] += 1
            for v in o.values():
                rename_sockets(v)
        elif isinstance(o, list):
            for v in o:
                rename_sockets(v)
    for g in graphs:
        for part in ("inputs", "outputs"):
            rename_sockets(g.get(part, []))
        for n in g.get("nodes", []):
            n.pop("widgets_values_named", None)  # frontend shadow copy; it would restore the private values
            rename_sockets(n.get("inputs", []))
            rename_sockets(n.get("outputs", []))
            props = n.get("properties", {})
            for k in [k for k in props if k.startswith("saya_source")]:
                del props[k]  # names of the maintainer's old workflow files
            for k in [k for k in props if k.startswith("saya_") and isinstance(props[k], str)]:
                props[k] = neutral_label(props[k])
        if g is not wf and isinstance(g.get("extra"), dict):
            g["extra"] = {k: v for k, v in g["extra"].items() if not k.startswith("saya_")}
    wf["extra"] = {k: v for k, v in wf.get("extra", {}).items() if k in ("ds", "frontendVersion")}
    note_id = max(n["id"] for n in wf["nodes"]) + 1
    x0, y0 = min(n["pos"][0] for n in wf["nodes"]), min(n["pos"][1] for n in wf["nodes"])
    wf["nodes"].append({
        "id": note_id, "type": "MarkdownNote", "pos": [x0, y0 - 420], "size": [900, 380], "flags": {}, "order": 0, "mode": 0,
        "inputs": [], "outputs": [], "properties": {}, "title": "READ ME FIRST",
        "widgets_values": ["# SELECT YOUR MODELS, THEN CLICK GENERATE\n\n"
                           "This is the complete 6-phase Saya Couple pipeline (sampling, hires / USDU, HiDream refine, pre-detail, "
                           "detailers, final upscale & naturalize). No model is included: pick your SDXL / Illustrious checkpoints, "
                           "VAEs, the HiDream models, the upscale model and one detector model per **Detailer 01-13** slot you use "
                           "(bypass the others with their toggles).\n\n"
                           "Prompts follow the 4-field grammar (docs/PROMPTS_GUIDE_SFW_EN.md): MAIN = background only (the "
                           "*Prompt · MAIN* node: prefix + a background from the stock or your own text), ACTION = what the two "
                           "characters do, P1 / P2 = their appearance. Phase 1 reads each pixel's owner from the model's own attention "
                           "(dynamic ownership) and hands that map to every later pass.\n\n"
                           "Needs Saya Couple 2.0 (`./saya install`, no ComfyUI core modification) and the custom nodes listed in the README "
                           "(section *Full workflow*). For a first test, use the simple demo workflow instead."]})
    wf["last_node_id"] = max(wf.get("last_node_id", 0), note_id)
    text = json.dumps(wf, indent=1, ensure_ascii=False)
    # Stable ids of the detailer hub selectors (only referenced inside this file): same rename everywhere.
    for i, name in enumerate(DETAILERS, 1):
        key = {"Head & Hair": "head-hair", "Full Eyes": "full-eyes", "Eyes · One by One": "eyes-one-by-one"}.get(name, name.lower())
        text = text.replace(f"saya-model-hub-detailers-selector-{key}\"", f"saya-model-hub-detailers-selector-detailer-{i:02d}\"")
    out = os.path.join(PKG, "workflows", "Saya_Couple_Full.json")
    open(out, "w", encoding="utf-8").write(text)
    print(changed)
    print("wrote", out)


if __name__ == "__main__":
    main()

"""Build a sanitized distribution copy; never write to the source workflow."""

import argparse
import hashlib
import json
from pathlib import Path
import re

BASE = ("best quality, detailed anime illustration, exactly two adults, one adult woman and one adult man, "
        "two-shot, both characters fully visible, standing together in a cozy modern apartment, "
        "smiling at each other while holding opposite ends of an open book, relaxed natural poses, "
        "normal casual clothing, large window, leafy houseplants, wooden bookshelf, sofa and woven rug, "
        "soft afternoon lighting, clean anime shading, detailed interior, balanced composition")
P1 = ("one adult woman, long dark hair, brown eyes, cream sweater and navy knee-length skirt, "
      "gentle smile, standing on the left, looking toward her partner, holding the left side of an open book")
P2 = ("one adult man, short brown hair, green eyes, casual blue jacket and beige trousers, "
      "calm smile, standing on the right, looking toward his partner, holding the right side of the open book")
NEG = ("worst quality, low quality, bad anatomy, bad hands, extra fingers, extra limbs, fused bodies, "
       "duplicate person, extra people, deformed face, blurry, watermark, text")


def set_widgets(node, values):
    node["widgets_values_named"] = values
    node["widgets_values"] = list(values.values())


def prepare(source, destination):
    raw = source.read_bytes()
    workflow = json.loads(raw)
    graphs = [workflow, *workflow["definitions"]["subgraphs"]]
    definitions = {g["id"]: g for g in graphs[1:]}
    prompts = {144: BASE, 807: P1, 808: P2, 145: NEG,
               12402: BASE + ", preserve composition and illustrated material texture",
               12403: P1 + ", preserve identity and clothing",
               12404: P2 + ", preserve identity and clothing", 12405: NEG}
    detail_graph = definitions["05a69a26-72ba-450b-8e6f-c65079c34f77"]
    parts = {group["title"].removeprefix("Bypass · "): f"Detailer {i}"
             for i, group in enumerate(detail_graph["groups"], 1)}
    parts["Hair"] = parts["Head & Hair"]
    phase_names = {g["id"]: f"Phase {g['id'][-1:].zfill(2)}"
                   for g in graphs[1:] if g["id"].startswith("saya-auto-phase-")}
    names = {**phase_names, "f997914d-608a-4c45-b914-7a66a3017156-hub-output-fix-v4": "Models",
             "3fdf6153-0555-4e71-9ac6-86af3073cfa9": "Resolution",
             "859aea13-20ba-4309-9ccc-eed45439bd29-conditioning-v4": "Couple MASTER",
             "590b2e2f-b1ea-474d-a1e9-cfe076ef8b83-forge-vae-exact-v2": "Sampler 1",
             "554b0689-f165-425d-9a7b-c39454ca0852": "Sampler 2",
             detail_graph["id"]: "Detailers",
             "239cd0bc-50ce-45c6-8aa4-82fd64c85ab0": "Naturalize Prompts"}
    keep_properties = {"cnr_id", "aux_id", "Node name for S&R", "proxyWidgets", "ue_properties",
                       "previousName", "__lm_widget_ids", "matchColors", "matchTitle", "showNav",
                       "showAllGraphs", "sort", "customSortAlphabet", "toggleRestriction",
                       "comparer_mode", "showOutputText", "horizontal", "randomMin", "randomMax",
                       "saya_phase", "saya_original_mode", "saya_shared_base"}
    lora_index = 0
    for graph in graphs:
        graph["extra"] = {}
        if graph is not workflow:
            graph["name"] = names.get(graph["id"], graph.get("name", "Settings"))
            if graph["name"] == "New Subgraph" or graph["name"].startswith("v19"):
                graph["name"] = "Upscale Model"
        for group in graph.get("groups", []):
            old = group["title"].removeprefix("Bypass · ")
            group["title"] = parts.get(old, "HiDream" if graph.get("id") == "saya-auto-phase-3" else group["title"])
        for node in graph["nodes"]:
            typ = node["type"]
            old_title = node.get("title", typ)
            node["properties"] = {k: v for k, v in node.get("properties", {}).items() if k in keep_properties}
            node["properties"].pop("ver", None)
            node["title"] = names.get(typ, old_title)
            node["title"] = re.sub(r"^(?:TECH · |v19 · |SHARED · )", "", node["title"])
            node["title"] = node["title"].replace(" · FORGE COARSE MAIN 2.3", "").replace(" · HARD RESET v19", "")
            if node["title"] == typ and typ in definitions:
                node["title"] = definitions[typ]["name"]
            for part, public in sorted(parts.items(), key=lambda x: -len(x[0])):
                if part in old_title and ("Detailer" in old_title or "Detector" in old_title or "Detect" in old_title or "Mask" in old_title or "Bypass" in old_title or "Compare" in old_title):
                    role = "" if typ == "SayaDetailerForEach" else " · " + ("Toggle" if "Bypasser" in typ else "Compare" if "Comparer" in typ else "Detector" if "Provider" in typ else "Mask")
                    node["title"] = public + role
                    if "matchTitle" in node["properties"]: node["properties"]["matchTitle"] = "^" + public + "$"
                    break
            if node["id"] in prompts:
                set_widgets(node, {"value": prompts[node["id"]]})
                node["title"] = {144:"Base Prompt",807:"Person 1 Prompt",808:"Person 2 Prompt",145:"Negative Prompt",
                                 12402:"Naturalize Base Prompt",12403:"Naturalize Person 1 Prompt",12404:"Naturalize Person 2 Prompt",12405:"Naturalize Negative Prompt"}[node["id"]]
            if typ == "Lora Loader (LoraManager)":
                lora_index += 1
                node["title"] = f"LoRA {lora_index}"
                set_widgets(node, {"__lm_autocomplete_meta_text": {"version":1,"textWidgetName":"text"}, "text":"", "loras":[]})
            if typ == "SayaHiDreamLoraSettings":
                set_widgets(node, {"enabled":False,"lora_name":"none","strength_model":1.0,"trigger_text":""})
                node["title"] = "HiDream LoRA and Trigger"
            if typ == "SayaHiDreamLoraLoader": node["title"] = "HiDream LoRA Loader"
            if typ == "SayaComfyCoupleHiDreamCopy": node["title"] = "HiDream Couple COPY"
            if typ == "SayaComfyCoupleForge": node["title"] = "Couple MASTER"
            if typ == "SayaComfyCoupleForgeCopy": node["title"] = "Couple COPY"
            if "Image Comparer" in typ: set_widgets(node, {"images": []})
            if typ == "SayaDualCLIPTextEncode": set_widgets(node, {"text":"","send_data":True,"naturalize_text":""})
            if re.fullmatch(r"SayaImagePhase\dStop", typ):
                set_widgets(node, {"seed":42,"positive_prompt":"","negative_prompt":"","models_json":"[]",
                                   "vaes_json":"[]","samplers_json":"{}","source_path":"","unload_after_phase":True})
                node["inputs"].append({"name":"unload_after_phase","type":"BOOLEAN","widget":{"name":"unload_after_phase"},"link":None})
            if typ == "SayaImageGenerationReview":
                set_widgets(node, {"seed":42,"positive_prompt":"","negative_prompt":"","models_json":"[]",
                                   "vaes_json":"[]","samplers_json":"{}","checkpoint_root":"image/checkpoints"})
                node["title"] = "Phase 01 Review"
            if typ in {"SayaDetailerForEach", "DetailDaemonSamplerNode", "CFGZeroStar", "Epsilon Scaling", "EasyColorCorrection", "SayaUSDU1IdentitySafe", "SayaUSDU2IdentitySafe"}:
                node["mode"] = 4
                if "saya_original_mode" in node["properties"]:node["properties"]["saya_original_mode"] = 4
            named = node.get("widgets_values_named", {})
            for key in list(named):
                if key in {"seed","noise_seed","seed_value"}:named[key] = 42
                if key in {"scheduler","scheduler_name"} and named[key] == "beta45": named[key] = "beta"
                if key == "structure_preservation":named[key] = 0.0
                if key == "download_civitai_data":named[key] = False
            # Ordinary nodes store widgets in the same order as their named map.
            if named and len(named) == len(node.get("widgets_values", [])):
                node["widgets_values"] = list(named.values())
            if typ == "Seed (rgthree)": set_widgets(node, {"seed":42,"lastSeed":"","lastSeedButton":"","randomize":""})
            if typ == "CheckpointLoaderSimple":
                set_widgets(node, {"ckpt_name":"SELECT_SDXL_CHECKPOINT.safetensors"})
                node["title"] = "Base Model" if node["id"] == 488 else "Refiner Model"
            if typ == "UnetLoaderGGUF":set_widgets(node, {"unet_name":"SELECT_HIDREAM_MODEL.gguf"})
            if typ == "VAELoader":set_widgets(node, {"vae_name":"SELECT_HIDREAM_VAE.safetensors" if node["id"] == 10149 else "SELECT_SDXL_VAE.safetensors"})
            if typ == "UpscaleModelLoader":set_widgets(node, {"model_name":"SELECT_UPSCALE_MODEL.pth"})
            if typ == "UltralyticsDetectorProvider":set_widgets(node, {"model_name":"bbox/SELECT_DETECTOR.pt"})
            if typ == "SAMLoader":set_widgets(node, {"model_name":"SELECT_SAM_MODEL.pth","device_mode":"AUTO"})
            if typ == "CustomCombo":
                node["widgets_values"][0:2] = ["Main Model VAE", 0]
                if named:
                    keys = list(named);named[keys[0]] = "Main Model VAE";named[keys[1]] = 0

    for graph in graphs:
        for node in graph["nodes"]:
            if node["type"] in definitions:
                node["title"] = definitions[node["type"]]["name"]
    for i, group in enumerate(workflow.get("groups", [])):
        group["title"] = ["Settings", "Outputs", "Prompts", "LoRA", "Detailers"][i]

    # Clear filenames in promoted widget values as well as nested metadata.
    def sanitize(value):
        if isinstance(value, dict):return {k:sanitize(v) for k,v in value.items()}
        if isinstance(value, list):return [sanitize(v) for v in value]
        if not isinstance(value, str):return value
        if value.endswith((".safetensors", ".ckpt", ".gguf", ".pth", ".pt")) and "SELECT_" not in value:
            return "SELECT_MODEL" + Path(value).suffix
        for word in ("Breasts","Buttocks","Anus","Vulva","Penis","NSFW"):
            value = re.sub(r"\b"+word+r"\b", parts[word], value, flags=re.I)
        return value
    workflow = sanitize(workflow)
    graphs = [workflow, *workflow["definitions"]["subgraphs"]]
    # Optional HiDream: the lazy image switch prevents both TE and MODEL load.
    hd = next(g for g in graphs[1:] if g["id"] == "saya-auto-phase-3")
    all_nodes = [n for g in graphs for n in g["nodes"]]
    switch_id = max(n["id"] for n in all_nodes) + 1
    all_links = [l for g in graphs for l in g["links"]]
    first_link = max(l["id"] if isinstance(l,dict) else l[0] for l in all_links) + 1
    link = next(l for l in hd["links"] if l["id"] == 40202)
    link["target_id"], link["target_slot"] = switch_id, 2
    hd["links"].extend([
        {"id":first_link,"origin_id":10150,"origin_slot":0,"target_id":switch_id,"target_slot":1,"type":"IMAGE"},
        {"id":first_link+1,"origin_id":switch_id,"origin_slot":0,"target_id":10145,"target_slot":0,"type":"IMAGE"}])
    next(n for n in hd["nodes"] if n["id"] == 10145)["inputs"][0]["link"] = first_link+1
    hd["nodes"].append({"id":switch_id,"type":"LazySwitchKJ","title":"Enable HiDream","pos":[2440,7440],"size":[340,110],"flags":{},"order":12,"mode":0,
        "inputs":[{"name":"switch","type":"BOOLEAN","widget":{"name":"switch"},"link":None},
                  {"name":"on_false","type":"IMAGE","link":first_link},
                  {"name":"on_true","type":"IMAGE","link":40202}],
        "outputs":[{"name":"IMAGE","type":"IMAGE","links":[first_link+1]}],
        "properties":{"cnr_id":"comfyui-kjnodes","Node name for S&R":"LazySwitchKJ"},
        "widgets_values":[False],"widgets_values_named":{"switch":False}})
    hd["state"]["lastNodeId"] = switch_id
    hd["state"]["lastLinkId"] = first_link+1
    workflow["last_node_id"] = switch_id
    workflow["last_link_id"] = first_link+1
    workflow["revision"] = 1
    # Rebuild reciprocal references from actual links (source has stale outputs).
    for graph in graphs:
        links = [l if isinstance(l,dict) else dict(zip(("id","origin_id","origin_slot","target_id","target_slot","type"),l)) for l in graph["links"]]
        for node in graph["nodes"]:
            for slot, output in enumerate(node["outputs"]):
                output["links"] = [l["id"] for l in links if l["origin_id"] == node["id"] and l["origin_slot"] == slot]
        for slot, port in enumerate(graph.get("inputs", [])):
            port["linkIds"] = [l["id"] for l in links if l["origin_id"] == -10 and l["origin_slot"] == slot]
        for slot, port in enumerate(graph.get("outputs", [])):
            port["linkIds"] = [l["id"] for l in links if l["target_id"] == -20 and l["target_slot"] == slot]
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(workflow, ensure_ascii=False, indent=2)+"\n")
    assert source.read_bytes() == raw, "Source workflow changed"
    print(f"Created {destination}; source SHA256 {hashlib.sha256(raw).hexdigest()}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    if args.source.resolve() == args.destination.resolve():parser.error("source and destination must differ")
    prepare(args.source, args.destination)

"""Loads the demo workflow in a running ComfyUI's real frontend, checks every node type is registered,
converts it to an API prompt, optionally compares the Shark dual sampling core with a reference API prompt,
then queues ONE generation and saves the result.

    python tests/live_demo_check.py --url http://127.0.0.1:8189 --workflow workflows/Saya_Couple_Demo.json \
        --out result.png [--reference validated_api.json] [--chromium /path/to/headless_shell]

Needs `pip install playwright` (only for this check).
"""

import argparse
import io
import json
import sys
import time
import urllib.parse
import urllib.request

from playwright.sync_api import sync_playwright

SAMPLER_KEYS = ("eta", "sampler_name", "scheduler", "steps", "steps_to_run", "denoise", "cfg", "sampler_mode", "bongmath")


def get(url):
    return json.load(urllib.request.urlopen(url, timeout=15))


def by_class(prompt, cls):
    return [v for v in prompt.values() if v["class_type"] == cls]


def literal(inputs):
    return {k: v for k, v in inputs.items() if not isinstance(v, list)}


def compare(demo, ref):
    rows, ok = [], True
    d_s = sorted(by_class(demo, "ClownsharKSampler_Beta"), key=lambda v: -v["inputs"]["steps"])
    r_s = sorted(by_class(ref, "ClownsharKSampler_Beta"), key=lambda v: -v["inputs"]["steps"])
    for label, d, r in zip(("Sampler 1", "Sampler 2"), d_s, r_s):
        for k in SAMPLER_KEYS:
            same = d["inputs"].get(k) == r["inputs"].get(k)
            ok &= same
            rows.append(f"  {label:10} {k:14} demo={d['inputs'].get(k)!r:30} ref={r['inputs'].get(k)!r:30} {'OK' if same else 'DIFF'}")
    for cls in ("Epsilon Scaling", "CFGZeroStar", "APG", "PerturbedAttentionGuidance", "ClownOptions_DetailBoost_Beta", "SayaSplitMask"):
        d, r = by_class(demo, cls), by_class(ref, cls)
        same = bool(d) and bool(r) and literal(d[0]["inputs"]) == literal(r[0]["inputs"])
        ok &= same
        rows.append(f"  {cls:30} demo={literal(d[0]['inputs']) if d else None} ref={literal(r[0]['inputs']) if r else None} {'OK' if same else 'DIFF'}")
    dc, rc = by_class(demo, "SayaMultiCouple"), by_class(ref, "SayaMultiCouple")
    for k in ("strength_1", "strength_2", "dual_attention_enabled"):
        same = dc[0]["inputs"].get(k) == rc[0]["inputs"].get(k)
        ok &= same
        rows.append(f"  SayaMultiCouple {k:24} demo={dc[0]['inputs'].get(k)!r} ref={rc[0]['inputs'].get(k)!r} {'OK' if same else 'DIFF'}")
    rows.append(f"  SayaMultiCouple solo           demo={dc[0]['inputs'].get('solo')!r} (ref: linked to the Couple/Solo switch = False)")
    return ok, rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://127.0.0.1:8188")
    ap.add_argument("--workflow", required=True)
    ap.add_argument("--out", help="where to save the generated image")
    ap.add_argument("--reference")
    ap.add_argument("--chromium")
    ap.add_argument("--no-generate", action="store_true")
    ap.add_argument("--models", help="MODEL_1,MODEL_2 checkpoint names to select in the loaders, like a user would")
    ap.add_argument("--save-api", help="also write the API-format prompt produced by the frontend")
    a = ap.parse_args()
    wf = json.load(open(a.workflow, encoding="utf-8"))
    with sync_playwright() as p:
        kw = {"args": ["--use-gl=swiftshader", "--enable-unsafe-swiftshader", "--no-sandbox"]}
        if a.chromium:
            kw["executable_path"] = a.chromium
        b = p.chromium.launch(**kw)
        pg = b.new_page(viewport={"width": 1600, "height": 1000})
        pg.goto(a.url, wait_until="load")
        pg.wait_for_function("window.app && window.app.graph", timeout=120000)
        pg.wait_for_timeout(3000)
        pg.evaluate("wf => window.app.loadGraphData(wf)", wf)
        pg.wait_for_timeout(4000)
        info = pg.evaluate("""() => {
            const reg = window.LiteGraph.registered_node_types;
            const nodes = window.app.graph._nodes;
            return {count: nodes.length, missing: nodes.filter(n => !reg[n.type] && !['PrimitiveNode','MarkdownNote','Note'].includes(n.type)).map(n => n.type)};
        }""")
        print(f"frontend: {info['count']} nodes loaded, missing node types: {info['missing'] or 'none'}")
        public_api = pg.evaluate("async () => await window.app.graphToPrompt()")["output"]
        api = public_api
        if a.models:
            m1, m2 = a.models.split(",")
            pg.evaluate("""([m1, m2]) => {
                for (const n of window.app.graph._nodes) {
                    if (n.type !== 'CheckpointLoaderSimple') continue;
                    n.widgets[0].value = (n.title || '').startsWith('MODEL_2') ? m2 : m1;
                }}""", [m1, m2])
            api = pg.evaluate("async () => await window.app.graphToPrompt()")["output"]
        b.close()
    print(f"API prompt: {len(api)} executable nodes")
    ok = not info["missing"]
    if a.save_api:
        json.dump(public_api, open(a.save_api, "w", encoding="utf-8"), indent=1, ensure_ascii=False)
        print("API prompt (public, placeholders) written to", a.save_api)
    if a.models:
        diffs = [(k, f) for k in api for f in api[k]["inputs"] if api[k]["inputs"][f] != public_api.get(k, {}).get("inputs", {}).get(f)]
        only_ckpt = bool(diffs) and all(api[k]["class_type"] == "CheckpointLoaderSimple" and f == "ckpt_name" for k, f in diffs)
        print(f"model selection changed only ckpt_name: {only_ckpt}  ({diffs})")
        ok &= only_ckpt
    if a.reference:
        same, rows = compare(api, json.load(open(a.reference, encoding="utf-8")))
        print("Shark dual core vs reference:")
        print("\n".join(rows))
        print("Shark dual core:", "IDENTICAL" if same else "DIFFERENT")
        ok &= same
    if not a.no_generate:
        body = json.dumps({"prompt": api, "client_id": "saya-demo-check"}).encode()
        pid = json.load(urllib.request.urlopen(urllib.request.Request(a.url + "/prompt", body, {"Content-Type": "application/json"})))["prompt_id"]
        t0 = time.time()
        while True:
            h = get(f"{a.url}/history/{pid}")
            if pid in h and h[pid]["status"]["status_str"] in ("success", "error"):
                break
            if time.time() - t0 > 1800:
                sys.exit("TIMEOUT")
            time.sleep(2)
        st = h[pid]["status"]["status_str"]
        print(f"generation: {st} in {time.time() - t0:.0f}s")
        if st != "success":
            print(json.dumps(h[pid]["status"])[:2000])
            sys.exit(1)
        img = [o for n in h[pid]["outputs"].values() for o in n.get("images", []) if o.get("type") == "output"][0]
        q = urllib.parse.urlencode({"filename": img["filename"], "subfolder": img["subfolder"], "type": "output"})
        with urllib.request.urlopen(f"{a.url}/view?{q}") as r:
            data = r.read()
        from PIL import Image  # re-encode without metadata: no prompt / model names in the published image
        Image.open(io.BytesIO(data)).save(a.out)
        print("saved", a.out)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

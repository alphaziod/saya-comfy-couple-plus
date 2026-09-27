"""Test campaigns used to choose the PERSON gain. Queues images on a running ComfyUI, one at a time.

  RNG (a new random seed per image, the way people really click Generate):
    python tests/run_campaign.py rng --tag g078 --n 10 --out campaign/
  Themed scenes (same characters, different worlds; see tests/themes_v1.json):
    python tests/run_campaign.py themes --tag g078 --n 3 --out themes/

The gain lives in comfy/ldm/modules/attention.py (SAYA_LOCKED_DELTA_PERSON_GAIN): change it, restart
ComfyUI, then run the next tag. Uses workflows/Saya_Couple_Demo_api.json (nodes found by their titles).
Images are named <tag>_seed<seed>.png (or <scene>_<tag>_seed<seed>.png); runs.json keeps times.
"""

import argparse
import glob
import json
import os
import secrets
import sys
import time
import urllib.parse
import urllib.request

PKG = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def get(url):
    return json.load(urllib.request.urlopen(url, timeout=15))


def node_by_title(api, prefix):
    for k, v in api.items():
        if v.get("_meta", {}).get("title", "").startswith(prefix):
            return k
    sys.exit(f"no node titled '{prefix}...' in the API prompt")


def generate(url, api, seed, out_png):
    p = json.loads(json.dumps(api))
    for v in p.values():
        if v["class_type"] == "ClownsharKSampler_Beta":
            v["inputs"]["seed"] = seed
    q = get(url + "/queue")
    if q["queue_running"] or q["queue_pending"]:
        sys.exit("ComfyUI queue is not empty: stopping (one job at a time)")
    body = json.dumps({"prompt": p, "client_id": "saya-campaign"}).encode()
    pid = json.load(urllib.request.urlopen(urllib.request.Request(url + "/prompt", body, {"Content-Type": "application/json"})))["prompt_id"]
    t0 = time.time()
    while True:
        h = get(f"{url}/history/{pid}")
        if pid in h and h[pid]["status"]["status_str"] in ("success", "error"):
            break
        if time.time() - t0 > 1800:
            sys.exit("TIMEOUT")
        time.sleep(2)
    if h[pid]["status"]["status_str"] != "success":
        sys.exit(json.dumps(h[pid]["status"])[:2000])
    img = [o for n in h[pid]["outputs"].values() for o in n.get("images", []) if o.get("type") == "output"][0]
    q = urllib.parse.urlencode({"filename": img["filename"], "subfolder": img["subfolder"], "type": "output"})
    with urllib.request.urlopen(f"{url}/view?{q}") as r, open(out_png, "wb") as f:
        f.write(r.read())
    return round(time.time() - t0, 1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=("rng", "themes"))
    ap.add_argument("--tag", required=True, help="e.g. g078 (the gain currently set in the core)")
    ap.add_argument("--n", type=int, default=10)
    ap.add_argument("--out", required=True)
    ap.add_argument("--url", default="http://127.0.0.1:8188")
    ap.add_argument("--api", default=os.path.join(PKG, "workflows", "Saya_Couple_Demo_api.json"))
    ap.add_argument("--themes", default=os.path.join(PKG, "tests", "themes_v1.json"))
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    api = json.load(open(a.api, encoding="utf-8"))
    ids = {k: node_by_title(api, k) for k in ("MAIN", "P1", "P2", "NEGATIVE")}
    runs_f = os.path.join(a.out, "runs.json")
    runs = json.load(open(runs_f)) if os.path.exists(runs_f) else {}
    jobs = []
    if a.mode == "rng":
        jobs.append((f"{a.tag}", api))
    else:
        t = json.load(open(a.themes, encoding="utf-8"))
        for sc in t["scenes"]:
            p = json.loads(json.dumps(api))
            p[ids["MAIN"]]["inputs"]["text"] = t["common_main_head"] + sc["main"] + t["common_main_tail"]
            p[ids["P1"]]["inputs"]["text"] = t["p1_base"] + sc["p1"]
            p[ids["P2"]]["inputs"]["text"] = t["p2_base"] + sc["p2"]
            p[ids["NEGATIVE"]]["inputs"]["text"] = t["negative"]
            jobs.append((f"{sc['id']}_{a.tag}", p))
    for prefix, p in jobs:
        while len(glob.glob(os.path.join(a.out, f"{prefix}_seed*.png"))) < a.n:
            seed = secrets.randbelow(2**48)
            name = f"{prefix}_seed{seed}"
            dt = generate(a.url, p, seed, os.path.join(a.out, name + ".png"))
            runs[name] = {"seed": seed, "time_s": dt}
            json.dump(runs, open(runs_f, "w"), indent=1)
            print(f"{name}  {dt}s", flush=True)
    print("DONE")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""F03 RAM/VRAM protocol. Read-only unless --release --confirm-release.

Never run the release path while ComfyUI is generating. The default mode only
reads /system_stats, /queue and /proc and prints a JSON report. A release is
reported as released=true only when Torch reservations fall by at least the
threshold on the same devices; otherwise released=false.
"""
import argparse, json, time, urllib.request

def get(base, path):
    with urllib.request.urlopen(base + path, timeout=10) as r:
        return json.loads(r.read().decode())

def post(base, path, body):
    req = urllib.request.Request(base + path, data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(req, timeout=10) as r:
        return json.loads(r.read().decode() or "{}")

def rss(pid):
    if not pid:
        return None
    try:
        for line in open(f"/proc/{pid}/status"):
            if line.startswith("VmRSS:"):
                return int(line.split()[1]) * 1024
    except OSError:
        return None
    return None

def devices(stats):
    out = []
    for d in stats.get("devices") or []:
        if d.get("type") not in ("cuda", "xpu", "npu", "mlu"):
            continue
        if "vram_total" not in d or "vram_free" not in d:
            continue
        out.append({"name": d.get("name"), "type": d.get("type"), "index": d.get("index"),
                    "vramTotal": d["vram_total"], "vramFree": d["vram_free"],
                    "torchReserved": d.get("torch_vram_total")})
    return out

def sample(base, pid):
    stats = get(base, "/system_stats")
    return {"at": time.time(), "devices": devices(stats),
            "systemRamFree": (stats.get("system") or {}).get("ram_free"),
            "comfyRss": rss(pid)}

def torch_total(s):
    vals = [d["torchReserved"] for d in s["devices"] if d["torchReserved"] is not None]
    return sum(vals) if vals else None

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--base-url", default="http://127.0.0.1:8188")
    ap.add_argument("--comfy-pid", type=int)
    ap.add_argument("--threshold-mib", type=float, default=64)
    ap.add_argument("--settle-seconds", type=float, default=10)
    ap.add_argument("--release", action="store_true", help="send POST /free")
    ap.add_argument("--confirm-release", action="store_true")
    a = ap.parse_args()
    base = a.base_url.rstrip("/")
    if a.release and not a.confirm_release:
        raise SystemExit("--release exige --confirm-release")
    before = sample(base, a.comfy_pid)
    report = {"mode": "release" if a.release else "measure-only", "before": before,
              "queue": get(base, "/queue"), "released": False}
    if a.release:
        q = report["queue"]
        if q.get("queue_running") or q.get("queue_pending"):
            raise SystemExit("ComfyUI occupé : déchargement refusé")
        post(base, "/free", {"unload_models": True, "free_memory": True})
        deadline = time.time() + a.settle_seconds
        after = sample(base, a.comfy_pid)
        b, x = torch_total(before), torch_total(after)
        same = [(d.get("name"), d.get("type"), d.get("index"), d.get("vramTotal"))
                for d in before["devices"]] == \
               [(d.get("name"), d.get("type"), d.get("index"), d.get("vramTotal"))
                for d in after["devices"]]
        while not (same and b is not None and x is not None and b > x
                   and b - x >= a.threshold_mib * 1048576) and time.time() < deadline:
            time.sleep(0.25)
            after = sample(base, a.comfy_pid)
            x = torch_total(after)
        report.update({"after": after, "released": bool(same and b is not None and x is not None
                       and b > x and b - x >= a.threshold_mib * 1048576),
                       "torchBefore": b, "torchAfter": x,
                       "thresholdMiB": a.threshold_mib})
    print(json.dumps(report, indent=2, ensure_ascii=False))

if __name__ == "__main__":
    main()

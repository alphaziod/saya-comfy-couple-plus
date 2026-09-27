"""Runs the installer's capability checks on past ComfyUI releases and records the result in compatibility.json.

    python3 tools/scan_history.py --repo /path/to/ComfyUI.git [--tags v0.3.40 v0.34.0 ...]

--repo is any clone of comfy-org/ComfyUI (a bare, blob-less clone is enough). Nothing is executed from the
scanned versions: only their source files are read. MultiMaskCouple is assumed installed (it is a separate
custom node, not part of ComfyUI).
"""

import argparse
import datetime
import json
import os
import subprocess
import sys
import tempfile

PKG = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(PKG, "installer"))
from saya_installer import compat  # noqa: E402

PATHS = ["comfy", "comfy_extras", "nodes.py", "server.py", "folder_paths.py", "node_helpers.py", "latent_preview.py", "execution.py", "comfyui_version.py", "main.py"]


def git(repo, *args):
    return subprocess.run(["git", "--git-dir", repo, *args], capture_output=True, text=True, check=True).stdout


def default_tags(repo):
    tags = [t for t in git(repo, "tag", "--sort=creatordate").split() if t.startswith("v0.")]
    last = {}
    for t in tags:  # last patch release of every minor + a few v0.3.x points (v0.3 spans a long period)
        parts = t.lstrip("v").split(".")
        if len(parts) != 3 or not all(p.isdigit() for p in parts):
            continue
        key = f"{parts[0]}.{parts[1]}" if parts[1] != "3" else f"0.3.{int(parts[2]) // 10}"
        last[key] = t
    return list(last.values())


def scan(repo, ref):
    with tempfile.TemporaryDirectory(prefix="saya_hist_") as d:
        existing = [p for p in PATHS if git(repo, "ls-tree", "--name-only", ref, p).strip()]
        tar = subprocess.run(["git", "--git-dir", repo, "archive", ref, *existing], capture_output=True, check=True).stdout
        subprocess.run(["tar", "-x", "-C", d], input=tar, check=True)
        os.makedirs(os.path.join(d, "custom_nodes", "MultiMaskCouple"), exist_ok=True)
        axes = compat.check_all(d, PKG)
        return {a.name: {"status": a.status, "failures": a.failures[:6]} for a in axes}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True)
    ap.add_argument("--tags", nargs="*")
    a = ap.parse_args()
    tags = a.tags or default_tags(a.repo)
    rows = []
    for t in tags:
        date = git(a.repo, "log", "-1", "--format=%ad", "--date=short", t).strip()
        res = scan(a.repo, t)
        row = {"ref": t, "date": date, **{k: v["status"] for k, v in res.items()}, "failures": {k: v["failures"] for k, v in res.items() if v["failures"]}}
        rows.append(row)
        print(f"{t:10} {date}  " + " | ".join(f"{k.split(' (')[0]}: {v['status']}" for k, v in res.items()), flush=True)
    path = os.path.join(PKG, "compatibility.json")
    data = json.load(open(path))
    data["history_scan"] = {"scanned_on": datetime.date.today().isoformat(), "method": "static capability checks of the installer, MultiMaskCouple assumed present", "results": rows}
    json.dump(data, open(path, "w"), indent=1)


if __name__ == "__main__":
    main()

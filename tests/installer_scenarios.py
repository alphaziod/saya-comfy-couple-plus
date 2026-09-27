"""End-to-end installer scenarios on throwaway copies of a clean ComfyUI.

    python3 tests/installer_scenarios.py --clean /path/to/clean/ComfyUI --python /path/to/comfyui/python \
        [--old /path/to/incompatible/ComfyUI]

The clean ComfyUI must be an untouched git checkout (+ MultiMaskCouple and RES4LYF in custom_nodes).
Every scenario runs on its own copy; the given folders are never modified.
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

PKG = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SAYA = [sys.executable, os.path.join(PKG, "installer", "saya.py")]
CORE = ("comfy/ldm/modules/attention.py", "comfy/model_management.py")
results = []


def sha(path):
    if not os.path.isfile(path):
        return None
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def tree_hash(root):
    h = hashlib.sha256()
    for dirpath, dirs, files in os.walk(root):
        dirs[:] = sorted(d for d in dirs if d not in (".git", "__pycache__"))
        for f in sorted(files):
            p = os.path.join(dirpath, f)
            h.update(os.path.relpath(p, root).encode())
            h.update((sha(p) or "").encode())
    return h.hexdigest()


def run(args, root, py, env=None, stdin=""):
    e = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
    e.update(env or {})
    r = subprocess.run(SAYA + args + ["--comfyui", root, "--python", py], capture_output=True, text=True, input=stdin, env=e, timeout=1800)
    return r.returncode, r.stdout + r.stderr


def check(name, cond, detail=""):
    results.append((name, bool(cond), detail))
    print(f"{'PASS' if cond else 'FAIL'} {name}" + (f"  ({detail})" if detail and not cond else ""), flush=True)


def copy_tree(src, tmp, name):
    dst = os.path.join(tmp, name)
    shutil.copytree(src, dst, symlinks=True)
    return dst


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--clean", required=True)
    ap.add_argument("--python", required=True)
    ap.add_argument("--old", help="an incompatible (too old) ComfyUI tree, optional")
    a = ap.parse_args()
    tested = json.load(open(os.path.join(PKG, "compatibility.json")))["officially_tested"][0]
    upstream = {p: sha(os.path.join(a.clean, p)) for p in CORE}

    with tempfile.TemporaryDirectory(prefix="saya_scen_") as tmp:
        # A. fresh install with the AMD patch, verify, idempotent re-install, restore, re-install
        r = copy_tree(a.clean, tmp, "A")
        code, out = run(["install", "--yes", "--amd", "yes"], r, a.python)
        check("A1 fresh install exits 0", code == 0, out[-600:])
        check("A2 attention.py == tested patched file", sha(os.path.join(r, CORE[0])) == tested["files_patched"][CORE[0]])
        check("A3 model_management.py == tested patched file", sha(os.path.join(r, CORE[1])) == tested["files_patched"][CORE[1]])
        code, out = run(["verify"], r, a.python)
        check("A4 verify RESULT OK", code == 0 and "RESULT        : OK" in out, out[-800:])
        state1 = json.load(open(os.path.join(r, ".saya_install.json")))
        code, out = run(["install", "--yes", "--amd", "yes"], r, a.python)
        state2 = json.load(open(os.path.join(r, ".saya_install.json")))
        check("A5 re-install idempotent (0 files copied)", code == 0 and "0 copied" in out, out[-600:])
        check("A6 re-install keeps the ORIGINAL backup", state1["backup"] == state2["backup"] and state1["core_files"] == state2["core_files"])
        code, out = run(["restore", "--yes"], r, a.python)
        check("A7 restore exits 0", code == 0, out[-600:])
        check("A8 core byte-identical to before install", all(sha(os.path.join(r, p)) == upstream[p] for p in CORE))
        g = subprocess.run(["git", "status", "--porcelain", "--untracked-files=no"], cwd=r, capture_output=True, text=True).stdout.strip()
        check("A9 git sees no tracked change after restore", g == "", g)
        check("A10 custom node removed, install record closed", not os.path.exists(os.path.join(r, "custom_nodes/Saya_Couple")) and not os.path.exists(os.path.join(r, ".saya_install.json")))
        code, out = run(["install", "--yes", "--amd", "no"], r, a.python)
        check("A11 install again after restore", code == 0 and sha(os.path.join(r, CORE[1])) == upstream[CORE[1]], out[-600:])

        # B. restore refuses to silently overwrite a file changed after install
        r = copy_tree(a.clean, tmp, "B")
        run(["install", "--yes", "--amd", "no"], r, a.python)
        with open(os.path.join(r, CORE[0]), "a", encoding="utf-8") as f:
            f.write("\n# user edit after install\n")
        edited = sha(os.path.join(r, CORE[0]))
        code, out = run(["restore"], r, a.python, stdin="\n")
        check("B1 drift detected and restore cancelled by default", code != 0 and "changed since install" in out and sha(os.path.join(r, CORE[0])) == edited, out[-600:])
        code, out = run(["restore", "--force"], r, a.python)
        check("B2 restore --force puts back the original", code == 0 and sha(os.path.join(r, CORE[0])) == upstream[CORE[0]], out[-600:])

        # C. core already modified by someone else -> fail closed, nothing touched
        r = copy_tree(a.clean, tmp, "C")
        p = os.path.join(r, CORE[0])
        txt = open(p, encoding="utf-8").read().replace("if block_attn2 in attn2_replace_patch:", "if block_attn2 in attn2_replace_patch:  # other extension", 1)
        open(p, "w", encoding="utf-8").write(txt)
        before = tree_hash(r)
        code, out = run(["install", "--yes"], r, a.python)
        check("C1 modified core -> UNSUPPORTED, exit != 0", code != 0 and "UNSUPPORTED" in out, out[-600:])
        check("C2 modified core -> nothing written", tree_hash(r) == before)

        # D. missing required dependency MultiMaskCouple -> refused
        r = copy_tree(a.clean, tmp, "D")
        shutil.rmtree(os.path.join(r, "custom_nodes/MultiMaskCouple"))
        before = tree_hash(r)
        code, out = run(["install", "--yes"], r, a.python)
        check("D1 missing MultiMaskCouple -> refused, nothing written", code != 0 and tree_hash(r) == before, out[-400:])

        # E. simulated NVIDIA -> AMD patch never offered
        r = copy_tree(a.clean, tmp, "E")
        code, out = run(["install", "--yes"], r, a.python, env={"SAYA_FAKE_GPU": json.dumps({"vendor": "nvidia", "backend": "cuda 12.4", "name": "sim RTX", "vram_gib": 8})})
        check("E1 NVIDIA: AMD patch not offered, model_management untouched", code == 0 and "Not offered" in out and sha(os.path.join(r, CORE[1])) == upstream[CORE[1]], out[-600:])

        # F. simulated AMD 24 GB -> optional, default NO
        r = copy_tree(a.clean, tmp, "F")
        code, out = run(["install", "--yes"], r, a.python, env={"SAYA_FAKE_GPU": json.dumps({"vendor": "amd", "backend": "rocm 6.4", "name": "sim 7900 XTX", "vram_gib": 23.98})})
        check("F1 AMD 24 GB: information only, not applied", code == 0 and "Information only" in out and "Apply the AMD" not in out and sha(os.path.join(r, CORE[1])) == upstream[CORE[1]], out[-600:])

        # G. simulated AMD 20 GB -> recommended, default YES
        r = copy_tree(a.clean, tmp, "G")
        code, out = run(["install", "--yes"], r, a.python, env={"SAYA_FAKE_GPU": json.dumps({"vendor": "amd", "backend": "rocm 6.4", "name": "sim 7900 XT", "vram_gib": 19.98})})
        check("G1 AMD 20 GB: recommended, applied by default", code == 0 and "RECOMMENDED" in out and sha(os.path.join(r, CORE[1])) == tested["files_patched"][CORE[1]], out[-600:])

        # J. simulated AMD 12 GB -> strongly recommended, applied by default
        r = copy_tree(a.clean, tmp, "J")
        code, out = run(["install", "--yes"], r, a.python, env={"SAYA_FAKE_GPU": json.dumps({"vendor": "amd", "backend": "rocm 6.4", "name": "sim 12 GB", "vram_gib": 11.98})})
        check("J1 AMD 12 GB: STRONGLY RECOMMENDED, applied by default", code == 0 and "STRONGLY RECOMMENDED" in out and sha(os.path.join(r, CORE[1])) == tested["files_patched"][CORE[1]], out[-600:])

        # K. simulated AMD 12 GB, the user answers "no" -> not applied
        r = copy_tree(a.clean, tmp, "K")
        code, out = run(["install"], r, a.python, env={"SAYA_FAKE_GPU": json.dumps({"vendor": "amd", "backend": "rocm 6.4", "name": "sim 12 GB", "vram_gib": 11.98})}, stdin="y\nn\n")
        check("K1 AMD 12 GB: user refuses, AMD patch not applied, install OK", code == 0 and "Skipped." in out and sha(os.path.join(r, CORE[1])) == upstream[CORE[1]], out[-600:])

        # H. dry-run changes nothing
        r = copy_tree(a.clean, tmp, "H")
        before = tree_hash(r)
        code, out = run(["install", "--dry-run"], r, a.python)
        check("H1 --dry-run writes nothing", code == 0 and tree_hash(r) == before, out[-300:])

        # I. incompatible (old) ComfyUI -> refused, nothing written
        if a.old:
            r = copy_tree(a.old, tmp, "I")
            before = tree_hash(r)
            code, out = run(["install", "--yes"], r, a.python)
            check("I1 incompatible ComfyUI -> UNSUPPORTED, nothing written", code != 0 and "UNSUPPORTED" in out and tree_hash(r) == before, out[-800:])

    bad = [n for n, ok, _ in results if not ok]
    print(f"\n{len(results) - len(bad)}/{len(results)} scenario checks passed")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()

"""Saya Couple installer: install / verify / restore / check-compat / amd.

Fail-closed: if a required check fails, nothing is modified.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys

from . import compat, gpu, patchlib

PKG = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
STATE_FILE = ".saya_install.json"
BACKUP_DIR = ".saya_backups"
NODE_DIR = "custom_nodes/Saya_Couple"
LEGACY_NODE_DIRS = ("custom_nodes/Saya_Couple_Upated",)
LEGACY_ATTENTION_PATCH = compat.LEGACY_ATTENTION_PATCH  # 1.x only: removed at upgrade, never applied by 2.0
AMD_PATCH = "patches/amd_vram_safety.patch"


# ---------------------------------------------------------------- helpers

def sha256_file(path: str) -> str | None:
    if not os.path.isfile(path):
        return None
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_json(path: str):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def write_json(path: str, data) -> None:
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=1, ensure_ascii=False)
    os.replace(tmp, path)


def say(msg: str = "") -> None:
    print(msg, flush=True)


def ask(question: str, default: bool, assume: bool | None) -> bool:
    if assume is not None:
        say(f"{question} [{'Y/n' if default else 'y/N'}] -> {'yes' if assume else 'no'} (from command line)")
        return assume
    try:
        answer = input(f"{question} [{'Y/n' if default else 'y/N'}] ").strip().lower()
    except EOFError:
        answer = ""
    if not answer:
        return default
    return answer in ("y", "yes", "o", "oui")


def manifest() -> dict:
    return load_json(os.path.join(PKG, "MANIFEST.json"))


def node_entries() -> list[dict]:
    return [e for e in manifest()["files"] if e["component"] == "saya_node"]


def find_comfyui(explicit: str | None) -> str:
    candidates = [explicit] if explicit else []
    here = os.getcwd()
    candidates += [here, os.path.dirname(here), os.path.join(here, "ComfyUI"), os.path.dirname(PKG), os.path.join(os.path.dirname(PKG), "ComfyUI")]
    for c in candidates:
        if c and is_comfyui(c):
            return os.path.abspath(c)
    sys.exit("ComfyUI not found. Run from your ComfyUI folder or pass --comfyui /path/to/ComfyUI")


def is_comfyui(path: str) -> bool:
    return all(os.path.exists(os.path.join(path, p)) for p in ("main.py", "comfy/ldm/modules/attention.py", "comfy/model_management.py", "custom_nodes"))


def comfy_identity(root: str) -> dict:
    ident = {"version": None, "commit": None, "git": False}
    vf = os.path.join(root, "comfyui_version.py")
    if os.path.isfile(vf):
        m = re.search(r'__version__\s*=\s*"([^"]+)"', open(vf, encoding="utf-8").read())
        ident["version"] = m.group(1) if m else None
    try:
        r = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, capture_output=True, text=True, timeout=10)
        if r.returncode == 0:
            ident["commit"], ident["git"] = r.stdout.strip(), True
    except (OSError, subprocess.SubprocessError):
        pass
    return ident


def patch_of(rel: str) -> patchlib.FilePatch:
    return compat.load_patch(os.path.join(PKG, rel))


def patch_state(root: str, rel: str) -> str:
    fp = patch_of(rel)
    text = compat.read(root, fp.path)
    return "missing" if text is None else patchlib.status(text, fp)


def apply_patch(root: str, rel: str) -> str:
    """Applies the patch in place. Returns the new status. Never writes a partially patched file."""
    fp = patch_of(rel)
    path = os.path.join(root, fp.path)
    text = compat.read(root, fp.path)
    st = patchlib.status(text, fp)
    if st == "applied":
        return st
    if st != "clean":
        raise RuntimeError(f"{fp.path}: patch does not apply ({st}); file left untouched")
    new = patchlib.apply_to_text(text, fp)
    with open(path + ".saya_tmp", "w", encoding="utf-8", newline="") as f:
        f.write(new)
    os.replace(path + ".saya_tmp", path)
    return patchlib.status(compat.read(root, fp.path), fp)


def print_axes(axes) -> None:
    for ax in axes:
        say(f"  [{ax.status}] {ax.name}")
        for ok, what in ax.checks:
            if not ok:
                say(f"      FAIL {what}")
        for n in ax.notes:
            say(f"      - {n}")


# ---------------------------------------------------------------- state / backup

def state_path(root: str) -> str:
    return os.path.join(root, STATE_FILE)


def load_state(root: str) -> dict | None:
    p = state_path(root)
    return load_json(p) if os.path.isfile(p) else None


def new_backup(root: str, ident: dict) -> tuple[str, dict]:
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    bdir = os.path.join(root, BACKUP_DIR, stamp)
    os.makedirs(os.path.join(bdir, "files"), exist_ok=False)
    state = {
        "saya_version": manifest()["saya_version"],
        "installed_at": datetime.datetime.now().isoformat(timespec="seconds"),
        "comfyui": ident,
        "backup": os.path.relpath(bdir, root),
        "core_files": {},          # rel -> {existed, sha256_before, sha256_after, component}
        "node_dir": {"existed_before": False, "previous_saved_as": None, "files": {}},
    }
    return bdir, state


def backup_file(root: str, state: dict, rel: str, component: str) -> None:
    if rel in state["core_files"]:
        return  # first backup wins: restore must bring back the state before the FIRST install
    src = os.path.join(root, rel)
    dst = os.path.join(root, state["backup"], "files", rel)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copy2(src, dst)
    if sha256_file(dst) != sha256_file(src):
        raise RuntimeError(f"backup of {rel} is not byte-identical")
    state["core_files"][rel] = {"existed": True, "sha256_before": sha256_file(src), "sha256_after": None, "component": component}


def save_state(root: str, state: dict) -> None:
    write_json(state_path(root), state)
    write_json(os.path.join(root, state["backup"], "backup.json"), state)


# ---------------------------------------------------------------- commands

def cmd_check_compat(args) -> int:
    root = find_comfyui(args.comfyui)
    ident = comfy_identity(root)
    say(f"ComfyUI: {root}\n  version {ident['version']}  commit {ident['commit'] or 'n/a (not a git checkout)'}")
    axes = compat.check_all(root, PKG)
    print_axes(axes)
    blocking = [a for a in axes[:2] if a.status == compat.UNSUPPORTED]
    say("\nVERDICT: " + ("UNSUPPORTED - Saya Couple cannot be installed on this ComfyUI" if blocking else f"installable ({axes[0].status} / {axes[1].status})"))
    return 1 if blocking else 0


def remove_legacy_patch(root: str, state: dict) -> str:
    """Upgrade from 1.x: reverse the 1.x core patch on attention.py (the 2.0 engine lives in the pack)."""
    fp = patch_of(LEGACY_ATTENTION_PATCH)
    rel = fp.path
    text = compat.read(root, rel)
    new = patchlib.apply_to_text(text, fp, reverse=True) if text is not None else None
    if new is None:
        raise RuntimeError(f"{rel}: the 1.x patch does not reverse cleanly; file left untouched (restore it from your backup or `saya restore`)")
    with open(os.path.join(root, rel), "w", encoding="utf-8", newline="") as f:
        f.write(new)
    rec = state["core_files"].setdefault(rel, {"existed": True, "sha256_before": None, "component": "saya_core"})
    rec["sha256_after"] = sha256_file(os.path.join(root, rel))
    rec["legacy_patch_removed_by"] = manifest()["saya_version"]
    save_state(root, state)
    st = patchlib.status(new, fp)
    if rec.get("sha256_before") and rec["sha256_after"] == rec["sha256_before"]:
        return "file byte-identical to the backup taken before the 1.x install"
    return f"patch state now: {st}"


def install_node(root: str, state: dict) -> dict:
    """Copies the custom node. Returns counts. Keeps the previous folder (if foreign) inside the backup.
    Files installed by a previous Saya version and gone from this one are removed (upgrade)."""
    dest_root = os.path.join(root, NODE_DIR)
    entries = node_entries()
    ours = state["node_dir"]["files"]
    wanted = {e["destination"] for e in entries}
    removed = 0
    for rel in [r for r in list(ours) if r not in wanted]:
        p = os.path.join(root, rel)
        if os.path.isfile(p):
            os.remove(p)
            removed += 1
        del ours[rel]
    for dirpath, dirs, files in os.walk(dest_root, topdown=False):
        if os.path.basename(dirpath) == "__pycache__":
            shutil.rmtree(dirpath, ignore_errors=True)
        elif os.path.isdir(dirpath) and not os.listdir(dirpath) and dirpath != dest_root:
            os.rmdir(dirpath)
    if os.path.isdir(dest_root) and not ours:
        state["node_dir"]["existed_before"] = True
        saved = os.path.join(root, state["backup"], "previous_" + os.path.basename(NODE_DIR))
        shutil.copytree(dest_root, saved)
        state["node_dir"]["previous_saved_as"] = os.path.relpath(saved, root)
        shutil.rmtree(dest_root)
    copied = kept = 0
    for e in entries:
        dst = os.path.join(root, e["destination"])
        if sha256_file(dst) == e["sha256"]:
            kept += 1
        else:
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            shutil.copy2(os.path.join(PKG, e["path"]), dst)
            copied += 1
        ours[e["destination"]] = e["sha256"]
    return {"copied": copied, "unchanged": kept, "removed": removed}


def cmd_install(args) -> int:
    root = find_comfyui(args.comfyui)
    ident = comfy_identity(root)
    py = gpu.comfy_python(root, args.python)
    say("=" * 64 + "\nSaya Couple installer\n" + "=" * 64)
    say(f"ComfyUI   : {root}\nversion   : {ident['version']}   commit: {ident['commit'] or 'n/a'}\npython    : {py}")

    say("\n[1/7] Compatibility (capability based)")
    axes = compat.check_all(root, PKG)
    print_axes(axes)
    att, pack, amd = axes
    if att.status == compat.UNSUPPORTED or pack.status == compat.UNSUPPORTED:
        say("\nUNSUPPORTED: nothing was modified. Fix the FAIL lines above (or use a supported ComfyUI).")
        return 1
    for legacy in LEGACY_NODE_DIRS:
        if os.path.isdir(os.path.join(root, legacy)):
            say(f"\n  WARNING: {legacy} exists. It registers the same node names as Saya_Couple;"
                "\n           disable or remove it after installing, or ComfyUI will load both.")

    say("\n[2/7] What will change")
    legacy = patch_state(root, LEGACY_ATTENTION_PATCH)
    say("  core modification          : NONE (2.0 runs on the stock core: the engine is injected with ModelPatcher object patches)")
    if legacy == "applied":
        say("  upgrade from 1.x           : the 1.x core patch is still on comfy/ldm/modules/attention.py -> it will be REMOVED (file back to upstream)")
    say(f"  custom node                : {NODE_DIR}/ ({len(node_entries())} files)")
    level, msg = gpu.amd_recommendation(gpu.detect(py))
    say(f"  OPTIONAL AMD patch          : comfy/model_management.py - {msg}")
    if legacy == "applied" or level in ("recommended", "strongly_recommended"):
        say("\n  !! The optional AMD patch (and the removal of a 1.x patch) modify ComfyUI's own files.")
        say("  !! BACK UP YOUR OWN COMFYUI INSTALLATION FIRST. The installer also keeps a backup,")
        say("  !! but your own copy is the one you can trust. Use at your own risk.")
    if args.dry_run:
        say("\n--dry-run: stopping before any change.")
        return 0
    if not ask("\nProceed with the installation?", False, True if args.yes else None):
        say("Cancelled, nothing modified.")
        return 1

    say("\n[3/7] Backup")
    state = load_state(root)
    if state is None:
        bdir, state = new_backup(root, ident)
    else:
        bdir = os.path.join(root, state["backup"])
        say(f"  existing Saya install found: reusing its original backup {state['backup']} (restore goes back to before the FIRST install)")
    if legacy == "applied":
        backup_file(root, state, "comfy/ldm/modules/attention.py", "saya_core")
    save_state(root, state)
    say(f"  backup folder: {os.path.relpath(bdir, root)}")

    say("\n[4/7] Core patch")
    if legacy == "applied":
        st = remove_legacy_patch(root, state)
        say(f"  1.x core patch removed from attention.py ({st})")
    else:
        say("  none needed (2.0): attention.py left untouched")

    say("\n[5/7] Custom node")
    counts = install_node(root, state)
    save_state(root, state)
    say(f"  {NODE_DIR}: {counts['copied']} copied, {counts['unchanged']} already up to date"
        + (f", {counts['removed']} obsolete file(s) of the previous version removed" if counts.get("removed") else ""))

    say("\n[6/7] AMD VRAM safety patch (optional)")
    run_amd(root, state, py, args.amd, args.yes)

    say("\n[7/7] Verification")
    ok = report(root, state, py, full=args.full)
    say("\nInstall " + ("COMPLETE. Restart ComfyUI." if ok else "finished WITH PROBLEMS (see above). `saya restore` puts back the backed-up files."))
    return 0 if ok else 1


def run_amd(root: str, state: dict, py: str, choice: str, yes: bool) -> None:
    info = gpu.detect(py)
    level, msg = gpu.amd_recommendation(info)
    st = patch_state(root, AMD_PATCH)
    say(f"  GPU: {info.get('name') or info['vendor']} ({info.get('backend') or 'unknown backend'}, {info.get('vram_gib', 0):.1f} GB)")
    say(f"  {msg}")
    if st == "applied":
        say("  AMD patch already active.")
        return
    if level == "not_applicable" and choice != "yes":
        say("  Not offered (not an AMD/ROCm GPU).")
        return
    if level == "optional" and choice != "yes":
        say("  Information only: not applied. Use `saya amd --yes` if you want it anyway.")
        return
    if st != "clean":
        say(f"  model_management.py does not match the code this patch expects ({st}): NOT applied (optional, Saya works without it).")
        return
    default = level in ("recommended", "strongly_recommended")
    assume = {"yes": True, "no": False}.get(choice)
    if assume is None and yes:
        assume = default
    if not ask("  Apply the AMD VRAM safety patch?", default, assume):
        say("  Skipped.")
        return
    rel = "comfy/model_management.py"
    backup_file(root, state, rel, "amd_optional")
    new = apply_patch(root, AMD_PATCH)
    state["core_files"][rel]["sha256_after"] = sha256_file(os.path.join(root, rel))
    save_state(root, state)
    say(f"  model_management.py: {new}. Recommended launch flags on AMD: --disable-dynamic-vram --reserve-vram 0.75")


def smoke(root: str, py: str) -> dict:
    script = os.path.join(PKG, "installer", "saya_installer", "smoke.py")
    env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
    try:
        r = subprocess.run([py, script, root], capture_output=True, text=True, timeout=600, env=env)
        return json.loads(r.stdout.strip().splitlines()[-1])
    except Exception as e:  # noqa: BLE001
        return {"errors": [f"smoke runner: {e}"]}


def _replaces_attention(folder: str) -> bool:
    rx = re.compile(r"(BasicTransformerBlock|CrossAttention)\.forward\s*=|attention\.BasicTransformerBlock\s*=")
    for dirpath, _, files in os.walk(folder):
        for f in files:
            if f.endswith(".py"):
                with open(os.path.join(dirpath, f), encoding="utf-8", errors="ignore") as fh:
                    if rx.search(fh.read()):
                        return True
    return False


def conflicts(root: str) -> list[str]:
    found = [f"{d} also installed (duplicate node names)" for d in LEGACY_NODE_DIRS if os.path.isdir(os.path.join(root, d))]
    cn = os.path.join(root, "custom_nodes")
    for d in sorted(os.listdir(cn)) if os.path.isdir(cn) else []:
        if d not in ("Saya_Couple", "__pycache__") and os.path.isdir(os.path.join(cn, d)) and _replaces_attention(os.path.join(cn, d)):
            found.append(f"custom_nodes/{d} replaces BasicTransformerBlock/CrossAttention.forward (Saya dual mode may be bypassed)")
    return found


def report(root: str, state: dict | None, py: str, full: bool = False) -> bool:
    ident = comfy_identity(root)
    axes = compat.check_all(root, PKG)
    legacy = patch_state(root, LEGACY_ATTENTION_PATCH)
    amd_state = patch_state(root, AMD_PATCH)
    ok = True
    lines = [f"Saya Couple verify  (package {manifest()['saya_version']}, {platform.system()} {platform.machine()})",
             f"ComfyUI       : {root}",
             f"version/commit: {ident['version']} / {ident['commit'] or 'n/a (not git)'}",
             f"compatibility : engine={axes[0].status} | pack={axes[1].status} | amd={axes[2].status}"]
    engine_ok = axes[0].status != compat.UNSUPPORTED
    ok &= engine_ok
    lines.append("core files    : " + ("stock attention.py (2.0 needs no core modification)" if legacy != "applied"
                                       else "1.x CORE PATCH STILL APPLIED on attention.py -> run `saya install` to remove it"))
    ok &= legacy != "applied"
    missing, changed = 0, 0
    for e in node_entries():
        h = sha256_file(os.path.join(root, e["destination"]))
        missing += h is None
        changed += h is not None and h != e["sha256"]
    node_word = "OK" if not missing and not changed else f"INCOMPLETE ({missing} missing, {changed} changed)"
    ok &= node_word == "OK"
    lines.append(f"custom node   : {node_word}  [{NODE_DIR}, {len(node_entries())} files]")
    lines.append("AMD patch     : " + {"applied": "ACTIVE", "clean": "NOT INSTALLED", "conflict": "INCOMPATIBLE (model_management.py differs)", "missing": "INCOMPATIBLE"}[amd_state])
    for rel in ("comfy/ldm/modules/attention.py", "comfy/model_management.py"):
        lines.append(f"sha256 {rel}: {sha256_file(os.path.join(root, rel))}")
    deps = []
    for dep, need in (("MultiMaskCouple", "required"), ("RES4LYF", "needed by the demo and full workflows"), ("ComfyUI-Impact-Pack", "full workflow detailers"), ("ComfyUI_UltimateSDUpscale", "full workflow USDU passes")):
        deps.append(f"{dep} {'present' if os.path.isdir(os.path.join(root, 'custom_nodes', dep)) else 'MISSING'} ({need})")
    lines.append("dependencies  : " + "; ".join(deps))
    ok &= os.path.isdir(os.path.join(root, "custom_nodes", "MultiMaskCouple"))
    cf = conflicts(root)
    lines.append("conflicts     : " + ("; ".join(cf) if cf else "none known"))
    s = smoke(root, py)
    smoke_ok = bool(s.get("core_import") and s.get("saya_symbols") and s.get("dual_path_live") and s.get("pack_import")) and not s.get("errors")
    ok &= all(os.path.isdir(os.path.join(root, "custom_nodes", d)) for d in ("MultiMaskCouple",))
    ok &= smoke_ok
    lines.append(f"quick tests   : {'PASS' if smoke_ok else 'FAIL'}  (core import {s.get('core_import')}, engine path live {s.get('dual_path_live')}, "
                 f"gain {s.get('gain')}, pack import {s.get('pack_import')} / {s.get('pack_nodes')} nodes)")
    for err in s.get("errors", []):
        lines.append(f"   error: {err}")
    if full:
        r = subprocess.run([py, os.path.join(root, NODE_DIR, "tests", "run_all.py")], cwd=os.path.join(root, NODE_DIR),
                           capture_output=True, text=True, env=dict(os.environ, PYTHONDONTWRITEBYTECODE="1", SAYA_COMFY_ROOT=root))
        summary = [l for l in r.stdout.splitlines() if l.startswith("passed=")]
        lines.append(f"full tests    : {summary[-1] if summary else 'no summary (see output)'}")
        ok &= r.returncode == 0
    lines.append(f"installed     : {'yes, backup ' + state['backup'] if state else 'no Saya install record'}")
    lines.append(f"RESULT        : {'OK' if ok else 'PROBLEMS FOUND'}")
    say("\n".join(lines))
    return ok


def cmd_verify(args) -> int:
    root = find_comfyui(args.comfyui)
    return 0 if report(root, load_state(root), gpu.comfy_python(root, args.python), full=args.full) else 1


def cmd_restore(args) -> int:
    root = find_comfyui(args.comfyui)
    state = load_state(root)
    if state is None:
        say("No Saya install record (.saya_install.json) in this ComfyUI: nothing to restore.")
        return 1
    bdir = os.path.join(root, state["backup"])
    say(f"Restoring from {state['backup']} (install of {state['installed_at']})")
    drift = []
    for rel, rec in state["core_files"].items():
        cur = sha256_file(os.path.join(root, rel))
        if rec.get("sha256_after") and cur != rec["sha256_after"]:
            drift.append(f"{rel} changed since install (a ComfyUI update or manual edit?)")
    for rel, h in state["node_dir"]["files"].items():
        cur = sha256_file(os.path.join(root, rel))
        if cur is not None and cur != h:
            drift.append(f"{rel} changed since install")
    if drift:
        say("WARNING - these files changed after the install and would be overwritten/removed:")
        for d in drift:
            say(f"  - {d}")
        if not ask("Overwrite them anyway?", False, True if args.force else None):
            say("Cancelled, nothing modified.")
            return 1
    elif not ask("Restore the original ComfyUI files now?", True, True if args.yes or args.force else None):
        return 1
    problems = []
    for rel, rec in state["core_files"].items():
        src = os.path.join(bdir, "files", rel)
        dst = os.path.join(root, rel)
        shutil.copy2(src, dst)
        if sha256_file(dst) != rec["sha256_before"]:
            problems.append(f"{rel}: restored file hash differs from the backup record")
        else:
            say(f"  restored {rel} (sha256 {rec['sha256_before'][:12]}...)")
    for rel in state["node_dir"]["files"]:
        p = os.path.join(root, rel)
        if os.path.isfile(p):
            os.remove(p)
    node_root = os.path.join(root, NODE_DIR)
    for dirpath, dirs, files in os.walk(node_root, topdown=False):
        for d in dirs:
            if d == "__pycache__":
                shutil.rmtree(os.path.join(dirpath, d), ignore_errors=True)
        if os.path.isdir(dirpath) and not os.listdir(dirpath):
            os.rmdir(dirpath)
    if os.path.isdir(node_root):
        say(f"  {NODE_DIR} kept: it contains files that were not installed by Saya")
    if state["node_dir"]["previous_saved_as"]:
        shutil.copytree(os.path.join(root, state["node_dir"]["previous_saved_as"]), node_root, dirs_exist_ok=True)
        say(f"  previous {NODE_DIR} put back")
    else:
        say(f"  removed {NODE_DIR}")
    if problems:
        for p in problems:
            say("  ERROR " + p)
        return 1
    os.replace(state_path(root), os.path.join(bdir, "restored_" + STATE_FILE.lstrip(".")))
    say("Restore COMPLETE: core files are byte-identical to the backup taken before installation. Restart ComfyUI.")
    return 0


def cmd_amd(args) -> int:
    root = find_comfyui(args.comfyui)
    py = gpu.comfy_python(root, args.python)
    if args.revert:
        state = load_state(root)
        rec = (state or {}).get("core_files", {}).get("comfy/model_management.py")
        if not rec:
            say("The AMD patch was not installed by Saya (no backup record); nothing reverted.")
            return 1
        cur = sha256_file(os.path.join(root, "comfy/model_management.py"))
        if cur != rec["sha256_after"] and not ask("model_management.py changed since the AMD patch. Overwrite?", False, True if args.force else None):
            return 1
        shutil.copy2(os.path.join(root, state["backup"], "files", "comfy/model_management.py"), os.path.join(root, "comfy/model_management.py"))
        del state["core_files"]["comfy/model_management.py"]
        save_state(root, state)
        say("AMD patch reverted (original model_management.py restored). Restart ComfyUI.")
        return 0
    info = gpu.detect(py)
    level, msg = gpu.amd_recommendation(info)
    say(f"GPU: {info.get('name') or info['vendor']} ({info.get('backend')}, {info.get('vram_gib', 0):.1f} GB)\n{msg}\npatch state: {patch_state(root, AMD_PATCH)}")
    if args.check:
        return 0
    state = load_state(root)
    if state is None:
        bdir, state = new_backup(root, comfy_identity(root))
    run_amd(root, state, py, "yes" if args.yes else ("no" if args.no else "ask"), False)
    return 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(prog="saya", description="Saya Couple installer (install / verify / restore / check-compat / amd)")
    sub = ap.add_subparsers(dest="cmd", required=True)

    def common(p):
        p.add_argument("--comfyui", help="ComfyUI root folder (default: auto-detect)")
        p.add_argument("--python", help="ComfyUI's Python (default: auto-detect .venv / venv / python_embeded)")
        return p

    p = common(sub.add_parser("install", help="install Saya Couple"))
    p.add_argument("--yes", action="store_true", help="no questions (AMD patch follows the recommendation)")
    p.add_argument("--amd", choices=("ask", "yes", "no"), default="ask")
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--full", action="store_true", help="also run the pack's full test suite")
    p = common(sub.add_parser("verify", help="short support report"))
    p.add_argument("--full", action="store_true")
    p = common(sub.add_parser("restore", help="put back the files saved at install"))
    p.add_argument("--yes", action="store_true")
    p.add_argument("--force", action="store_true", help="overwrite files changed since install without asking")
    common(sub.add_parser("check-compat", help="compatibility report, changes nothing"))
    p = common(sub.add_parser("amd", help="AMD VRAM patch only"))
    p.add_argument("--check", action="store_true")
    p.add_argument("--revert", action="store_true")
    p.add_argument("--yes", action="store_true")
    p.add_argument("--no", action="store_true")
    p.add_argument("--force", action="store_true")
    args = ap.parse_args(argv)
    return {"install": cmd_install, "verify": cmd_verify, "restore": cmd_restore, "check-compat": cmd_check_compat, "amd": cmd_amd}[args.cmd](args)

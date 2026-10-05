"""Optional AMD/ROCm VRAM safety patch for ComfyUI core.

Run it with the Python of your ComfyUI install:
    python custom_nodes/Saya_Couple_Upated/tools/core_patches/apply_amd_vram_patch.py
Options: --comfyui PATH, --yes (no question), --force (apply even if not needed), --revert, --check.

Policy (AMD GPU under ROCm):
  <= 16 GB  -> strongly recommended for the Saya Couple workflow (asks, default yes)
  16-24 GB  -> recommended with big models / heavy workflows (asks, default yes)
  >= 24 GB  -> optional, not needed for Saya (skipped, use --force to apply)
NVIDIA / CPU: not needed, nothing is changed.
"""

import argparse
import glob
import os
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PATCH = os.path.join(HERE, "amd_vram_safety.patch")
TARGET = os.path.join("comfy", "model_management.py")
BACKUP_SUFFIX = ".bak-saya-amd"
# Cards report a little under their nominal size (16 GB -> 15.9 GiB, 24 GB -> 23.98 GiB).
STRONG_UP_TO_GIB = 16.5
OPTIONAL_FROM_GIB = 23.0


def detect_gpu():
    """Returns (vendor, total_vram_gib) with vendor in {"amd", "nvidia", "none"}."""
    try:
        import torch
        if torch.cuda.is_available():
            total = torch.cuda.get_device_properties(0).total_memory / 1024**3
            return ("amd" if getattr(torch.version, "hip", None) else "nvidia"), total
    except ImportError:
        pass
    # No torch in this Python: read the amdgpu driver directly (Linux).
    sizes = []
    for f in glob.glob("/sys/class/drm/card*/device/mem_info_vram_total"):
        try:
            sizes.append(int(open(f).read()) / 1024**3)
        except OSError:
            pass
    return ("amd", max(sizes)) if sizes else ("none", 0.0)


def git_apply(root, *extra):
    return subprocess.run(["git", "apply", *extra, PATCH], cwd=root, capture_output=True, text=True)


def state(root):
    if git_apply(root, "--reverse", "--check").returncode == 0:
        return "applied"
    if git_apply(root, "--check").returncode == 0:
        return "clean"
    return "unknown"


def main():
    default_root = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--comfyui", default=default_root, help="ComfyUI root folder (default: %(default)s)")
    ap.add_argument("--yes", action="store_true", help="apply without asking")
    ap.add_argument("--force", action="store_true", help="apply even if the GPU does not need it")
    ap.add_argument("--revert", action="store_true", help="remove the patch")
    ap.add_argument("--check", action="store_true", help="only report, change nothing")
    a = ap.parse_args()

    root = os.path.abspath(a.comfyui)
    target = os.path.join(root, TARGET)
    if not os.path.isfile(target):
        sys.exit(f"ComfyUI not found: {target} does not exist (use --comfyui PATH).")
    if shutil.which("git") is None:
        sys.exit("git is required to apply the patch safely.")

    st = state(root)
    vendor, vram = detect_gpu()
    print(f"GPU: {vendor.upper()} {vram:.1f} GB | patch: {st}")

    if a.revert:
        if st != "applied":
            sys.exit("Patch is not applied, nothing to revert.")
        if not a.check:
            r = git_apply(root, "--reverse")
            sys.exit(r.stderr) if r.returncode else print("Patch removed. Restart ComfyUI.")
        return

    if st == "applied":
        print("Already applied, nothing to do.")
        return
    if st == "unknown":
        sys.exit("model_management.py does not match the expected ComfyUI version: patch not applied.\n"
                 "Your ComfyUI is either modified or too different from the tested version (upstream 41db8f4f).")

    if not a.force:
        if vendor != "amd":
            print("Not an AMD GPU: this patch is not needed.")
            return
        if vram >= OPTIONAL_FROM_GIB:
            print(f"AMD GPU with {vram:.0f} GB: optional, not needed for Saya Couple (use --force to apply anyway).")
            return
    if vendor == "amd" and vram < OPTIONAL_FROM_GIB:
        level = "STRONGLY RECOMMENDED for the Saya Couple workflow" if vram <= STRONG_UP_TO_GIB else "RECOMMENDED with big models / heavy workflows"
        print(f"AMD GPU with {vram:.0f} GB: this patch is {level}.")

    if a.check:
        print("Patch can be applied (--check: nothing changed).")
        return

    if not a.yes:
        print("\nUnder ROCm, when PyTorch fills the VRAM the desktop can freeze\n"
              "or crash, and switching models (SDXL -> HiDream + LoRA) can run out of memory.\n"
              "This patch caps ComfyUI's VRAM (keeps 1 GB for the desktop) and unloads models fully between loads.")
        answer = input("Apply the AMD VRAM safety patch to ComfyUI? [Y/n] ").strip().lower()
        if answer not in ("", "y", "yes", "o", "oui"):
            print("Not applied.")
            return

    shutil.copy2(target, target + BACKUP_SUFFIX)
    r = git_apply(root)
    if r.returncode:
        sys.exit(f"git apply failed, file untouched:\n{r.stderr}")
    print(f"Patch applied. Backup: {TARGET}{BACKUP_SUFFIX}. Restart ComfyUI; the log should show 'AMD hard VRAM cap'.")
    print("Recommended launch flags on AMD: --disable-dynamic-vram --reserve-vram 0.75")


if __name__ == "__main__":
    main()

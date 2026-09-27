"""GPU detection through ComfyUI's own Python (the one that has torch), with driver fallbacks."""

from __future__ import annotations

import glob
import json
import os
import shutil
import subprocess
import sys

PROBE = r"""
import json
out = {"torch": None, "vendor": "none", "backend": None, "name": None, "vram_gib": 0.0}
try:
    import torch
    out["torch"] = torch.__version__
    hip = getattr(torch.version, "hip", None)
    if torch.cuda.is_available():
        p = torch.cuda.get_device_properties(0)
        out["name"] = p.name
        out["vram_gib"] = p.total_memory / 1024**3
        if hip:
            out["vendor"], out["backend"] = "amd", "rocm " + str(hip)
        else:
            out["vendor"], out["backend"] = "nvidia", "cuda " + str(torch.version.cuda)
except Exception as e:
    out["error"] = str(e)
print(json.dumps(out))
"""


def comfy_python(root: str, override: str | None = None) -> str:
    if override:
        return override
    for rel in (".venv/bin/python", "venv/bin/python", ".venv/Scripts/python.exe", "venv/Scripts/python.exe",
                "../python_embeded/python.exe", "python_embeded/python.exe"):
        p = os.path.normpath(os.path.join(root, rel))
        if os.path.isfile(p):
            return p
    return sys.executable


def detect(python: str) -> dict:
    info = {"vendor": "none", "backend": None, "name": None, "vram_gib": 0.0, "source": "none"}
    fake = os.environ.get("SAYA_FAKE_GPU")  # test hook, e.g. '{"vendor":"nvidia","backend":"cuda 12","vram_gib":8}'
    if fake:
        info.update(json.loads(fake), source="SAYA_FAKE_GPU")
        return info
    try:
        r = subprocess.run([python, "-c", PROBE], capture_output=True, text=True, timeout=180, env=dict(os.environ, PYTHONDONTWRITEBYTECODE="1"))
        data = json.loads(r.stdout.strip().splitlines()[-1])
        info.update({k: data.get(k) for k in ("vendor", "backend", "name", "vram_gib", "torch")})
        info["source"] = f"torch ({python})"
        if info["vendor"] != "none":
            return info
    except Exception as e:  # noqa: BLE001 - detection must never crash the installer
        info["probe_error"] = str(e)
    # Fallbacks without torch: amdgpu sysfs (Linux) / nvidia-smi.
    sizes = []
    for f in glob.glob("/sys/class/drm/card*/device/mem_info_vram_total"):
        try:
            with open(f) as fh:
                sizes.append(int(fh.read()) / 1024**3)
        except OSError:
            pass
    if sizes:
        info.update(vendor="amd", backend="amdgpu driver (ROCm not confirmed)", vram_gib=max(sizes), source="sysfs")
    elif shutil.which("nvidia-smi"):
        info.update(vendor="nvidia", backend="nvidia driver", source="nvidia-smi")
    return info


# AMD VRAM patch policy (cards report a little under their nominal size).
STRONG_UP_TO_GIB = 16.5
OPTIONAL_FROM_GIB = 23.0


def amd_recommendation(info: dict) -> tuple[str, str]:
    """(level, message). level in: not_applicable, optional, recommended, strongly_recommended."""
    if info.get("vendor") != "amd":
        return "not_applicable", "Not an AMD GPU: the AMD VRAM patch is not needed."
    if "rocm" not in str(info.get("backend") or ""):
        return "optional", "AMD GPU found, but ROCm/PyTorch was not confirmed: the patch only acts under ROCm."
    v = info.get("vram_gib") or 0.0
    if v >= OPTIONAL_FROM_GIB:
        return "optional", f"AMD GPU with {v:.0f} GB: this patch is generally not needed (optional)."
    if v <= STRONG_UP_TO_GIB:
        return "strongly_recommended", f"AMD GPU with {v:.0f} GB: STRONGLY RECOMMENDED for heavy workflows like Saya's."
    return "recommended", f"AMD GPU with {v:.0f} GB: RECOMMENDED with big models / heavy workflows."

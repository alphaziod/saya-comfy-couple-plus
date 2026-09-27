"""Regenerates MANIFEST.json (every distributed file, destination, sha256, origin, license, component).

    python3 tools/build_manifest.py
"""

import hashlib
import json
import os

PKG = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SKIP_DIRS = {"__pycache__", ".git"}
SKIP_FILES = {"MANIFEST.json"}
UPSTREAM = "ComfyUI 41db8f4f (v0.34.0+77)"

GPL = "GPL-3.0-or-later"
GPL_COMFY = "GPL-3.0 (ComfyUI)"
AGPL = "AGPL-3.0-or-later"

TYPES = {".py": "python", ".js": "javascript", ".cjs": "javascript", ".json": "json", ".md": "markdown", ".txt": "text", ".patch": "patch", ".bat": "script"}


def classify(rel: str) -> dict:
    e = {"destination": None, "origin": "Saya Couple (original)", "license": GPL, "required": False, "component": "docs", "description": ""}
    if rel.startswith("files/custom_nodes/Saya_Couple/"):
        inner = rel[len("files/custom_nodes/Saya_Couple/"):]
        e.update(destination=rel[len("files/"):], component="saya_node", required=True, description="Saya Couple custom node pack")
        if inner.startswith("src/ppm_vendor/"):
            modified = inner.endswith(("attention_couple/common.py", "attention_couple/unet_couple.py"))
            e.update(origin="pamparamm/ComfyUI-ppm (vendored" + (", MODIFIED by Saya)" if modified else ")"), license=AGPL,
                     description="vendored Attention Couple / NegPip implementation")
        elif inner == "src/nodes/saya_attention_couple.py":
            e.update(origin="adapted from pamparamm/ComfyUI-ppm 'Attention Couple (PPM)' node", license=AGPL, description="Saya Attention Couple PPM node")
        elif inner.startswith("core_patch/"):
            e.update(origin="derived from ComfyUI (patch)", license=GPL_COMFY, description="copy of the core patch, used by the pack tests")
        elif inner.startswith("tests/"):
            e["description"] = "pack test suite (installed with the node)"
    elif rel == "files/comfy/ldm/modules/attention.py":
        e.update(destination="comfy/ldm/modules/attention.py", component="saya_core", required=True, origin="ComfyUI + Saya patch", license=GPL_COMFY,
                 description="REFERENCE: patched attention.py for the tested upstream. The installer applies the patch; it never copies this file.",
                 upstream_expected=UPSTREAM)
    elif rel == "files/comfy/model_management.py":
        e.update(destination="comfy/model_management.py", component="amd_optional", origin="ComfyUI + Saya AMD patch", license=GPL_COMFY,
                 description="REFERENCE: patched model_management.py for the tested upstream (optional AMD patch).", upstream_expected=UPSTREAM)
    elif rel == "patches/saya_dual_attention.patch":
        e.update(destination="comfy/ldm/modules/attention.py", component="saya_core", required=True, origin="derived from ComfyUI", license=GPL_COMFY,
                 description="REQUIRED core patch: MAIN/P1/P2 forced attn2 path, main_locked_delta, PERSON gain 0.78", upstream_expected=UPSTREAM)
    elif rel == "patches/amd_vram_safety.patch":
        e.update(destination="comfy/model_management.py", component="amd_optional", origin="derived from ComfyUI", license=GPL_COMFY,
                 description="OPTIONAL AMD/ROCm VRAM safety patch", upstream_expected=UPSTREAM)
    elif rel.startswith("patches/third_party/"):
        e.update(component="third_party_optional", origin="derived from ssitu/ComfyUI_UltimateSDUpscale", license="GPL-3.0",
                 description="OPTIONAL, not installed: per-tile Saya couple masks in Ultimate SD Upscale", upstream_expected="ComfyUI_UltimateSDUpscale bebd569")
    elif rel.startswith("workflows/"):
        e.update(component="workflow", description="demo workflow / demo prompt")
    elif rel.startswith("tests/"):
        e.update(component="test", description="public test and validation scripts")
    elif rel.startswith(("installer/", "tools/")) or rel in ("saya", "saya.bat", "compatibility.json"):
        e.update(component="installer", description="installer / release tooling")
    return e


def main() -> None:
    files = []
    for dirpath, dirs, names in os.walk(PKG):
        dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
        for n in sorted(names):
            rel = os.path.relpath(os.path.join(dirpath, n), PKG).replace(os.sep, "/")
            if rel in SKIP_FILES or n.endswith(".pyc"):
                continue
            with open(os.path.join(PKG, rel), "rb") as f:
                digest = hashlib.sha256(f.read()).hexdigest()
            ext = os.path.splitext(n)[1]
            entry = {"path": rel, "type": "script" if n == "saya" else TYPES.get(ext, ext.lstrip(".") or "file"), "sha256": digest}
            entry.update(classify(rel))
            files.append(entry)
    with open(os.path.join(PKG, "files/custom_nodes/Saya_Couple/VERSION.txt")) as f:
        version = f.read().strip()
    out = {"schema": 1, "saya_version": version, "tested_upstream": UPSTREAM, "files": files}
    with open(os.path.join(PKG, "MANIFEST.json"), "w", encoding="utf-8") as f:
        json.dump(out, f, indent=1, ensure_ascii=False)
    by = {}
    for e in files:
        by[e["component"]] = by.get(e["component"], 0) + 1
    print(f"MANIFEST.json: {len(files)} files {by}")


if __name__ == "__main__":
    main()

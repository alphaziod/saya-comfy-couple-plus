"""Checks, without importing ComfyUI, that a ComfyUI install has the infrastructure Saya Couple relies on.
Since M1 (2026-10-05) no ComfyUI core file is modified: the Couple engine is injected with ModelPatcher object patches.

    python custom_nodes/Saya_Couple_Upated/tools/check_comfyui_compat.py [--comfyui PATH]

No version number is trusted: every check reads the real source of the target ComfyUI.
REQUIRED failures -> exit code 1 (do not install). OPTIONAL failures are only reported.
"""

import argparse
import ast
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PACK = os.path.dirname(HERE)
PATCHES = os.path.join(HERE, "core_patches")
CORE_MODULE_ROOTS = ("comfy", "comfy_extras", "nodes", "server", "folder_paths", "node_helpers", "latent_preview", "execution")

# Runtime contracts of the core patch: keys and call shapes the forced attn2 path reads.
# (file, regex, why) — checked on the file BEFORE or AFTER the Saya patch, both contain them.
RUNTIME_ANCHORS = (
    ("comfy/samplers.py", r'transformer_options\["cond_or_uncond"\]\s*=\s*cond_or_uncond\[:\]', "cond/uncond rows passed to the model"),
    ("comfy/ldm/modules/attention.py", r'transformer_options\["activations_shape"\]\s*=\s*list\(x\.shape\)', "latent H/W passed to the transformer blocks"),
    ("comfy/ldm/modules/attention.py", r'optimized_attention\(q, k, v, self\.heads, attn_precision=self\.attn_precision, transformer_options=transformer_options\)', "attention backends accept transformer_options"),
    ("comfy/ldm/modules/attention.py", r'switch_temporal_ca_to_sa', "BasicTransformerBlock layout"),
    # M1 (2026-10-05): the engine is injected by object patches; the stock block forward must call its attn2 module this way.
    ("comfy/ldm/modules/attention.py", r'self\.attn2\(n, context=context_attn2, value=value_attn2, transformer_options=transformer_options\)', "BasicTransformerBlock.forward calls self.attn2(...) (Saya engine proxy entry point)"),
    ("comfy/model_patcher.py", r'def add_object_patch\(', "ModelPatcher.add_object_patch (engine injection)"),
    ("comfy/model_patcher.py", r'object_patches_backup', "ModelPatcher restores object patches (unpatch_model)"),
    ("comfy/patcher_extension.py", r'ON_DETACH\s*=', "CallbacksMP.ON_DETACH (restore when a clone is dropped)"),
    ("comfy/utils.py", r'def deepcopy_list_dict\(', "clone() keeps payload tensors by reference (engine state key)"),
    ("comfy/model_patcher.py", r'self\.wrappers\b[^=\n]*=', "ModelPatcher.wrappers (dual-mode safety checks)"),
    ("comfy/model_patcher.py", r'def get_model_object\(', "ModelPatcher.get_model_object"),
    ("comfy/model_patcher.py", r'self\.object_patches\b[^=\n]*=', "ModelPatcher.object_patches"),
    ("comfy/patcher_extension.py", r'DIFFUSION_MODEL\s*=', "WrappersMP.DIFFUSION_MODEL"),
    ("comfy/patcher_extension.py", r'SAMPLER_SAMPLE\s*=', "WrappersMP.SAMPLER_SAMPLE"),
)


def read(root, rel):
    p = os.path.join(root, rel)
    return open(p, encoding="utf-8", errors="replace").read() if os.path.isfile(p) else None


def module_file(root, dotted):
    base = dotted.replace(".", "/")
    for rel in (base + ".py", base + "/__init__.py"):
        if os.path.isfile(os.path.join(root, rel)):
            return rel
    # Namespace package (ComfyUI's comfy/ and comfy/ldm/ carry no __init__.py): importable, defines nothing itself.
    if os.path.isdir(os.path.join(root, base)):
        return base + "/"
    return None


def defined_names(src):
    """Top-level names of a module (defs, classes, assignments, re-exports). None = star import, cannot tell."""
    names = set()
    for node in ast.parse(src).body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            names.add(node.name)
        elif isinstance(node, ast.Assign):
            for t in node.targets:
                for n in ast.walk(t):
                    if isinstance(n, ast.Name):
                        names.add(n.id)
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            names.add(node.target.id)
        elif isinstance(node, (ast.Import, ast.ImportFrom)):
            for a in node.names:
                if a.name == "*":
                    return None
                names.add((a.asname or a.name).split(".")[0])
        elif isinstance(node, (ast.If, ast.Try)):
            for sub in ast.walk(node):
                if isinstance(sub, (ast.FunctionDef, ast.ClassDef)):
                    names.add(sub.name)
                elif isinstance(sub, ast.Assign):
                    names.update(n.id for t in sub.targets for n in ast.walk(t) if isinstance(n, ast.Name))
                elif isinstance(sub, (ast.Import, ast.ImportFrom)):
                    names.update((a.asname or a.name).split(".")[0] for a in sub.names)
    return names


def pack_requirements():
    """Raw core references of the pack: ("mod", module, where), ("from", module, name, where), ("attr", dotted, where)."""
    reqs = set()
    for dirpath, dirs, files in os.walk(PACK):
        dirs[:] = [d for d in dirs if d not in ("tests", "tools", "__pycache__", "web")]
        for f in files:
            if not f.endswith(".py"):
                continue
            path = os.path.join(dirpath, f)
            where = os.path.relpath(path, PACK)
            tree = ast.parse(open(path, encoding="utf-8").read())
            aliases = {}
            for node in ast.walk(tree):
                if isinstance(node, ast.Import):
                    for a in node.names:
                        if a.name.split(".")[0] in CORE_MODULE_ROOTS:
                            reqs.add(("mod", a.name, where))
                            if a.asname:
                                aliases[a.asname] = a.name
                            else:
                                aliases[a.name.split(".")[0]] = a.name.split(".")[0]
                elif isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
                    root = node.module.split(".")[0]
                    if root in CORE_MODULE_ROOTS:
                        for a in node.names:
                            reqs.add(("from", node.module, a.name, where))
                            aliases[a.asname or a.name] = f"{node.module}.{a.name}"
                    elif root == "custom_nodes":
                        reqs.add(("custom", node.module, where))
            for node in ast.walk(tree):
                if isinstance(node, ast.Attribute):
                    chain, cur = [node.attr], node.value
                    while isinstance(cur, ast.Attribute):
                        chain.append(cur.attr); cur = cur.value
                    if isinstance(cur, ast.Name) and cur.id in aliases:
                        reqs.add(("attr", ".".join([aliases[cur.id]] + chain[::-1]), where))
    return reqs


def check_patch(root, patch):
    def run(*extra):
        return subprocess.run(["git", "apply", *extra, patch], cwd=root, capture_output=True, text=True).returncode == 0
    if run("--reverse", "--check"):
        return True, "already applied"
    if run("--check"):
        return True, "applies cleanly"
    return False, "does not apply: the patched code differs in this ComfyUI version"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--comfyui", default=os.path.abspath(os.path.join(PACK, "..", "..")))
    ap.add_argument("--quiet", action="store_true", help="only print failures and the verdict")
    a = ap.parse_args()
    root = os.path.abspath(a.comfyui)
    required, optional = [], []

    def report(ok, level, what, detail):
        (required if level == "REQUIRED" else optional).append((ok, what, detail))
        if not a.quiet or not ok:
            print(f"{'PASS' if ok else 'FAIL'} [{level}] {what}: {detail}")

    for rel, rx, why in RUNTIME_ANCHORS:
        src = read(root, rel)
        report(src is not None and re.search(rx, src) is not None, "REQUIRED", why, rel)

    cache, seen = {}, set()

    def has_name(module, name):
        rel = module_file(root, module)
        if module_file(root, f"{module}.{name}"):
            return True
        if rel.endswith("/"):
            return False  # namespace package: only its submodules exist
        if rel not in cache:
            cache[rel] = defined_names(read(root, rel))
        return cache[rel] is None or name in cache[rel]

    def fail_once(key, what, detail):
        if key not in seen:
            seen.add(key)
            report(False, "REQUIRED", what, detail)

    for req in sorted(pack_requirements(), key=lambda r: (r[0], r[1])):
        kind, where = req[0], req[-1]
        if kind == "custom":
            name = req[1].split(".")[1]
            if name not in seen:
                seen.add(name)
                report(module_file(root, req[1]) is not None, "REQUIRED", f"custom node {name}", f"needed by {where}")
        elif kind == "mod":
            if module_file(root, req[1]) is None:
                fail_once(req[1], f"module {req[1]}", f"missing (used by {where})")
        elif kind == "from":
            module, name = req[1], req[2]
            if module_file(root, module) is None:
                fail_once(module, f"module {module}", f"missing (used by {where})")
            elif not has_name(module, name):
                fail_once((module, name), f"{module}.{name}", f"missing (used by {where})")
        else:
            parts = req[1].split(".")
            # longest prefix that is a real module; the next part must be defined in it. Deeper parts are
            # attributes of classes/objects and are covered by the runtime anchors and the patch checks.
            for i in range(len(parts) - 1, 0, -1):
                if module_file(root, ".".join(parts[:i])):
                    module, name = ".".join(parts[:i]), parts[i]
                    if not has_name(module, name):
                        fail_once((module, name), f"{module}.{name}", f"missing (used by {where})")
                    break

    ok, detail = check_patch(root, os.path.join(PATCHES, "amd_vram_safety.patch"))
    report(ok, "OPTIONAL", "AMD VRAM safety patch (comfy/model_management.py)", detail)

    # Packs needed at run time only (not at import): Detailers (Impact Pack + subpack) and USDU Couple (UltimateSDUpscale
    # with the per-tile crop patch). Their absence breaks those nodes, not the pack: OPTIONAL. Audit Fable 2026-10-05.
    def custom_dir(*stems):
        folder = os.path.join(root, "custom_nodes")
        names = os.listdir(folder) if os.path.isdir(folder) else []
        return next((os.path.join(folder, n) for n in names for stem in stems if n.lower() == stem.lower()), None)

    for what, stems, why in (("Impact Pack", ("ComfyUI-Impact-Pack", "comfyui-impact-pack"), "Detailers (SayaDuoSegsDetail)"),
                             ("Impact Subpack", ("comfyui-impact-subpack", "ComfyUI-Impact-Subpack"), "UltralyticsDetectorProvider in the workflow")):
        found = custom_dir(*stems)
        report(found is not None, "OPTIONAL", f"custom node {what}", f"needed by {why}" + ("" if found else " (missing)"))
    res4lyf = custom_dir("RES4LYF")
    if res4lyf is None:
        report(False, "OPTIONAL", "custom node RES4LYF", "needed by the workflow samplers / HiDream phase (missing)")
    else:
        ok, detail = check_patch(res4lyf, os.path.join(PATCHES, "res4lyf_hidream_attention_split.patch"))
        report(ok, "OPTIONAL", "RES4LYF HiDream masked-attention split patch (avoids ~5 GB on the HiDream mask)", detail)
    usdu = custom_dir("ComfyUI_UltimateSDUpscale")
    if usdu is None:
        report(False, "OPTIONAL", "custom node UltimateSDUpscale", "needed by SayaCoupleUSDUPass (missing)")
    else:
        ok, detail = check_patch(usdu, os.path.join(PATCHES, "ultimate_sd_upscale_saya_couple_crop.patch"))
        report(ok, "OPTIONAL", "UltimateSDUpscale couple-crop patch (per-tile masks in USDU Couple)", detail)

    bad = [r for r in required if not r[0]]
    print(f"\nREQUIRED {len(required) - len(bad)}/{len(required)} ok, OPTIONAL {sum(r[0] for r in optional)}/{len(optional)} ok")
    print("VERDICT: COMPATIBLE" if not bad else "VERDICT: NOT COMPATIBLE — do not install Saya Couple on this ComfyUI")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()

"""Capability-based compatibility checks. Nothing here imports ComfyUI: it only reads its source.

Three independent axes:
  A. Saya engine (2.0)     -> the stock core APIs the engine is injected through (ModelPatcher object patches) and
                              the runtime keys it reads; NO core file is modified any more
  B. Saya custom node pack -> every ComfyUI module/symbol the pack imports, and the MultiMaskCouple custom node
  C. AMD VRAM patch        -> comfy/model_management.py code the optional patch touches

Status per axis: OFFICIALLY TESTED / POTENTIALLY SUPPORTED / UNSUPPORTED. Age alone never blocks.
"""

from __future__ import annotations

import ast
import hashlib
import json
import os
import re
from dataclasses import dataclass, field

from . import patchlib

TESTED = "OFFICIALLY TESTED"
POTENTIAL = "POTENTIALLY SUPPORTED"
UNSUPPORTED = "UNSUPPORTED"

CORE_MODULE_ROOTS = ("comfy", "comfy_extras", "comfy_api", "nodes", "server", "folder_paths", "node_helpers", "latent_preview", "execution")

# Runtime contracts of the Saya engine (2.0: injected with ModelPatcher object patches, the core is left stock).
ENGINE_ANCHORS = (
    ("comfy/samplers.py", r'transformer_options\["cond_or_uncond"\]\s*=\s*cond_or_uncond\[:\]', "sampler passes cond/uncond rows"),
    ("comfy/samplers.py", r'transformer_options\["sigmas"\]\s*=\s*timestep', "sampler passes the step sigmas (dynamic ownership)"),
    ("comfy/ldm/modules/attention.py", r'transformer_options\["activations_shape"\]\s*=\s*list\(x\.shape\)', "latent H/W reach the transformer blocks"),
    ("comfy/ldm/modules/attention.py", r'optimized_attention\(q, k, v, self\.heads, attn_precision=self\.attn_precision, transformer_options=transformer_options\)', "attention backends accept transformer_options"),
    ("comfy/ldm/modules/attention.py", r'self\.attn2\(n, context=context_attn2, value=value_attn2, transformer_options=transformer_options\)', "BasicTransformerBlock.forward calls self.attn2(...) (engine entry point)"),
    ("comfy/model_patcher.py", r'def add_object_patch\(', "ModelPatcher.add_object_patch (engine injection)"),
    ("comfy/model_patcher.py", r'object_patches_backup', "ModelPatcher restores object patches on unpatch"),
    ("comfy/patcher_extension.py", r'ON_DETACH\s*=', "CallbacksMP.ON_DETACH (restore when a clone is dropped)"),
    ("comfy/utils.py", r'def deepcopy_list_dict\(', "clone() keeps payload tensors by reference"),
)
LEGACY_ATTENTION_PATCH = "patches/legacy/saya_dual_attention_1.x.patch"
# Object APIs the pack reads beyond plain imports.
PACK_ANCHORS = (
    ("comfy/model_patcher.py", r'self\.wrappers\b[^=\n]*=', "ModelPatcher.wrappers"),
    ("comfy/model_patcher.py", r'def get_model_object\(', "ModelPatcher.get_model_object"),
    ("comfy/model_patcher.py", r'self\.object_patches\b[^=\n]*=', "ModelPatcher.object_patches"),
    ("comfy/patcher_extension.py", r'DIFFUSION_MODEL\s*=', "WrappersMP.DIFFUSION_MODEL"),
    ("comfy/patcher_extension.py", r'SAMPLER_SAMPLE\s*=', "WrappersMP.SAMPLER_SAMPLE"),
)


@dataclass
class Axis:
    name: str
    status: str = UNSUPPORTED
    checks: list[tuple[bool, str]] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    def add(self, ok: bool, what: str):
        self.checks.append((bool(ok), what))

    @property
    def failures(self):
        return [w for ok, w in self.checks if not ok]


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def read(root: str, rel: str) -> str | None:
    p = os.path.join(root, rel)
    if not os.path.isfile(p):
        return None
    with open(p, encoding="utf-8", newline="") as f:
        return f.read()


def module_file(root: str, dotted: str) -> str | None:
    base = dotted.replace(".", "/")
    for rel in (base + ".py", base + "/__init__.py"):
        if os.path.isfile(os.path.join(root, rel)):
            return rel
    return base if os.path.isdir(os.path.join(root, base)) else None  # namespace package (comfy/ has no __init__)


def defined_names(src: str) -> set[str] | None:
    """Top-level names of a module. None = star import (cannot tell, treated as present)."""
    names: set[str] = set()
    for node in ast.parse(src).body:
        for sub in ([node] if not isinstance(node, (ast.If, ast.Try)) else list(ast.walk(node))):
            if isinstance(sub, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                names.add(sub.name)
            elif isinstance(sub, ast.Assign):
                names.update(n.id for t in sub.targets for n in ast.walk(t) if isinstance(n, ast.Name))
            elif isinstance(sub, ast.AnnAssign) and isinstance(sub.target, ast.Name):
                names.add(sub.target.id)
            elif isinstance(sub, (ast.Import, ast.ImportFrom)):
                for a in sub.names:
                    if a.name == "*":
                        return None
                    names.add((a.asname or a.name).split(".")[0])
    return names


def pack_references(pack_dir: str) -> set[tuple]:
    """("mod", m, where) / ("from", m, name, where) / ("attr", dotted, where) / ("custom", m, where)."""
    refs: set[tuple] = set()
    for dirpath, dirs, files in os.walk(pack_dir):
        dirs[:] = [d for d in dirs if d not in ("tests", "__pycache__", "web")]
        for f in files:
            if not f.endswith(".py"):
                continue
            path = os.path.join(dirpath, f)
            where = os.path.relpath(path, pack_dir).replace(os.sep, "/")
            with open(path, encoding="utf-8") as fh:
                tree = ast.parse(fh.read())
            aliases: dict[str, str] = {}
            for node in ast.walk(tree):
                if isinstance(node, ast.Import):
                    for a in node.names:
                        if a.name.split(".")[0] in CORE_MODULE_ROOTS:
                            refs.add(("mod", a.name, where))
                            aliases[a.asname or a.name.split(".")[0]] = a.name if a.asname else a.name.split(".")[0]
                elif isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
                    root = node.module.split(".")[0]
                    if root in CORE_MODULE_ROOTS:
                        for a in node.names:
                            refs.add(("from", node.module, a.name, where))
                            aliases[a.asname or a.name] = f"{node.module}.{a.name}"
                    elif root == "custom_nodes":
                        refs.add(("custom", node.module, where))
            for node in ast.walk(tree):
                if isinstance(node, ast.Attribute):
                    chain, cur = [node.attr], node.value
                    while isinstance(cur, ast.Attribute):
                        chain.append(cur.attr)
                        cur = cur.value
                    if isinstance(cur, ast.Name) and cur.id in aliases:
                        refs.add(("attr", ".".join([aliases[cur.id]] + chain[::-1]), where))
    return refs


def load_patch(path: str) -> patchlib.FilePatch:
    with open(path, encoding="utf-8") as f:
        fps = patchlib.parse(f.read())
    if len(fps) != 1:
        raise RuntimeError(f"{path}: expected one file in the patch, found {len(fps)}")
    return fps[0]


def original_text(root: str, fp: patchlib.FilePatch) -> tuple[str | None, str]:
    """(text before Saya's patch, patch status). If the patch is applied, the original is rebuilt by reversing it."""
    text = read(root, fp.path)
    if text is None:
        return None, "missing"
    st = patchlib.status(text, fp)
    if st == "applied":
        return patchlib.apply_to_text(text, fp, reverse=True), st
    return text, st


def amd_patch_for(root: str, pkg: str) -> patchlib.FilePatch:
    """Use the reserve-only upgrade only when it produces the complete known AMD patch."""
    full = load_patch(os.path.join(pkg, "patches", "amd_vram_safety.patch"))
    text = read(root, full.path)
    if text is not None and patchlib.status(text, full) == "conflict":
        upgrade = load_patch(os.path.join(pkg, "patches", "amd_desktop_reserve.patch"))
        updated = patchlib.apply_to_text(text, upgrade)
        if updated is not None and patchlib.status(updated, full) == "applied":
            return upgrade
    return full


def legacy_patch_state(root: str, pkg: str) -> str:
    """State of the 1.x core patch on attention.py: 'applied' means a 1.x install is still there (the 2.0 installer removes it)."""
    fp = load_patch(os.path.join(pkg, LEGACY_ATTENTION_PATCH))
    text = read(root, fp.path)
    return "missing" if text is None else patchlib.status(text, fp)


def check_engine(root: str, pkg: str, tested: dict) -> Axis:
    ax = Axis("Saya engine (object patches, no core modification)")
    for rel, rx, why in ENGINE_ANCHORS:
        src = read(root, rel)
        ax.add(src is not None and re.search(rx, src) is not None, f"{why} ({rel})")
    legacy = legacy_patch_state(root, pkg)
    ax.notes.append("legacy 1.x core patch: " + ("APPLIED (the installer will remove it)" if legacy == "applied" else "absent"))
    ax.status = UNSUPPORTED if ax.failures else (TESTED if is_tested_core(root, tested) else POTENTIAL)
    return ax


check_attention = check_engine  # name kept for tools written against 1.x


def check_pack(root: str, pkg: str, tested: dict) -> Axis:
    ax = Axis("Saya custom node pack")
    pack_dir = os.path.join(pkg, "files", "custom_nodes", "Saya_Couple")
    cache: dict[str, set | None] = {}
    fails: dict[tuple, str] = {}
    checked = 0

    def has(module: str, name: str) -> bool:
        if module_file(root, f"{module}.{name}"):
            return True
        rel = module_file(root, module)
        if rel is None or os.path.isdir(os.path.join(root, rel)):
            return False
        if rel not in cache:
            cache[rel] = defined_names(read(root, rel) or "")
        return cache[rel] is None or name in cache[rel]

    customs = set()
    for ref in sorted(pack_references(pack_dir), key=lambda r: (r[0], r[1])):
        kind, where = ref[0], ref[-1]
        if kind == "custom":
            customs.add(ref[1].split(".")[1])
            continue
        checked += 1
        if kind == "mod":
            if module_file(root, ref[1]) is None:
                fails.setdefault((ref[1],), f"module {ref[1]} missing (used by {where})")
        elif kind == "from":
            if module_file(root, ref[1]) is None:
                fails.setdefault((ref[1],), f"module {ref[1]} missing (used by {where})")
            elif not has(ref[1], ref[2]):
                fails.setdefault((ref[1], ref[2]), f"{ref[1]}.{ref[2]} missing (used by {where})")
        else:
            parts = ref[1].split(".")
            for i in range(len(parts) - 1, 0, -1):
                mod = ".".join(parts[:i])
                if module_file(root, mod):
                    if not has(mod, parts[i]):
                        fails.setdefault((mod, parts[i]), f"{mod}.{parts[i]} missing (used by {where})")
                    break
    ax.add(not fails, f"{checked} ComfyUI imports/symbols used by the pack")
    for msg in fails.values():
        ax.add(False, msg)
    for rel, rx, why in PACK_ANCHORS:
        src = read(root, rel)
        ax.add(src is not None and re.search(rx, src) is not None, f"{why} ({rel})")
    for name in sorted(customs):
        present = os.path.isdir(os.path.join(root, "custom_nodes", name))
        ax.add(present, f"custom node '{name}' installed (required dependency)")
    ax.status = UNSUPPORTED if ax.failures else (TESTED if is_tested_core(root, tested) else POTENTIAL)
    return ax


def is_tested_core(root: str, tested: dict) -> bool:
    """OFFICIALLY TESTED = every key core file is byte-identical to the tested ComfyUI (optional AMD patch applied or
    not; a still-applied legacy 1.x patch counts too, the installer removes it)."""
    patched = dict(tested.get("files_patched", {}), **tested.get("files_patched_legacy", {}))
    for rel, want in tested["key_files"].items():
        text = read(root, rel)
        if text is None:
            return False
        h = sha256_text(text)
        if h != want and h != patched.get(rel):
            return False
    return True


def check_amd(root: str, pkg: str, tested: dict) -> Axis:
    ax = Axis("AMD VRAM patch (optional)")
    fp = amd_patch_for(root, pkg)
    orig, st = original_text(root, fp)
    ax.add(st in ("clean", "applied"), f"amd_vram_safety.patch on {fp.path}: {st}")
    if orig is not None:
        for sym in ("def is_amd(", "def get_torch_device(", "def model_unload(self, memory_to_free=None", "def partially_unload(",
                    "def get_free_memory(", "def load_models_gpu(", "EXTRA_RESERVED_VRAM"):
            ax.add(sym in orig or sym in (read(root, "comfy/model_patcher.py") or ""), f"model management uses '{sym.strip('(')}'")
    ax.notes.append(f"patch state: {st}")
    ax.status = UNSUPPORTED if ax.failures else (TESTED if is_tested_core(root, tested) else POTENTIAL)
    return ax


def load_tested(pkg: str) -> dict:
    with open(os.path.join(pkg, "compatibility.json"), encoding="utf-8") as f:
        data = json.load(f)
    return data["officially_tested"][0]


def check_all(root: str, pkg: str) -> list[Axis]:
    tested = load_tested(pkg)
    return [check_engine(root, pkg, tested), check_pack(root, pkg, tested), check_amd(root, pkg, tested)]

"""Scans every file of the package (text + PNG metadata) for personal data, secrets and local paths.

    python3 tools/scan_personal_data.py [--private ~/private_patterns.txt]   -> findings, then CLEAN / NOT CLEAN

--private: one name per line (your model / LoRA file names, hostname, old workflow names...). Keep that file
OUTSIDE the repository, otherwise the scanner would publish the very names it looks for.
Public-facing files (README, docs, workflows) are also checked for explicit wording.
"""

import argparse
import json
import os
import re
import sys

PKG = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BLOCK = {
    "local path": re.compile(r"/home/[a-z_][a-z0-9_-]*|/Users/[A-Za-z]|[A-Za-z]:\\\\Users\\\\|/run/media/|/tmp/claude"),
    "e-mail": re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[a-z]{2,}"),
    "token / key": re.compile(r"\b(hf_[A-Za-z0-9]{20,}|sk-[A-Za-z0-9]{20,}|ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16})\b|BEGIN [A-Z ]*PRIVATE KEY"),
    "assistant scratch": re.compile(r"\.claude/|scratchpad|jobs/98f856f8"),
}
ALLOWED_EMAILS = {"noreply@anthropic.com"}  # none expected; placeholder for allow-listing
SKIP = {"LICENSE", "LICENSES/GPL-3.0.txt", "LICENSES/AGPL-3.0.txt", "tools/scan_personal_data.py"}


def texts(path):
    if path.endswith(".png"):
        from PIL import Image
        info = Image.open(path).info
        yield from (f"{k}: {v}" for k, v in info.items() if isinstance(v, str))
        return
    with open(path, encoding="utf-8", errors="replace") as f:
        yield f.read()


PUBLIC_FACING = ("README.md", "docs/", "workflows/", "THIRD_PARTY_NOTICES.md", "tests/themes_v1.json")
EXPLICIT = re.compile(r"(?i)\b(nsfw|nude|naked|sex|sexual|penis|vulva|pussy|anus|nipples?|futa\w*|loli|shota|hentai|bdsm|underwear|panties|lingerie)\b")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--private", help="file with private names to look for (kept outside the repo)")
    a = ap.parse_args()
    private = []
    if a.private:
        private = [l.strip() for l in open(os.path.expanduser(a.private), encoding="utf-8") if l.strip()]
    findings = []
    for dirpath, dirs, files in os.walk(PKG):
        dirs[:] = [d for d in dirs if d not in (".git", "__pycache__")]
        for n in files:
            rel = os.path.relpath(os.path.join(dirpath, n), PKG).replace(os.sep, "/")
            if rel in SKIP or n.endswith((".pyc", ".jpg")):
                continue
            for text in texts(os.path.join(PKG, rel)):
                for kind, rx in BLOCK.items():
                    for m in rx.finditer(text):
                        hit = m.group(0)
                        if kind == "e-mail" and (hit in ALLOWED_EMAILS or hit.endswith(("@local", "@example.com"))):
                            continue
                        line = text.count("\n", 0, m.start()) + 1
                        findings.append(f"{kind:18} {rel}:{line}: {hit}")
                low = text.lower()
                for name in private:
                    i = low.find(name.lower())
                    if i >= 0:
                        findings.append(f"{'private name':18} {rel}:{text.count(chr(10), 0, i) + 1}: {name}")
                if rel.startswith(PUBLIC_FACING) and rel != "docs/RELEASE_CANDIDATE_REPORT.md":
                    for m in EXPLICIT.finditer(text):
                        findings.append(f"{'explicit wording':18} {rel}:{text.count(chr(10), 0, m.start()) + 1}: {m.group(0)}")
    for f in findings:
        print(f)
    print("CLEAN" if not findings else f"NOT CLEAN ({len(findings)} findings)")
    sys.exit(1 if findings else 0)


if __name__ == "__main__":
    main()

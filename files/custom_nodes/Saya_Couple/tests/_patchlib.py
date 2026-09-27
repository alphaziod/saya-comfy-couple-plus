"""Minimal, fail-closed unified-diff applier (no git needed).

A hunk applies only if its full "before" block (context + removed lines) is found verbatim in the file.
Line offsets are tolerated (the code may have moved), content changes are not. Nothing is written
unless every hunk of every file applies.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

HUNK_RE = re.compile(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")


@dataclass
class Hunk:
    old_start: int
    before: list[str] = field(default_factory=list)
    after: list[str] = field(default_factory=list)


@dataclass
class FilePatch:
    path: str
    hunks: list[Hunk] = field(default_factory=list)


def parse(text: str) -> list[FilePatch]:
    files: list[FilePatch] = []
    cur: FilePatch | None = None
    hunk: Hunk | None = None
    for line in text.splitlines(keepends=True):
        if line.startswith("diff --git "):
            cur, hunk = None, None
            continue
        if line.startswith("+++ "):
            path = line[4:].strip()
            path = path[2:] if path.startswith("b/") else path
            cur = FilePatch(path)
            files.append(cur)
            hunk = None
            continue
        if line.startswith("--- ") or cur is None:
            continue
        m = HUNK_RE.match(line)
        if m:
            hunk = Hunk(int(m.group(1)))
            cur.hunks.append(hunk)
            continue
        if hunk is None:
            continue
        if line.startswith("\\"):  # "\ No newline at end of file"
            for block in (hunk.before, hunk.after):
                if block and block[-1].endswith("\n"):
                    block[-1] = block[-1][:-1]
            continue
        tag, body = line[:1], line[1:]
        if tag == " ":
            hunk.before.append(body); hunk.after.append(body)
        elif tag == "-":
            hunk.before.append(body)
        elif tag == "+":
            hunk.after.append(body)
    return files


def _find(lines: list[str], block: list[str], expected: int, start: int) -> int:
    """Index of block in lines at or after start, nearest to expected. -1 if absent."""
    n = len(block)
    hits = [i for i in range(start, len(lines) - n + 1) if lines[i:i + n] == block]
    if not hits:
        return -1
    return min(hits, key=lambda i: abs(i - expected))


def apply_to_text(text: str, fp: FilePatch, reverse: bool = False) -> str | None:
    """Patched text, or None if any hunk does not apply."""
    lines = text.splitlines(keepends=True)
    out: list[str] = []
    pos = 0
    shift = 0
    for h in fp.hunks:
        before, after = (h.after, h.before) if reverse else (h.before, h.after)
        expected = max(h.old_start - 1 + shift, 0)
        i = _find(lines, before, expected, pos)
        if i < 0:
            return None
        out.extend(lines[pos:i])
        out.extend(after)
        pos = i + len(before)
        shift += len(after) - len(before)
    out.extend(lines[pos:])
    return "".join(out)


def status(text: str, fp: FilePatch) -> str:
    """'applied', 'clean' (can be applied) or 'conflict'."""
    forward = apply_to_text(text, fp) is not None
    backward = apply_to_text(text, fp, reverse=True) is not None
    if backward and not forward:
        return "applied"
    if forward:
        return "clean"
    return "conflict"

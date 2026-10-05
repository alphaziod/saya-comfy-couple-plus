"""Saya Background Picker: one background (MAIN = scenery only) drawn from the categorized stock.

Category ``all`` draws from the whole stock, ``free`` injects nothing (or the ``custom`` text).
The draw is a pure function of (category, seed): ``fixed`` seed = locked background, ``randomize`` =
a new one each run, exactly like a sampler seed. The last 5 backgrounds actually generated are kept
on disk with a cursor: ``history`` = back / forward moves through them (one step per run), stay keeps the current one.
"""

from __future__ import annotations

import json
import os
import random
from pathlib import Path
from typing import Any


CATEGORIES = ["horror", "cozy", "dream", "feerique", "strange", "nature", "house", "city", "night", "water",
              "beach", "winter", "desert", "ruins", "temple", "castle", "space", "tech", "industrial", "luxury",
              "forest", "mountain", "cave", "garden", "sky", "rain", "autumn", "tropical", "underground", "gothic",
              "cyberpunk", "steampunk", "apocalypse", "festival", "market", "cafe", "library", "bedroom", "bathroom", "train", "horror++"]
ALL, FREE = "all", "free"
HISTORY_SIZE = 5

DATA_DIR = Path(__file__).resolve().parents[2] / "data"
#: Optional working copies of the stock (maintainer only): SAYA_BG_WORK_DIR=<dir holding backgrounds_3000.json / backgrounds_fou_1000.json>.
_WORK = Path(os.environ["SAYA_BG_WORK_DIR"]) if os.environ.get("SAYA_BG_WORK_DIR") else None
#: Stock = every file that exists among these, merged (ids differ: BG-xxx main stock, BF-xxxx improbable places).
#: The pack's own copies win over the working copies of the same stock.
STOCK_PATHS = [(DATA_DIR / "backgrounds.json", _WORK / "backgrounds_3000.json" if _WORK else None),
               (DATA_DIR / "backgrounds_fou.json", _WORK / "backgrounds_fou_1000.json" if _WORK else None)]
HISTORY_PATH = DATA_DIR / "background_history.json"

_stock_cache: dict[str, Any] = {}


class SayaBackgroundError(ValueError):
    pass


def load_stock() -> list[dict[str, str]]:
    paths = [p for choices in STOCK_PATHS if (p := next((c for c in choices if c.is_file()), None))]
    if not paths:
        raise SayaBackgroundError("background stock not found: " + " / ".join(str(c) for choices in STOCK_PATHS for c in choices))
    stamp = tuple((str(p), p.stat().st_mtime_ns) for p in paths)
    if _stock_cache.get("stamp") != stamp:
        items = []
        for path in paths:
            part = json.loads(path.read_text(encoding="utf-8")).get("backgrounds", [])
            bad = sorted({b.get("cat") for b in part} - set(CATEGORIES))
            if bad:
                raise SayaBackgroundError(f"{path.name}: categories outside the list {bad}")
            items += part
        ids = [b["id"] for b in items]
        if len(set(ids)) != len(ids):
            raise SayaBackgroundError("background stock: duplicate ids across files")
        _stock_cache.update(stamp=stamp, items=items)
    return _stock_cache["items"]


def pick(category: str, seed: int) -> dict[str, str]:
    stock = load_stock()
    pool = stock if category == ALL else [b for b in stock if b["cat"] == category]
    if not pool:
        raise SayaBackgroundError(f"no background in category {category!r}")
    return random.Random(seed).choice(pool)


def ultra_text(entry: dict[str, str]) -> str:
    """Version poussée à fond, écrite pour chaque décor (champ « ultra ») ; jamais générée à la volée."""
    if not entry.get("ultra"):
        raise SayaBackgroundError(f"{entry['id']}: no written ultra version (field 'ultra' missing)")
    return entry["ultra"]


NEW, BACK, FORWARD, STAY = "new", "back", "forward", "stay"


def read_state() -> dict[str, Any]:
    """{"entries": [newest first], "cursor": index of the background shown (0 = newest)}."""
    try:
        state = json.loads(HISTORY_PATH.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        state = {}
    if isinstance(state, list):  # ancien format : liste seule
        state = {"entries": state}
    entries = state.get("entries") if isinstance(state.get("entries"), list) else []
    cursor = state.get("cursor", 0)
    cursor = min(max(cursor if isinstance(cursor, int) else 0, 0), max(len(entries) - 1, 0))
    return {"entries": entries, "cursor": cursor}


def read_history() -> list[dict[str, Any]]:
    return read_state()["entries"]


def write_state(state: dict[str, Any]) -> None:
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    tmp = HISTORY_PATH.with_suffix(".tmp")
    tmp.write_text(json.dumps(state, ensure_ascii=False, indent=1), encoding="utf-8")
    os.replace(tmp, HISTORY_PATH)


def push_history(entry: dict[str, Any]) -> None:
    state = read_state()
    entries = state["entries"]
    if not (entries and entries[0].get("id") == entry["id"] and entries[0].get("seed") == entry["seed"]
            and entries[0].get("ultra") == entry.get("ultra")):  # même tirage relancé : pas une nouvelle génération
        entries = [entry, *entries][:HISTORY_SIZE]
    write_state({"entries": entries, "cursor": 0})


def short_name(entry: dict[str, Any]) -> str:
    return entry.get("short") or entry.get("name", "")


class SayaBackgroundPicker:
    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        return {
            "required": {
                "category": ([ALL, FREE, *CATEGORIES], {"default": ALL,
                             "tooltip": "all = whole stock, free = inject nothing (or the custom text)."}),
                "seed": ("INT", {"default": 0, "min": 0, "max": 0xffffffffffffffff, "control_after_generate": True,
                                 "tooltip": "fixed = locked background, randomize = a new one each run."}),
                "history": ([NEW, BACK, FORWARD, STAY], {"default": NEW,
                            "tooltip": "new = draw from category + seed. back / forward = one step older / newer in the "
                                       "last 5 generations at each run. stay = keep the background currently shown."}),
                "ultra_detailed": ("BOOLEAN", {"default": False,
                                               "tooltip": "Version poussée à fond, écrite pour chaque décor (même lieu)."}),
            },
            "optional": {
                "custom": ("STRING", {"default": "", "multiline": True, "tooltip": "free: this text is used as the background."}),
            },
        }

    RETURN_TYPES = ("STRING", "STRING", "STRING")
    RETURN_NAMES = ("background", "short_name", "info")
    FUNCTION = "choose"
    CATEGORY = "saya/couple"
    DESCRIPTION = "Background (MAIN = scenery only) from the categorized stock, locked or random like a seed, back / forward through the last 5 generations."

    @classmethod
    def IS_CHANGED(cls, category, seed, history=NEW, ultra_detailed=False, custom=""):
        if history in (BACK, FORWARD):
            return float("nan")  # moves the cursor: must run every time
        state = json.dumps(read_state()) if history == STAY else ""
        return f"{category}|{seed}|{history}|{ultra_detailed}|{custom}|{state}"

    def choose(self, category: str, seed: int, history: str = NEW, ultra_detailed: bool = False,
               custom: str = "") -> tuple[str, str, str]:
        if history != NEW:
            state = read_state()
            entries = state["entries"]
            if not entries:
                raise SayaBackgroundError(f"history {history}: no background generated yet")
            cursor = state["cursor"] + {BACK: 1, FORWARD: -1, STAY: 0}[history]
            cursor = min(max(cursor, 0), len(entries) - 1)
            write_state({"entries": entries, "cursor": cursor})
            entry = entries[cursor]
            ultra = ", ultra" if entry.get("ultra") else ""
            where = "dernier" if cursor == 0 else f"{cursor} génération(s) avant"
            return (entry["bg"], short_name(entry),
                    f"{history} -> {where} ({cursor + 1}/{len(entries)}): {entry['id']} [{entry['cat']}] {entry['name']} (seed {entry['seed']}{ultra})")
        if category == FREE:
            return custom.strip(), "free", "free: custom text" if custom.strip() else "free: no background injected"
        chosen = pick(category, seed)
        text = ultra_text(chosen) if ultra_detailed else chosen["bg"]
        # L'historique garde le texte réellement envoyé : back / forward redonne exactement la même version.
        push_history({"id": chosen["id"], "cat": chosen["cat"], "name": chosen["name"], "short": short_name(chosen),
                      "bg": text, "category": category, "seed": seed, "ultra": bool(ultra_detailed)})
        ultra = ", ultra" if ultra_detailed else ""
        return text, short_name(chosen), f"{chosen['id']} [{chosen['cat']}] {chosen['name']} (seed {seed}, {category}{ultra})"

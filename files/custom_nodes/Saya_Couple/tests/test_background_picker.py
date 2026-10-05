"""SayaBackgroundPicker: categories, seed lock / random, free, ultra, short name, back / forward history."""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

from harness import Check, load_pack


def test_background_picker():
    load_pack()
    from saya_couple.src.nodes import background_picker as bp

    c = Check("background_picker")
    saved = bp.STOCK_PATHS, bp.HISTORY_PATH, bp.DATA_DIR
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        stock = [{"id": f"BG-{k:03d}", "cat": bp.CATEGORIES[k % 3], "name": f"lieu numéro {k}",
                  "bg": f"indoors, place {k}, depth of field, rich detailed background",
                  "ultra": f"indoors, place {k}, many details, depth of field, rich detailed background"} for k in range(1, 31)]
        stock[0]["short"] = "lieu court"
        (tmp / "backgrounds.json").write_text(json.dumps({"backgrounds": stock}))
        bp.STOCK_PATHS, bp.HISTORY_PATH, bp.DATA_DIR = [(tmp / "backgrounds.json",), (tmp / "absent.json",)], tmp / "history.json", tmp
        try:
            node = bp.SayaBackgroundPicker()
            c.eq(len(node.INPUT_TYPES()["required"]["category"][0]), 43, "43 choices: all, free + 41 categories (horror++ since 2026-10-05)")
            c.eq(node.RETURN_NAMES, ("background", "short_name", "info"), "outputs")
            c.eq(node.choose("horror", 7)[0], node.choose("horror", 7)[0], "same category + seed = same background (lock)")
            c.ok(len({node.choose("all", s)[0] for s in range(40)}) > 10, "different seeds = different backgrounds (random)")
            cat = bp.CATEGORIES[1]
            c.ok(all(bp.pick(cat, s)["cat"] == cat for s in range(30)), "category respected")
            c.eq(node.choose("free", 3, custom="  my own scenery  ")[0], "my own scenery", "free: custom text")
            c.eq(node.choose("free", 3)[0], "", "free without text: nothing injected")
            c.eq(bp.short_name(stock[0]), "lieu court", "short name = field 'short'")
            c.eq(bp.short_name(stock[1]), "lieu numéro 2", "no short name yet: the name")
            c.eq(bp.ultra_text(stock[0]), stock[0]["ultra"], "ultra = the version written for this background")
            c.raises(bp.SayaBackgroundError, lambda: bp.ultra_text({"id": "BG-X", "bg": "x"}), "no written ultra: clear error")
            c.ok("ultra" in node.choose("horror", 5, ultra_detailed=True)[2], "info says ultra")

            bp.HISTORY_PATH.unlink()
            c.raises(bp.SayaBackgroundError, lambda: node.choose("all", 1, "back"), "back with an empty history: clear error")
            outs = [node.choose("all", s)[0] for s in (101, 102, 103, 104, 105, 106)]
            c.eq(len(bp.read_history()), 5, "history keeps the last 5 generations")
            c.eq(node.choose("all", 1, "back")[0], outs[-2], "back: one generation older")
            c.eq(node.choose("all", 1, "back")[0], outs[-3], "back again: two generations older")
            c.eq(node.choose("all", 1, "stay")[0], outs[-3], "stay keeps the background shown")
            c.eq(node.choose("all", 1, "forward")[0], outs[-2], "forward: one generation newer")
            node.choose("all", 1, "forward")
            c.eq(node.choose("all", 1, "forward")[0], outs[-1], "forward stops at the newest")
            for _ in range(8):
                last = node.choose("all", 1, "back")
            c.ok(last[0] == outs[-5] and "5/5" in last[2], "back stops at the oldest")
            c.eq(len(bp.read_history()), 5, "back / forward do not change the history")
            sent = node.choose("all", 107, ultra_detailed=True)[0]
            c.ok(bp.read_history()[0]["bg"] == sent and bp.read_state()["cursor"] == 0, "a new draw goes on top and resets the cursor")
            node.choose("all", 107, ultra_detailed=True)
            c.eq(bp.read_history()[1]["bg"], outs[-1], "re-running the same draw is not a new generation")
            a, b = node.IS_CHANGED("all", 1, "back"), node.IS_CHANGED("all", 1, "back")
            c.ok(a != b, "back / forward always re-run")
            c.eq(node.IS_CHANGED("all", 1, "new"), node.IS_CHANGED("all", 1, "new"), "locked draw is cached")
            bp.HISTORY_PATH.write_text(json.dumps(bp.read_history()))  # ancien format (liste seule)
            c.eq(bp.read_state()["cursor"], 0, "old history format still read")
            (tmp / "backgrounds.json").write_text(json.dumps({"backgrounds": [dict(stock[0], cat="spooky")]}))
            c.raises(bp.SayaBackgroundError, lambda: bp.load_stock(), "stock with an unknown category refused")
        finally:
            bp.STOCK_PATHS, bp.HISTORY_PATH, bp.DATA_DIR = saved
    return c.report()


TESTS = (test_background_picker,)

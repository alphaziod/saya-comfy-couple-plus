"""SayaMainPrompt: prefix/extra verbatim (LoRA triggers untouched), background locked / random / free, menus, shared history."""

from __future__ import annotations

import json
import math
import tempfile
from pathlib import Path

from harness import Check, load_pack


def _mods(tmp: Path):
    load_pack()
    from saya_couple.src.nodes import background_picker as bp
    from saya_couple.src.nodes import main_prompt as mp

    stock = [{"id": "BG-001", "cat": "bedroom", "name": "chambre", "short": "chambre", "bg": "indoors, (cozy bedroom:1.2), bed, lamp, depth of field",
              "ultra": "indoors, (cozy bedroom:1.2), bed, lamp, creased linen, brass lamp base, depth of field"},
             {"id": "BG-002", "cat": "forest", "name": "forêt", "short": "forêt", "bg": "outdoors, (deep forest:1.2), ferns, moss", "ultra": "outdoors, (deep forest:1.2), ferns, moss, bark"},
             {"id": "BF-0001", "cat": "dream", "name": "rêve", "short": "rêve", "bg": "outdoors, (giant leaf:1.2), dew", "ultra": "outdoors, (giant leaf:1.2), dew, veins"}]
    (tmp / "backgrounds.json").write_text(json.dumps({"backgrounds": stock}))
    saved = bp.STOCK_PATHS, bp.HISTORY_PATH, bp.DATA_DIR
    bp.STOCK_PATHS, bp.HISTORY_PATH, bp.DATA_DIR = [(tmp / "backgrounds.json",)], tmp / "history.json", tmp
    return bp, mp, stock, saved


def test_main_prompt():
    c = Check("main_prompt")
    with tempfile.TemporaryDirectory() as directory:
        tmp = Path(directory)
        bp, mp, stock, saved = _mods(tmp)
        try:
            node = mp.SayaMainPrompt()
            schema = mp.SayaMainPrompt.INPUT_TYPES()
            groups = [g["name"] for g in mp.tag_groups()]
            c.ok(len(groups) >= 5 and all(g in schema["required"] for g in groups), f"one menu per tag group: {groups}")
            c.ok(all(schema["required"][g][0][0] == mp.NONE for g in groups), "'none' first in every menu")
            c.eq(list(schema["required"])[:5], ["prefix", "background", "seed", "history", "ultra_detailed"], "fixed inputs first, menus after (stable widget order)")
            c.ok(len(schema["required"]["background"][0]) == len(bp.SayaBackgroundPicker.INPUT_TYPES()["required"]["category"][0]), "same background choices as the picker")
            none = {g: mp.NONE for g in groups}

            # prefix verbatim (LoRA triggers, odd spacing, weights), background locked by seed
            prefix = "best quality,  absurdres, (mystyle_v8:1.1), scnr ,addmicrodetails"
            main, bg, short, info = node.compose(prefix, "bedroom", 7, **none)
            c.ok(main.startswith(prefix + ",\n"), f"prefix copied as written, then ',\\n': {main[:70]!r}")
            c.eq(bg, stock[0]["bg"], "background = the stock entry (category + seed)")
            c.eq(main, prefix + ",\n" + stock[0]["bg"], "no menu, no extra: MAIN = prefix + background")
            again = node.compose(prefix, "bedroom", 7, **none)
            c.eq(again[1], bg, "same seed = locked background")
            c.ok(node.compose(prefix, "all", 1, **none)[1] in {s["bg"] for s in stock}, "all + seed: a stock background")
            # prefix ending with a comma / newline: no double comma
            c.eq(node.compose("prefix,\n", "bedroom", 7, **none)[0], "prefix,\n" + stock[0]["bg"], "prefix ending with ',' gets only the newline")
            c.eq(mp.join_prompt(["", "a", "", "b,", "c"]), "a,\nb,\nc", "join skips empty parts and never doubles commas")

            # menus and extra
            tags = dict(none, lighting="moonlight", palette="cool colors")
            main, bg, short, info = node.compose(prefix, "bedroom", 7, extra="my extra, <lora:foo:0.8>", **tags)
            c.eq(main, prefix + ",\n" + stock[0]["bg"] + ",\nmoonlight, cool colors,\nmy extra, <lora:foo:0.8>", "MAIN = prefix + background + chosen tags (menu order) + extra verbatim")
            c.ok("tags: moonlight, cool colors" in info, f"info lists the chosen tags: {info}")

            # ultra, free, history shared with the picker
            c.eq(node.compose(prefix, "bedroom", 7, ultra_detailed=True, **none)[1], stock[0]["ultra"], "ultra_detailed = hand-written ultra")
            main, bg, short, info = node.compose(prefix, "free", 0, custom_background="  my own place, lamp  ", **none)
            c.eq((bg, short), ("my own place, lamp", "free"), "free = custom_background as written (stripped)")
            c.eq(node.compose(prefix, "free", 0, **none)[0], prefix, "free without text: MAIN = prefix only")
            c.eq(bp.read_history()[0]["bg"], stock[0]["ultra"], "history shared with the picker (last stock draw)")
            c.eq(node.compose(prefix, "forest", 3, history="back", **none)[1], stock[0]["bg"], "history back = previous background, same text as sent")
            c.ok(math.isnan(mp.SayaMainPrompt.IS_CHANGED(prefix, "forest", 3, history="back", **none)), "back / forward: IS_CHANGED nan (runs every time)")
            c.ok(mp.SayaMainPrompt.IS_CHANGED(prefix, "forest", 3, **none) != mp.SayaMainPrompt.IS_CHANGED(prefix, "forest", 3, **dict(none, lighting="moonlight")),
                 "a menu change invalidates the cache")
        finally:
            bp.STOCK_PATHS, bp.HISTORY_PATH, bp.DATA_DIR = saved
    return c.report()


def test_main_prompt_fr():
    """The French node = the English one with French labels: same outputs for equivalent choices, complete and unique labels."""
    c = Check("main_prompt_fr")
    with tempfile.TemporaryDirectory() as directory:
        tmp = Path(directory)
        bp, mp, stock, saved = _mods(tmp)
        try:
            en, fr = mp.SayaMainPrompt(), mp.SayaMainPromptFR()
            schema_en, schema_fr = mp.SayaMainPrompt.INPUT_TYPES(), mp.SayaMainPromptFR.INPUT_TYPES()
            c.eq(len(schema_fr["required"]) + len(schema_fr["optional"]), len(schema_en["required"]) + len(schema_en["optional"]), "same number of inputs")
            cats_en, cats_fr = schema_en["required"]["background"][0], schema_fr["required"]["decor"][0]
            c.eq(len(cats_fr), len(cats_en), "every background choice has a French label")
            c.eq(len(set(cats_fr)), len(cats_fr), "French background labels are unique")
            c.ok(all(cat in mp.CATEGORIES_FR for cat in cats_en), f"no untranslated category: {[x for x in cats_en if x not in mp.CATEGORIES_FR]}")
            for group in mp.tag_groups():
                opts_fr = schema_fr["required"][group["name"] + "_fr"][0]
                c.eq(len(opts_fr), len(group["options"]) + 1, f"{group['name']}: every tag has a French label")
                c.eq(len(set(opts_fr)), len(opts_fr), f"{group['name']}: unique French labels")
            none_en = {g["name"]: mp.NONE for g in mp.tag_groups()}
            none_fr = {g["name"] + "_fr": mp.NONE_FR for g in mp.tag_groups()}
            prefix = "best quality, (mystyle_v8:1.1), scnr"
            out_en = en.compose(prefix, "bedroom", 7, ultra_detailed=True, extra="x", **dict(none_en, lighting="moonlight", palette="cool colors"))
            out_fr = fr.composer(prefix, "chambre", 7, ultra_detaille=True, complement="x", **dict(none_fr, lighting_fr="clair de lune", palette_fr="couleurs froides"))
            c.eq(out_fr, out_en, "French choices give exactly the English node's output (tags sent in English)")
            c.eq(fr.composer(prefix, "libre", 0, decor_libre="mon lieu", **none_fr)[1], "mon lieu", "libre = decor_libre tel quel")
            c.eq(fr.composer(prefix, "forêt", 3, historique="retour", **none_fr)[1], stock[0]["ultra"], "historique retour = décor précédent (historique partagé)")
            c.eq(mp.SayaMainPromptFR.IS_CHANGED(prefix, "chambre", 7, **none_fr), mp.SayaMainPrompt.IS_CHANGED(prefix, "bedroom", 7, **none_en), "same cache key")
        finally:
            bp.STOCK_PATHS, bp.HISTORY_PATH, bp.DATA_DIR = saved
    return c.report()


TESTS = (test_main_prompt, test_main_prompt_fr)

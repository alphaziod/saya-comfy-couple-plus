"""ACTION kept apart from MAIN across the phases (imprint v2 prompts.action) and explicit P1/P2 texts for the anchors."""

from harness import Check, load_pack


def test_action_imprint():
    load_pack()
    from saya_couple.src.nodes.couple_imprint_v2 import build_prompts, scene_text
    from saya_couple.src.nodes.saya_multi_couple import _ownership_anchors

    c = Check("action_imprint")
    with_action = build_prompts(main="bedroom", person_1="1girl, pink hair", person_2="1girl, horns", negative="", action="2girls, kissing")
    c.eq(with_action.get("action"), "2girls, kissing", "ACTION stored in the imprint prompts")
    c.eq(scene_text(with_action), "bedroom,\n2girls, kissing", "reconstructed phases get MAIN + ACTION")
    blank = build_prompts(main="bedroom", person_1="1girl, pink hair", person_2="1girl, horns", negative="", action="  ")
    c.ok("action" not in blank, "blank ACTION -> key absent")
    old = build_prompts(main="bedroom", person_1="1girl, pink hair", person_2="1girl, horns", negative="")
    c.eq(scene_text(old), "bedroom", "imprint without ACTION (older images) -> MAIN unchanged")
    c.eq(build_prompts(**with_action), with_action, "read-side revalidation accepts the ACTION key")
    p1 = "1girl, long white floppy ears, deep dark pink hair, small penis, erect penis"
    p2 = "1girl, long swept-back horns, vivid red streaks, medium penis, erect penis"
    got = _ownership_anchors(None, None, None, None, "", "", "phrase_anatomy", p1, p2)
    c.ok(got is not None and "small penis" in got[0][0] and "medium penis" in got[1][0],
         "explicit P1/P2 texts (identity + anatomy built in the graph) give the anchors without guessing from the prompt")
    return c.report()


def test_quality_imprint_order():
    """2.2: QUALITY kept apart in the imprint; rebuilt phases read QUALITY first and the background last."""
    load_pack()
    from saya_couple.src.nodes.couple_imprint_v2 import (SayaCoupleImprintPackV2, background_text, build_prompts,
                                                         parse_imprint_json, scene_text, solo_text)

    c = Check("quality_imprint_order")
    q = build_prompts(main="garden, roses", person_1="1girl, pink hair", person_2="1girl, horns", negative="",
                      action="2girls, kissing", quality="best quality, scnr")
    c.eq(q.get("quality"), "best quality, scnr", "QUALITY stored in the imprint prompts")
    c.eq(scene_text(q), "best quality, scnr,\n2girls, kissing,\ngarden, roses", "couple MAIN: QUALITY, ACTION, background")
    c.eq(background_text(q), "best quality, scnr,\ngarden, roses", "background cells: QUALITY, background (no ACTION)")
    c.eq(solo_text(q), "best quality, scnr,\n2girls, kissing,\n1girl, pink hair,\ngarden, roses", "solo: QUALITY, ACTION, P1, background")
    c.eq(build_prompts(**q), q, "read-side revalidation accepts the QUALITY key")
    old = build_prompts(main="best quality, garden", person_1="1girl, pink hair", negative="", action="kissing")
    c.ok("quality" not in old, "no QUALITY -> key absent (2.1 imprints)")
    c.eq((scene_text(old), background_text(old), solo_text(old)),
         ("best quality, garden,\nkissing", "best quality, garden", "best quality, garden,\nkissing, 1girl, pink hair"),
         "2.1 imprint: historic texts, unchanged")
    _, payload = SayaCoupleImprintPackV2().pack("garden, roses", "1girl, pink hair", "", "vertical", 50, 0.0, 1.0, 1.0, 832, 1216,
                                                "model.safetensors", "clip", "2.2", person_2_prompt="1girl, horns",
                                                action_prompt="2girls, kissing", quality_prompt="best quality, scnr")
    c.eq(parse_imprint_json(payload)["couple_imprint"]["prompts"].get("quality"), "best quality, scnr", "pack writes QUALITY, parse reads it back")
    return c.report()


TESTS = (test_action_imprint, test_quality_imprint_order)

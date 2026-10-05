"""ACTION kept apart from MAIN across the phases (imprint v2 prompts.action) and explicit P1/P2 texts for the anchors."""

from harness import Check, load_pack


def test_action_imprint():
    load_pack()
    from custom_nodes.Saya_Couple_Upated.src.nodes.couple_imprint_v2 import build_prompts, scene_text
    from custom_nodes.Saya_Couple_Upated.src.nodes.saya_multi_couple import _ownership_anchors

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


TESTS = (test_action_imprint,)

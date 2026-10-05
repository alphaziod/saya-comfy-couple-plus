"""Anchors of the dynamic ownership derived from the P1/P2 prompts (src/nodes/ownership_anchors.py)."""

from harness import Check, load_pack

P1 = "1girl, adult, small breasts,\nmedium-short hair, deep dark pink hair, fluffy messy hair, side bangs,\n(white eyes:1.1), natural anime eyes, shy expression,\nreal lop ears, (long white floppy ears:1.3), fluffy white rabbit tail, oversized cream sweater"
P2 = "1girl, adult, small breasts,\nmedium-length messy hair, (chestnut-brown hair:1.3), (vivid red streaks:1.4), red streaks through bangs,\nlong swept-back horns, slender demon tail,\n(pale skin:1.4), muted crimson eyes:1.3, relaxed face, obsidian circlet between the horns"


def test_ownership_anchors():
    load_pack()
    from custom_nodes.Saya_Couple_Upated.src.nodes.ownership_anchors import discriminant_anchors, head_category

    c = Check("ownership_anchors")
    a1, a2 = discriminant_anchors(P1, P2), discriminant_anchors(P2, P1)
    c.eq(a1, ["long white floppy ears", "deep dark pink hair", "white eyes"], "P1: ears, hair colour, eyes")
    c.eq(discriminant_anchors("1girl, pale penis, pink hair", "1girl, medium penis, horns"), ["pink hair"], "same organ on both: no anatomy anchor")
    c.eq(discriminant_anchors("1girl, vulva, pink hair", "1girl, penis, horns")[:1], ["vulva"], "organ only one person has: anatomy anchor")
    c.eq(a2, ["long swept-back horns", "vivid red streaks", "muted crimson eyes"], "P2: horns, streaks, eyes")
    generic = {"1girl", "adult", "small breasts", "woman", "body", "standing"}
    c.ok(not generic & set(a1 + a2), "no generic or shared phrase")
    c.eq(head_category("obsidian circlet between the horns"), "outfit", "an item placed on a feature is an outfit item")
    c.eq(discriminant_anchors("1girl, adult, smile", "1girl, adult, blush"), [], "nothing identity-bearing: no anchor (static split)")
    from custom_nodes.Saya_Couple_Upated.src.nodes.ownership_anchors import distinctive_anchor_words
    p1 = "1girl, deep dark pink hair, small penis, erect penis"
    p2 = "1girl, chestnut-brown hair, faint pink blush, small nipples, medium penis, erect penis"
    d1 = dict(distinctive_anchor_words(p1, p2))
    c.eq(d1.get("small penis"), ["small"], "distinctive: shared noun dropped, size kept")
    c.ok("pink" in d1.get("deep dark pink hair", []), "distinctive: pink kept although the other has pink blush")
    c.eq(dict(distinctive_anchor_words(p2, p1)).get("medium penis"), ["medium"], "distinctive: the other organ's size")
    c.eq(discriminant_anchors(p1, p2), ["deep dark pink hair"], "phrase mode unchanged: no anatomy anchor for a shared organ")
    from custom_nodes.Saya_Couple_Upated.src.nodes.ownership_anchors import hybrid_anchor_words
    h1 = dict(hybrid_anchor_words("1girl, long white floppy ears, deep dark pink hair, small penis", "1girl, long swept-back horns, chestnut-brown hair, medium penis"))
    c.eq(h1.get("long white floppy ears"), ["long", "white", "floppy", "ears"], "hybrid: own noun (ears vs horns) keeps the whole phrase")
    c.eq(h1.get("deep dark pink hair"), ["deep", "dark", "pink"], "hybrid: shared noun hair dropped")
    c.eq(h1.get("small penis"), ["small"], "hybrid: shared organ keeps its size only")
    from custom_nodes.Saya_Couple_Upated.src.nodes.ownership_anchors import phrase_anatomy_anchors
    pa_1 = "1girl, long white floppy ears, deep dark pink hair, small penis, erect penis"
    pa_2 = "1girl, long swept-back horns, chestnut-brown hair, medium penis, erect penis"
    c.eq(phrase_anatomy_anchors(pa_1, pa_2), discriminant_anchors(pa_1, pa_2) + ["small penis"], "phrase_anatomy: phrase anchors unchanged + whole distinct anatomy phrase")
    same = "1girl, long white floppy ears, medium penis"
    c.eq(phrase_anatomy_anchors(same, "1girl, horns, medium penis"), discriminant_anchors(same, "1girl, horns, medium penis"), "phrase_anatomy: identical anatomy adds nothing")
    c.eq(phrase_anatomy_anchors("1girl, futanari, pink hair, medium breasts", "1boy, blue hair, flat chest"), ["pink hair", "medium breasts", "futanari"],
         "phrase_anatomy: distinct chest and body type added after the head anchors")
    c.eq(phrase_anatomy_anchors("1girl, fox girl, orange hair", "1girl, futanari, black hair"), ["orange hair", "fox girl"], "body type: fox girl against 1girl")
    c.eq(phrase_anatomy_anchors("1girl, futanari, pink hair, small breasts", "1girl, futanari, blue hair, small breasts"), ["pink hair"],
         "same body type and chest: head anchors only")
    c.eq(head_category("pubic hair"), "anatomy", "pubic hair is on the organ, not on the head")
    c.eq(head_category("short black pixie cut"), "hair", "Danbooru hair style noun")
    c.eq(phrase_anatomy_anchors("1girl, pink hair, small clitoris, pussy", "1girl, blue hair, large clitoris, vagina"), ["pink hair", "small clitoris"],
         "clitoris size distinct; pussy / vagina are one organ")
    c.eq(phrase_anatomy_anchors("1girl, pink hair, pussy", "1girl, blue hair, vagina"), ["pink hair"], "synonym organ: no anatomy anchor")
    c.eq(phrase_anatomy_anchors("1girl, pink hair, medium penis, flacid penis", "1girl, blue hair, medium penis, erect penis"), ["pink hair", "flacid penis"],
         "state distinguishes the organ (flacid / limp = flaccid)")
    c.eq(phrase_anatomy_anchors("1girl, pink hair, open anus, closed pussy", "1girl, blue hair, tight anus"), ["closed pussy", "pink hair", "open anus"],
         "one anchor per organ")
    c.eq(phrase_anatomy_anchors("1girl, pink hair, thick thighs, wide hips, red lips, spread legs, hands on own hips", "1girl, blue hair, slim legs, narrow waist"),
         ["pink hair", "thick thighs", "wide hips", "red lips"], "body shape and details: one anchor per part, poses ignored")
    return c.report()


TESTS = (test_ownership_anchors,)

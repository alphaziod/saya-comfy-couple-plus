"""Discriminant identity anchors of the dynamic ownership, derived from the P1 and P2 prompts.

A prompt is split into tag phrases (commas / lines, weights removed). A phrase's category is that of its
head noun: the last attribute noun before the first preposition or participle ("obsidian circlet between
the horns" is an outfit item, not horns). Only that head segment is scored and used as the anchor, so a
phrase locating an item on the other person ("tail curling around the rabbit-eared woman's calf") never
pulls words of that person. A segment is a candidate when it is distinctive: its words do not appear in
the other person's prompt. One phrase per category is kept, in
category order, at most MAX_ANCHORS: e.g. "long white floppy ears, deep dark pink hair, white eyes"
against "long swept-back horns, vivid red streaks, muted crimson eyes". No anchor = no dynamic ownership.
"""

from __future__ import annotations

import re

MAX_ANCHORS = 3

CATEGORIES = (
    ("species", {"horn", "horns", "ear", "ears", "tail", "tails", "wing", "wings", "halo", "antler", "antlers", "fang", "fangs",
                 "antennae", "fins", "tentacles", "tusks", "claws", "paws", "hooves", "feathers", "gills", "stinger", "mane", "joints", "implants", "plating", "core", "limb", "prosthetic", "vines", "leaves", "moss", "crystals", "petals", "stitches", "gears"}),
    # Anatomy only one of the two persons has (the noun itself, see EXCLUSIVE_NOUN): anchors the organ to its
    # owner wherever it goes. Two persons with the same organ ("pale penis" / "medium penis") give no anchor:
    # the noun would light both organs.
    ("anatomy", {"penis", "penises", "glans", "scrotum", "testicles", "foreskin", "erection", "pussy", "labia", "clitoris", "anus",
                 "pubic"}),
    # Head nouns of the most used Danbooru hair / eyes / skin tags (tag groups hair_styles, eyes_tags; *_hair, *_eyes, ... by post count).
    ("hair", {"hair", "streaks", "streak", "bangs", "locks", "ponytail", "twintails", "braid", "braids", "bob", "pixie", "curls",
              "forelock", "bun", "buns", "afro", "mohawk", "dreadlocks", "undercut", "mullet", "cut", "sidelocks", "sidelock", "ahoge",
              "drills", "drill", "ringlets", "updo", "hairdo", "topknot", "cornrows", "quiff", "pompadour", "flattop", "sidecut",
              "intakes", "bald", "waves", "strands", "tuft", "chignon", "tresses", "fringe", "highlights", "pigtails", "plait", "plaits"}),
    ("eyes", {"eye", "eyes", "pupils", "irises", "iris", "sclera", "eyelashes", "eyebrows", "heterochromia", "eyeliner", "eyeshadow",
              "tsurime", "tareme", "lashes", "brows", "monocle", "eyepatch"}),
    ("skin", {"skin", "skinned", "tan", "tanlines", "freckles", "mole", "birthmark", "scar", "scars", "tattoo", "tattoos", "markings",
              "mark", "scales", "fur", "marks", "spots", "patterns", "patches", "runes", "texture", "lipstick", "makeup", "blush", "dimples", "cheekbones", "jawline", "chin", "nose", "lines", "smudge", "stains", "smears", "bruises", "veining", "beard", "stubble", "mustache", "moustache", "sideburns", "goatee"}),
    ("outfit", {"sweater", "cardigan", "coat", "cape", "cloak", "dress", "gown", "robe", "robes", "armor", "armour", "helm",
                "helmet", "gauntlet", "gauntlets", "gloves", "socks", "stockings", "scarf", "choker", "harness", "jacket",
                "shirt", "skirt", "kimono", "yukata", "hoodie", "veil", "glasses", "goggles", "earrings", "necklace",
                "bracelet", "ribbon", "bow", "mask", "circlet", "crown", "satchel", "bag", "boots", "sandals", "bodice", "fan",
                "hairpin", "hairpins", "plume", "armband", "wristband", "hairclip", "collar", "belt", "hat", "jewelry", "chains", "anklets", "anklet", "rings", "ring", "headphones", "eyewear", "bodysuit", "suit", "uniform", "accessories", "pen", "pencil", "straw", "flowers", "flower", "snowflakes", "headband", "tiara", "garter", "bandages", "earpiece"}),
)
# Body anchors, added after the head anchors in phrase_anatomy mode (never picked as one of the MAX_ANCHORS head
# anchors): the chest and the body type / population (Danbooru: futanari, 1boy, otoko no ko, furry, cat girl,
# monster girl...). Each only when its words differ from the other person's ("large breasts" / "small breasts",
# "futanari" / "1girl", "fox girl" / "1girl"); two "1girl, futanari" give none.
BODY_CATEGORIES = (
    ("chest", {"breasts", "nipples", "areolae", "chest", "pecs", "pectorals"}),
    ("body", {"futanari", "futa", "girl", "girls", "boy", "boys", "woman", "women", "man", "men", "male", "female", "lady", "ladies",
              "guy", "dude", "gentleman", "femboy", "trap", "newhalf", "shemale", "dickgirl", "otoko", "tomboy", "crossdresser",
              "androgynous", "mature", "milf", "gyaru", "furry", "anthro", "kemonomimi", "nekomimi", "catgirl", "catboy", "foxgirl",
              "wolfgirl", "doggirl", "bunnygirl", "elf", "dwarf", "fairy", "witch", "goddess", "android", "robot", "cyborg",
              "vampire", "succubus", "angel", "demon", "demoness", "devil", "oni", "monster", "dragon", "lamia", "harpy", "mermaid",
              "centaur", "slime", "orc", "goblin", "giant", "ghost", "zombie", "kitsune", "dryad", "undead", "yokai", "gorgon", "spirit", "nymph", "nereid", "golem", "gargoyle", "sphinx", "djinn", "imp", "valkyrie", "seraph", "alien", "ghoul", "doll", "automaton", "statue", "minotaur", "manticore", "nekomata", "masculine", "feminine", "intersex", "hermaphrodite", "nonbinary", "transgender", "genderless", "agender", "bishounen", "otokonoko", "josou", "reverse", "bunny", "rabbit", "fox", "wolf", "cat", "cow", "cheetah", "bat", "dragoness"}),
    # Body shape and small body details (Danbooru: thick_thighs, wide_hips, narrow_waist, abs, huge_ass, red_lips,
    # sharp_teeth, black_nails, navel_piercing, muscular, plump, curvy, petite...): one anchor per part.
    ("parts", {"thighs", "hips", "waist", "abs", "navel", "belly", "ass", "legs", "arms", "shoulders", "collarbone", "armpits",
               "ribs", "feet", "hands", "nails", "toenails", "lips", "teeth", "tongue", "piercing", "veins", "build", "physique",
               "muscular", "toned", "plump", "curvy", "chubby", "skinny", "slim", "slender", "petite", "athletic", "voluptuous",
               "stocky", "lanky", "shortstack", "giantess", "figure", "tall", "nail", "polish", "posture"}),
)
CATEGORY_OF = {noun: name for name, nouns in CATEGORIES + BODY_CATEGORIES for noun in nouns}
CATEGORIES_BY_NAME = dict(CATEGORIES + BODY_CATEGORIES)
EXCLUSIVE_NOUN = {"anatomy"}
# Where the head phrase stops: what follows only says where the item is.
HEAD_END = {"in", "on", "over", "behind", "between", "through", "with", "under", "around", "across", "of", "from", "into",
            "near", "against", "beside", "toward", "towards", "along", "above", "below", "like", "than", "each",
            "showing", "pushed", "pinned", "threaded", "tied", "draped", "poking", "gripping", "holding"}

# Words that never make a phrase distinctive on their own.
GENERIC = {
    "a", "an", "the", "and", "with", "of", "on", "in", "at", "to", "no", "very", "slightly", "slight", "both",
    "girl", "girls", "woman", "women", "adult", "person", "people", "body", "standing", "solo", "duo", "character",
    "characters", "anime", "natural", "soft", "gentle", "real", "young", "face", "expression", "gaze", "look",
    "mannerisms", "language", "one", "two", "side", "medium", "long", "short", "small", "large", "big", "detailed",
}
# Size words: generic across categories, but distinctive inside one (distinctive mode: "small penis" / "medium penis").
SIZE = {"medium", "long", "short", "small", "large", "big", "thick", "thin", "tiny", "huge"}


def phrases(text: str) -> list[str]:
    out = []
    for raw in re.split(r"[,\n]", text):
        phrase = re.sub(r"[()\[\]{}]", "", raw)
        # every weight, also nested or stacked ones ("(white eyes:1.2):1.1", "eyes:1.2 :1.3:1.2")
        phrase = re.sub(r"\s*:\s*-?[\d.]+", "", phrase).strip().lower()
        if phrase:
            out.append(phrase)
    return out


# Synonyms compared as one word: "pussy" (the tag Illustrious knows best) and "vagina" are the same organ, so they
# never make two persons distinct, while "small clitoris" / "large clitoris" do.
SYNONYM = {"flacid": "flaccid", "limp": "flaccid", "hard": "erect", "gape": "gaping", "spreading": "spread", "vagina": "pussy", "vulva": "pussy", "cunt": "pussy", "cock": "penis", "dick": "penis", "balls": "testicles",
           "breast": "breasts", "tits": "breasts", "boobs": "breasts", "nipple": "nipples", "areola": "areolae", "clit": "clitoris",
           "butt": "ass", "buttocks": "ass", "stomach": "belly", "tummy": "belly", "fingernails": "nails", "piercings": "piercing"}


def words(text: str) -> list[str]:
    # "doll-like", "cat-like": a comparison, not the noun itself (the succubus is not a doll)
    text = re.sub(r"[a-z]+-like\b", " ", text.lower())
    # gender tags written with a hyphen or a space: one word, as Danbooru tags them
    text = re.sub(r"\bnew[- ]half\b", "newhalf", text)
    text = re.sub(r"\bfem[- ]boy\b", "femboy", text)
    text = re.sub(r"\botoko[- ]no[- ]ko\b", "otokonoko", text)
    text = re.sub(r"\bnon[- ]binary\b", "nonbinary", text)
    text = re.sub(r"\bdick[- ]girl\b", "dickgirl", text)
    return [SYNONYM.get(w, w) for w in re.findall(r"[a-z]+", text)]


def head_segment(phrase: str) -> str:
    """The phrase up to its first preposition or participle."""
    for match in re.finditer(r"[a-z]+", phrase):
        if match.group() in HEAD_END and not phrase[:match.start()].endswith("-"):
            return phrase[:match.start()].strip()
    return phrase


def head_category(phrase: str) -> str | None:
    head = None
    ws = words(head_segment(phrase))
    if "pubic" in ws:  # "pubic hair" sits on the organ, not on the head
        return "anatomy"
    for w in ws:
        if w in CATEGORY_OF:
            head = CATEGORY_OF[w]
    return head


def _category_words(text: str, name: str) -> set[str]:
    return {w for phrase in phrases(text) if head_category(phrase) == name for w in words(head_segment(phrase))}


def _pick(own: str, other: str, distinctive: bool) -> list[str]:
    other_phrases = set(phrases(other))
    all_other_words = set(words(other))
    picks = []
    for name, nouns in CATEGORIES:
        # Distinctive mode compares within the category: "pink" in the other's "pink blush" does not make
        # "pink hair" shared, "small" in the other's "small nipples" does not make "small penis" shared.
        other_words = _category_words(other, name) if distinctive else all_other_words
        generic = GENERIC - SIZE if distinctive else GENERIC
        best = None
        for phrase in phrases(own):
            segment = head_segment(phrase)
            if phrase in other_phrases or segment in other_phrases or segment in picks or head_category(phrase) != name:
                continue
            ws = words(segment)
            modifiers = {w for w in ws if w not in other_words and w not in generic and w not in nouns}
            own_nouns = {w for w in set(ws) & nouns if w not in other_words}
            if name in EXCLUSIVE_NOUN and not own_nouns and not distinctive:
                continue
            score = len(modifiers) + 2 * len(own_nouns)
            if score > 0 and (best is None or score > best[0]):
                best = (score, segment)
        if best is not None:
            picks.append(best[1])
        if len(picks) == MAX_ANCHORS:
            break
    return picks


def discriminant_anchors(own: str, other: str) -> list[str]:
    """Anchor phrases; the whole head segment is attended ("deep dark pink hair")."""
    return _pick(own, other, distinctive=False)


def distinctive_anchor_words(own: str, other: str) -> list[tuple[str, list[str]]]:
    """Distinctive mode: (segment, words to attend) where only the segment's words absent from the other
    prompt are attended. A noun both persons have ("hair", "penis") is dropped from the anchor, so an organ
    both have can be anchored by its distinct modifiers ("small penis" -> "small")."""
    out = []
    for segment in _pick(own, other, distinctive=True):
        other_words = _category_words(other, head_category(segment))
        keep = [w for w in words(segment) if w not in other_words and w not in GENERIC - SIZE]
        if keep:
            out.append((segment, keep))
    return out


def hybrid_anchor_words(own: str, other: str) -> list[tuple[str, list[str]]]:
    """Hybrid mode: like distinctive, but a phrase whose head noun the other person does not have in that
    category ("ears" against "horns") keeps all its words: the noun is the strongest localized signal. A shared
    noun ("hair", "eyes", "penis") is still dropped, only its distinct modifiers are attended."""
    out = []
    for segment in _pick(own, other, distinctive=True):
        name = head_category(segment)
        other_words = _category_words(other, name)
        nouns = CATEGORIES_BY_NAME[name]
        ws = words(segment)
        if any(w in nouns and w not in other_words for w in ws):
            out.append((segment, ws))
            continue
        keep = [w for w in ws if w not in other_words and w not in GENERIC - SIZE]
        if keep:
            out.append((segment, keep))
    return out


# A pose, not an identity: "spread legs", "crossed arms", "hands on own hips" are never body part anchors.
POSE = {"spread", "crossed", "outstretched", "raised", "bound", "clenched", "holding", "grabbing", "folded", "lifted", "open"}


# Hair on the organ ("green hairy penis", "pink pubic hair"): a part of its own, kept next to the organ's size or
# state ("medium penis", "erect penis"). Its colour is the person's hair colour, so it tells whose organ it is.
PUBIC_HAIR = {"hairy", "pubic"}


def _head_noun(segment: str, name: str) -> str | None:
    if name == "anatomy" and PUBIC_HAIR & set(words(segment)):
        return "pubic"
    head = None
    for w in words(segment):
        if w in CATEGORIES_BY_NAME[name]:
            head = w
    return head


def body_anchors(own: str, other: str, name: str) -> list[str]:
    """Per body part of category name (penis, pussy, anus, breasts, nipples...), the person's phrase whose words
    differ from the other person's phrases on that same part, attended whole: size or state ("small penis" /
    "medium penis", "flaccid penis" / "erect penis", "gaping anus" / "anus") tells whose part it is. Parts both
    describe with the same words give nothing."""
    other_parts = {}
    for phrase in phrases(other):
        segment = head_segment(phrase)
        if head_category(segment) == name:
            other_parts.setdefault(_head_noun(segment, name), set()).update(words(segment))
    best = {}
    for phrase in phrases(own):
        segment = head_segment(phrase)
        if head_category(segment) != name or name == "parts" and (segment != phrase or POSE & set(words(segment))):
            continue
        part = _head_noun(segment, name)
        score = len([w for w in words(segment) if w not in other_parts.get(part, set()) and w not in GENERIC - SIZE])
        if score > 0 and (part not in best or score > best[part][0]):
            best[part] = (score, segment)
    return [segment for _, segment in best.values()]


def phrase_anatomy_anchors(own: str, other: str) -> list[str]:
    """phrase_anatomy mode: the validated phrase anchors, plus the distinct anatomy, chest, body type and body part phrases."""
    picks = discriminant_anchors(own, other)
    if not picks:
        return picks
    for name in ("anatomy", "chest", "body", "parts"):
        picks += [extra for extra in body_anchors(own, other, name) if extra not in picks]
    return picks

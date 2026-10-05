# SFW PROMPT GUIDE — Saya Couple (dynamic ownership, Saya Couple 2.0 — 2026-10-05)

> Guide for the **dynamic** ownership mode (Saya Couple 2.0). Based on: duo grammar r6 (30/30 duos),
> batches G/H/I (900 images, Codex reviews), J/K tests, SFW validation of MAIN background (v19), 100-character crash test.

## 1. The 4 fields + the negative

```
┌──────────────────────────────────────────────────────────────────────────┐
│ MAIN  = the BACKGROUND only                                              │
│   quality, LoRA triggers, style, scenery, lighting, palette              │
│   → the only text background pixels receive (background = MAIN)          │
├──────────────────────────────────────────────────────────────────────────┤
│ ACTION = what the characters do IN the space                             │
│   count, framing, who is LEFT / RIGHT, pose, gestures, faces              │
│   → received by the characters (MAIN + ACTION), never by the background  │
├──────────────────────────────────────────────────────────────────────────┤
│ P1 = appearance of the RIGHT character  (identity, anchors)              │
│ P2 = appearance of the LEFT character   (identity, anchors)              │
├──────────────────────────────────────────────────────────────────────────┤
│ NEGATIVE = anti-quality + anti-inset (+ anti-nudity in SFW)              │
└──────────────────────────────────────────────────────────────────────────┘
```

**Wiring** (`SayaMultiCouple` node):
- `ownership` = `dynamic`;
- `anchor_tokens` = `phrase_anatomy`;
- `action` input = a separate `CLIPTextEncode` (the ACTION text).

When `action` is connected, the node turns `background_main` on by itself: the background gets MAIN only, the characters get MAIN + ACTION.

Without `action` connected, put the ACTION text at the end of MAIN: the background then also gets the pose, which gives more insets.

**Left / right order:** P2 is on the LEFT, P1 on the RIGHT. The word "LEFT" in the ACTION always means P2.

---

## 2. MAIN — the background

### 2.0 — the MAIN node

Since Saya Couple 2.0 the MAIN text can be built by one node, **Saya Main Prompt · Prefix + Background + Tags** (`SayaMainPrompt`, French labels: `SayaMainPromptFR`): `prefix` (your quality tags and LoRA triggers, copied as written), a background drawn from the stock of 7 409 categorized backgrounds (41 categories, `seed` fixed = locked, randomize = a new one each run, `free` = your own text, `ultra_detailed` = the long hand-written version), one tag per drop-down menu (lighting, time, weather, particles, palette, detail), and `extra`. The order follows the 9 blocks below. Phase 1 reads each pixel's owner from the model's own attention (`ownership = dynamic`) and the *Saya Ownership Map* node hands that map to every later pass.

MAIN is the only text the background pixels receive, and the characters receive it too. It describes **the place, the objects, the light and the mood**, never a person or a gesture. A **dense** background helps the mask: the more objects and materials the scenery has, the better the model separates the background from the bodies.

### 2.1 The order (9 blocks)

```
[1 quality], [2 LoRA triggers], [3 place: indoors/outdoors + location],
[4 objects and furniture: 5 to 10], [5 light: time + source], [6 weather / sky],
[7 particles / atmosphere], [8 palette], [9 depth / detail]
```

Example:

```
best quality, very aesthetic, absurdres, highres, <your LoRA triggers>,
indoors, (detailed cozy bedroom:1.2),
wooden floor, rumpled bed with many pillows, bookshelf full of books, potted plant, desk lamp, curtains, carpet,
evening, warm lamplight, sunset light through the window, light rays, dust particles,
warm colors, depth of field, rich detailed background
```

### 2.2 Blocks 1–2 — Quality and triggers

- **Validated prefix:** `best quality, very aesthetic, absurdres, highres` ✓. The validated checkpoint (Illustrious-based, filed as "no tags") does not need more quality tags. Do not stack more.
- **Triggers:** `<your LoRA triggers>` ✓, once (§6).
- **Rating tag** ○: `general` / `explicit`. It is an Illustrious convention, not tested here. Try it alone on 10 images.

### 2.3 Block 3 — The place (most used Danbooru tags)

Start with `indoors` or `outdoors`, then **one** main location. Weighting it is allowed, like Saya's MAIN: `(highly detailed cluttered anime gamer bedroom:1.2)`.

| Indoors | Outdoors | Fantasy / other |
|---|---|---|
| `bedroom`, `living room`, `kitchen` | `forest`, `bamboo forest`, `flower field` | `ruins`, `castle`, `throne room` |
| `bathroom`, `onsen`, `hot spring` | `beach`, `ocean`, `lake`, `riverbank` | `cathedral`, `shrine`, `temple` |
| `classroom`, `library`, `office` | `city`, `street`, `alley`, `rooftop` | `dungeon`, `cave`, `floating island` |
| `cafe`, `bar (place)`, `restaurant` | `park`, `garden`, `courtyard` | `space station`, `spaceship interior` |
| `hallway`, `dressing room`, `ballroom` | `mountain`, `cliff`, `desert`, `snowfield` | `cyberpunk city`, `steampunk workshop` |

### 2.4 Block 4 — Objects and furniture (5 to 10, they make the background dense)

- **Floor and walls:** `wooden floor`, `tatami`, `carpet`, `stone floor`, `brick wall`, `wallpaper`, `shoji`, `sliding doors`.
- **Furniture:** `bed`, `pillow`, `blanket`, `couch`, `chair`, `table`, `desk`, `bookshelf`, `cabinet`, `fireplace`.
- **Objects:** `book`, `potted plant`, `flower vase`, `teapot`, `cup`, `candle`, `lamp`, `lantern`, `clock`, `globe`, `rug`, `cushion`, `stuffed toy`.
- **Plants:** `plant`, `flower`, `ivy`, `tree`, `cherry blossoms`, `falling petals`, `leaves`.

### 2.5 Blocks 5–7 — Light, time, weather, atmosphere

| Time | Light source | Light effect | Sky / weather | Particles |
|---|---|---|---|---|
| `day`, `morning`, `dawn` | `sunlight`, `moonlight` | `light rays`, `sunbeam` | `blue sky`, `cloud`, `cloudy sky` | `light particles`, `sparkle` |
| `sunset`, `dusk`, `twilight` | `candlelight`, `lamp`, `lantern` | `dappled sunlight`, `backlighting` | `starry sky`, `aurora`, `moon` | `dust`, `steam`, `smoke` |
| `evening`, `night` | `chandelier`, `neon lights`, `fireplace` | `sidelighting`, `dim lighting`, `lens flare` | `rain`, `snow`, `fog` | `petals`, `snowflakes`, `embers` |

`backlighting` and `dim lighting` darken the faces: keep them for mood, not for an identity scene.

### 2.6 Blocks 8–9 — Palette and depth

- **Palette:** `warm colors`, `cool colors`, `pastel colors`, `muted colors`, `colorful`, or a theme (`blue theme`, `pink theme`…).
- **Depth:** `depth of field` ✓, `rich detailed background` ✓, `rich layered background` ✓. Avoid strong `blurry background` and `bokeh`: the background turns blurry, so less dense.

### 2.7 Banned in MAIN

- **People and gestures:** anything about a character goes in the ACTION or in P1/P2.
- **Empty backgrounds:** `simple background`, `white background`, `black background`, `grey background`, `gradient background`, "studio", "spotlight on black". The background becomes flat, with no cue for the mask.
- **Objects that create a person or an inset:** `mirror`, `reflection`, `statue`, `doll`, `portrait`, `painting \(object\)`, `poster \(object\)`, `photo \(object\)`, `television`, `monitor`, screens, canvases, sketches, and a dark window at night (it reflects the characters).
  Saya's original MAIN (node 144) contains `dual monitors`: a candidate to watch if insets appear in that scene.
- **Styles that remove color:** `monochrome`, `greyscale`, `sepia`, `lineart`. The mask relies on hair and eye colors.
- **Horror or transformation themes** (`horror \(theme\)`, corruption…) in an identity control scene.

---

## 3. ACTION — who the characters are, where they are, what they do

The ACTION tells the model **how many** characters there are, **where** each one is anchored in the space, **how** each one stands, what they do **together** and **where the faces are**. It is the field with the most impact on the composition.

The characters receive MAIN + ACTION; the background never receives it.

Legend: ✓ = validated in our batches · ○ = common Danbooru tag, not yet tested here.

*Test status:* in batches I/J/K, the count and framing (blocks 1–2) were still in the MAIN prefix. Putting them in the ACTION follows the "background = MAIN" rule: the background no longer receives "2girls", so fewer people appear in the scenery. To be confirmed on a small batch (10 images) before a full batch.

### 3.1 The order (8 blocks, one line each)

```
[1 population], [2 framing / camera],
[3 left/right anchoring + depth/height + support],
[4 posture of each], [5 INTERACTION TAG:1.3], [6 secondary tag:1.2],
[7 where the faces are], [8 1–2 short gestures]
```

Example:

```
2girls, 2 adult characters, duo, (fully clothed:1.2), modest clothing,
(upper body:1.2), close-up two-shot, from side, (single frame:1.2), (both faces visible:1.2), heads separated,
the red-streaked brunette woman on the LEFT and the pink-haired woman on the RIGHT, at the same height, sitting on a wide sofa,
both sitting, (head on another's shoulder:1.3), (holding hands:1.2),
both faces in three-quarter view, eye contact,
the red-streaked brunette woman's free hand resting on her knee
```

### 3.2 Block 1 — Population (who)

The count is the anti-fusion base: without it, 17% of images merge the two characters.

| Duo | ACTION (population) | In each character (P1/P2) |
|---|---|---|
| two girls | `2girls, 2 adult characters, duo` ✓ | `1girl` |
| girl + boy | `1girl, 1boy, 2 adult characters, duo` ○ | `1girl` / `1boy` |
| two boys | `2boys, 2 adult characters, duo` ○ | `1boy` |
| girl + androgynous / femboy | `1girl, 1boy, duo` ○ | `1boy, otoko no ko` (Danbooru: `trap` / `otoko_no_ko`) |
| non-human / undefined character | `1girl, 1other, duo` ○ | `1other, …` |

**Rules:**
- Always `2 adult characters`.
- Always `duo`, never `solo`.
- The count goes in the ACTION, not in P1/P2. Each character keeps its own `1girl` / `1boy`.

**Differences between the two (separation bonus):** `height difference` ○, `size difference` ○, `skin color difference` ○. They help the model build two distinct bodies.

### 3.3 Block 2 — Framing and camera

| Tag | Effect | Status |
|---|---|---|
| `(upper body:1.2)`, `close-up two-shot` | two heads and two torsos close together, best for identity | ✓ |
| `cowboy shot` | down to the thighs: standing poses, hands visible | ○ |
| `full body` | whole bodies, smaller faces (identity less reliable) | ○ |
| `from side` | profiles: the safest view for two faces in contact | ✓ (J/K) |
| `from above` / `from below` | lying seen from above / dominant character | ○ |
| `from behind` | **risky**: faces disappear or an inset appears | avoid |
| `dutch angle`, `wide shot` | style, wider scenery | ○ |
| `pov` | **avoid** in a duo: the viewer becomes a third person | ✗ |
| `(single frame:1.2)` | a single panel, no comic | ✓ |
| `(both faces visible:1.2)`, `heads separated` | two faces, two heads | ✓ |

### 3.4 Block 3 — Anchoring in space (where)

This is what puts each body on its mask zone. P2 = **LEFT**, P1 = **RIGHT** (always).

- **Left / right:** `the [P2 attr] on the LEFT and the [P1 attr] on the RIGHT` ✓. One attribute per character, **a hair or color one** ("the auburn-haired woman"), never a species trait (`-eared`, `horned`, `tailed`): the ACTION applies to both characters; for generic characters: `the woman on the LEFT and the man on the RIGHT`.
- **Height:** `at the same height` (safest), `slightly above`, `lower in the frame`. Avoid extreme `above` / `below`: NS-049 gave a tiny P1.
- **Depth:** `side by side` ✓, `slightly in front`, `slightly behind`. Keep one body from hiding the other.
- **Support** (anchors both bodies to the same object): `on a wide sofa`, `on the bed`, `on the floor`, `against the wall`, `at the edge of the bed`, `on one wide armchair` ✓, `on a bench`.
- **Orientation to each other:** `face-to-face` ○, `side-by-side` ✓, `back-to-back` ✓, `facing another` ○, `facing away` (hides a face), `cheek-to-cheek` ○, `forehead-to-forehead` ○.

### 3.5 Block 4 — Posture of each

| Standing / sitting | Low / lying | Body |
|---|---|---|
| `standing` ✓ | `kneeling` | `leaning forward` |
| `sitting` ✓ | `on one knee` | `leaning back` |
| `sitting on lap` | `seiza` | `arched back` |
| `straddling` | `lying`, `on back` | `head tilt` |
| `squatting` | `on side`, `on stomach` | `arms behind back` |
| `reclining` | `all fours` | `hand on own hip` |

One posture per character: `the red-streaked brunette woman standing, the pink-haired woman sitting on the desk`. With a posture that lowers a character (kneeling, on stomach), always say where its face is (block 7).

### 3.6 Blocks 5–6 — Interaction (one main tag at 1.3, one secondary at 1.2)

| Contact | Tag | Status |
|---|---|---|
| arm | `(arm around shoulder:1.3)` | ✓ stable duo |
| embrace | `(hug:1.3)` / `(hug from behind:1.3)` | ○ / ✓ |
| back | `(back-to-back:1.3)` | ✓ essential for this pose |
| head | `(head on another's shoulder:1.3)` | ✓ gentle contact |
| hands | `(holding hands:1.3)` / `(interlocked fingers:1.2)` | ✓ / ○ |
| face | `(hand on another's face:1.2)`, `(hand on another's head:1.2)` | ○ |
| carried | `(princess carry:1.3)`, `(piggyback:1.3)`, `(carrying:1.3)` | ○ (check both faces) |
| lap | `(lap pillow:1.3)`, `(sitting on lap:1.3)` | ○ |
| kiss | `(kissing:1.3)`, `(cheek kiss:1.2)` | ✓ heads in contact hold |
| shared object | `(shared blanket:1.3)`, `(shared umbrella:1.3)`, `(sharing food:1.2)` | ✓ / ○ |
| gaze | `eye contact`, `looking at another` | ○ (in block 7) |
| side by side | `(standing side by side:1.2)`, `(sitting side by side:1.2)` | ✓ as secondary |

### 3.7 Block 7 — Where the faces are (the rule learned in G/H/I)

- Say where **each** head is: `both faces in profile`, `her face above looking down`, `her head resting on the pillow, face turned to the camera`, `both faces at the top of the frame`.
- **A pose that hides a face = merged heads**: one head against the other's belly or back, a close-up on the hands, `facing away`.
- **Impossible view = inset.** "head turned back over her shoulder looking at…" in a back-facing pose: the model draws an inset to show the face. Pick the angle where both faces are naturally visible: `from side`, profiles.
- Possible gazes: `eye contact`, `looking at another`, `looking at viewer` (only one of the two), `closed eyes`.

### 3.8 Block 8 — Gestures

One or two short gestures, named per character: `the red-streaked brunette woman raising her staff`, `the pink-haired woman holding a mug`. No more than two: each gesture is one more constraint on bodies that are close together.

### 3.9 Banned in the ACTION

- **Appearance traits** (hair, eyes, horns, outfit): they go in P1/P2. Written in the ACTION, they apply to both characters.
- **Negations** (`no merging`, `no one else`): the negated word can still apply.
- **Fusion prose**: `no gap`, `pressed against`, `wedged`, `tangled`.
- **Close-ups** (`close-up on…`, `extreme focus`) when both faces must be visible.
- **Body transformations** in a control scene.
- **Mirrors, reflections, clones**: "effect" category, counted separately.

### 3.10 Length, order and what is shared

- **The first tags weigh the most.** CLIP reads the text in chunks of about 75 tokens (roughly 40 to 50 short tags). Keep the population, framing, LEFT/RIGHT and the main tag in the **first chunk**. Gestures come last.
- **Shared or specific to one character?**
  - What concerns **both** goes in the ACTION: `both blushing`, `both sweating`, `eye contact`, the object they hold together.
  - What concerns **only one** character goes in **its** P1/P2 (see §4): its expression, the state of its clothes.
  - In the ACTION, a trait specific to one character would apply to both.
- **One pose per image.** Two main interaction tags at 1.3 (for example `hug` + `back-to-back`) contradict each other, and the model mixes the two bodies.

---

## 4. P1 / P2 — appearance (identity + anchors)

The dynamic mask finds each character by reading its **anchors**. These are tags extracted automatically from its appearance prompt. A tag only becomes an anchor if it is **unique to that character**: its words must not appear in the other character's prompt.

**Write P1/P2 as comma-separated tags**, each trait in its own tag:

| Category (priority order) | Examples |
|---|---|
| species | `long white floppy ears`, `long swept-back horns`, `fox tail`, `angel wings`, `halo`, `antennae`, `fins` |
| hair | `deep dark pink hair`, `vivid red streaks`, `short black pixie cut`, `platinum bob`, `twin drills`, `ahoge`, `sidelocks` |
| eyes | `white eyes`, `muted crimson eyes`, `heterochromia`, `slit pupils`, `tsurime` |
| skin | `pale skin`, `dark-skinned`, `freckles`, `whisker markings`, `mole under eye`, `tattoos`, `scales` |
| outfit | `oversized cream sweater`, `obsidian circlet` |
| character type | `1girl`, `fox girl`, `elf`, `android`, `1boy`, `tomboy`… (anchor only if it differs from the other) |
| body shape / parts | `petite`, `curvy`, `muscular`, `thick thighs`, `wide hips`, `red lips`, `sharp teeth`, `black nails` |

**Anchor rules:**
1. **Each character's own colors.** If P2 has "purple hair", "purple eyes" stops working as a P1 anchor: the word "purple" would light both.
2. **Keep `1girl` in each character**, not `2girls`. It helps the model and does no harm.
3. **No pose or gesture in P1/P2.** "spread legs", "crossed arms", "hands on hips" belong to the ACTION, and the extractor ignores them.
4. **A placement tag is cut at the preposition:** "circlet between the horns" → `circlet` (outfit), "hair like water" → `hair`.
5. **Synonyms** count as one word: butt = ass, stomach = belly, fingernails = nails…
6. **Two characters with no unique trait** (same hair color, nothing else) → no anchor → the image stays on the **static split** 50/50, on purpose.
7. **Keep a validated identity verbatim.** When you reuse a character, only change the part you are targeting (outfit, state of the moment); every other tag is an anchor the engine relies on.

### 4.1 Order inside a character, and what changes with the scene

**Recommended order** (the first tags weigh the most):

```
[type: 1girl / 1boy, adult], [species: ears, horns, tail, wings], [hair: color + style],
[eyes], [skin + marks], [body shape + chest], [outfit], [state of the moment]
```

**What changes with the scene but stays specific to one character** goes at the **end** of its P1/P2. These are not identity anchors, but they need to stay in its zone:
- **expression:** `smile`, `blush`, `closed eyes`, `open mouth`, `tears`, `embarrassed`;
- **state of clothes:** `barefoot`, `rolled-up sleeves`, `loose hair`, `hood up`, `wet clothes`;
- **marks of the moment:** `sweat`, `wet hair`, `messy hair`, `lipstick mark`.

**Weights:** 1.1 to 1.4 on 2 or 3 key traits (like `(long white floppy ears:1.3)`), no more. Beyond that, the character spills over onto the other.

**Length:** 15 to 35 tags. Beyond that, the last tags (outfit, state of the moment) carry little weight.

---

## 5. SFW negative

```
worst quality, low quality, normal quality, score_1, score_2, score_3, artist name, bad anatomy, bad hands,
extra fingers, missing fingers, fused fingers, extra toes, missing toes, extra limbs, malformed limbs, blurry,
lowres, jpeg artifacts, uncanny, revealing clothing, cleavage, exposed skin,
inset, cutout, x-ray, cross-section, speech bubble, thought bubble, spoken heart, poster \(object\),
painting \(object\), picture frame
```

- The second line of tags (anti-inset, "neg2") reduces insets without removing them 100%. The real cause of an inset is often an impossible view requested in the ACTION.
- **Never add** `fused bodies, duplicate person`: these tags hide the mask's real failures.

**Possible additions** ○ (Danbooru tags, to test on 10 images if a defect keeps coming back):
- insets: `chibi inset`, `projected inset`, `split screen`, `multiple views`;
- borders: `border`, `letterboxed`;
- text: `watermark`, `signature`, `english text`;
- color: `monochrome`, `greyscale`;
- empty background: `simple background`.

Add only one group at a time, to know which one has an effect.

---

## 6. Writing triggers and weights

**LoRA triggers, once, in the MAIN prefix:**

| Trigger | LoRA | Strength |
|---|---|---|
| `<style trigger>` | a style LoRA | 0.85 |
| `addmicrodetails` | AddMicroDetails (Illustrious) | 0.25 |
| `scnr` | Scenery Enhancer (Illustrious) | 0.30 |

Never in P1/P2 or the ACTION: a trigger in P1 would only apply to P1's zone.

**Weight syntax:**
- **Weight:** `(tag:1.3)`. The parentheses are required: `muted crimson eyes:1.3` without parentheses is read as text, not as a weight.
- **Recommended ranges:** 1.1–1.4 in general, 1.3 for the main pose tag, 1.2 for the secondary one.
- **Parentheses inside a Danbooru tag:** escape them, `poster \(object\)`. Otherwise they are read as a weight.
- **Separator:** one comma per tag. Line breaks count as commas for the anchor extractor.

---

## 7. SFW checklist

- [ ] MAIN = background only: dense scenery, no person, statue, mirror or screen.
- [ ] LoRA triggers in the MAIN prefix, once.
- [ ] ACTION: `2girls, 2 adult characters, duo, (fully clothed:1.2)`, framing, `(both faces visible:1.2)`, `heads separated`.
- [ ] ACTION: `the [P2 attr] on the LEFT and the [P1 attr] on the RIGHT`, one attribute each.
- [ ] ACTION: one pose tag `(…:1.3)`, and where each face is.
- [ ] No impossible view, no negation, no fusion prose.
- [ ] P1/P2: appearance only, separate tags, at least one unique trait each (species, hair, eyes).
- [ ] SFW negative + neg2, without `fused bodies`.

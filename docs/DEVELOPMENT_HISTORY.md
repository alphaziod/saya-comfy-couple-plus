# Development history and experiments

The README describes the current version. This file keeps what was tried, what was measured and why things
were kept or dropped. It is here for the curious and for future me.

## The starting point

Classic regional prompting (attention couple, conditioning combine/concat, masked conditionings) made MAIN, P1
and P2 compete for the same cross-attention. Strong characters meant a simplified background, missing MAIN
objects and lost colours; a stronger MAIN degraded the characters or merged them.

## Approaches tried and dropped

- **Simple weights / strengths** on MAIN vs persons: moves the problem around, does not fix the interaction.
- **Time scheduling** (persons early, scene late, and the reverse): unstable identities, scene still dominated.
- **Mask variants** (soft, dynamic, per block, block selectors): no consistent gain, harder to reason about.
- **Conditioning mixes / ConditioningConcat / routing variants**: same competition, different shape.
- **Shared-softmax variants of the core**: rejected during the core design.

## The core design that stayed: `main_locked_delta`

MAIN is computed natively; P1/P2 get their own attention on conditional rows only; inside each mask the person
contributes a delta from MAIN, with the component that would cancel MAIN removed (locked orthogonal to MAIN);
unconditional rows stay pure MAIN. Everything that could silently bypass this (other attn2 hooks, attention
overrides) raises an error instead (fail-closed).

## The "ghost MAIN" dual stream (rejected)

A second, parallel native MAIN trajectory ("ghost") steered the anchored stream, with an extra attn1 region
confinement ("R2"). On the reference seed it gave MAIN its colour back (saturation 60.7 / colourfulness 31.6 vs
53.8 / 24.6 for the single stream) but **P1's identity collapsed** (ears, tail, clothing, hair, eyes lost) and
the two characters' identities drifted towards each other. A control run (ghost on, R2 off) showed the same
loss, so the ghost itself was the cause. It also cost about +38 % sampling time. Dropped completely; the code
went back to the single-stream `main_locked_delta`, verified byte-identical to the pre-ghost version.

## The PERSON gain

With the interaction clean, the remaining question was magnitude: `OUT = MAIN + g × (D1_locked + D2_locked)`.

- **g = 0.50** (5 seeds, simple scene): saturation +8.5, colourfulness +3.0, richer rooms, and in that scene the
  two characters even separated better. With a dense background prompt it freed the background strongly but
  **P1 lost her rabbit ears on 3 of 5 seeds**, and clothes started leaking between characters.
- **Sweep 0.68 → 0.83, 10 pure-RNG images each** (80 images, a new random seed per image):
  - 0.68–0.70: MAIN strong, P1 too weak (ears on 5–6/10);
  - 0.73–0.78: plateau, the three sources cooperate (ears 8–9/10, P2 recognisable 7–9/10);
  - 0.80: P1 dominates P2 (P2 eye colour often lost);
  - 0.83–0.85: the persons take the frame back, the background recedes.
- **g = 0.78 on 10 themed environments** (café, beach, winter market, library, summer festival, rooftop, sky
  library, steampunk airship, cyberpunk ramen stall, enchanted forest; 3 RNG images each): structure held on
  30/30 images, the background carried each theme; weak spots were secondary actions (sand castle, fox spirit)
  and very specific props.

0.78 was frozen as the recommended value; 1.0 stays the neutral reference (bit-identical to the pre-gain code).

Evaluation rules that emerged: judge **harmony** (are MAIN, P1 and P2 all clearly expressed?) rather than a
defect count; tolerate local attribution swaps (ears / eye colour on the wrong character) and ambiguous hidden
limbs; judge character details only on close shots; a first pass may have 2–3 weaknesses that later passes fix.

## Small things found while preparing the release

- A historical test expected MAIN/person region weights 0.6 / 0.4 while the code uses 0.65 / 0.35 on purpose;
  the test now reads the value from the code (`DEFAULT_ATTENTION_PARAMS`).
- Tests had hard-coded local paths and needed the maintainer's upscale model; they are now portable and skip
  what the machine does not have.
- The pack registered a `SIGUSR1` debug handler that does not exist on Windows; now guarded.
- An old saved workflow still carried a removed widget value (`person_until = 0.5`) at the position of the new
  `dual_attention_enabled` boolean; ComfyUI coerces 0.5 to True. The demo workflow sets it explicitly.
- The two "verbatim" vendored ComfyUI-ppm files had in fact been modified (per-tile mask cropping); they now
  say so.

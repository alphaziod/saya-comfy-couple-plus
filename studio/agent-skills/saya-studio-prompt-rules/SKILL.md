---
name: saya-studio-prompt-rules
description: House rules for prompts in Saya Studio — character sheets (identity prompt P1 + a separate genital-anatomy block), variants with backgrounds, casting actions, background prompts and test discipline. Use with the Danbooru prompt skills whenever you write or change a character, a background, an action or a test prompt for this project.
---

# Saya Studio prompt rules

Answer in the user's language. These rules come from the maintainer's validated tests; they take
precedence over generic prompt advice. Never change the user's existing characters or prompts to
make a test pass: write your own test prompts and modify only the field under test.

All characters are adults. Refuse any request where a character reads as a minor; a render that
looks younger than intended is a one-off drift: discard it and use a new seed.

## Character sheet

- A sheet is **P1 + anatomy**. A variant is **P1 + anatomy + one background (BG)**. A sheet never
  contains an action, a pose, a second person (P2), quality tags or a negative prompt.
- **P1 (identity):** subject count, adult age descriptors, hair colour and cut (one weighted tag),
  eyes, facial features, skin tone, build, chest. Clothing only when it is a permanent part of the
  character; otherwise the outfit comes from the generation, not the sheet.
- **Anatomy block:** describes the external male genitalia only, in the clinical order the engine
  uses — penile size (tiny / small / medium), penile state (erect or flaccid), testicular size
  matching the penile size, scrotum, and pubic hair whose colour matches the scalp hair. Breasts,
  nipples and the rest of the body are **not** anatomy: they belong to P1. A character shown with a
  device or clothing over the genital area keeps her normal penile size in the sheet; the device or
  garment belongs to the action.

## Actions (casting prompts)

- Danbooru tags in the chronological order of the action; one tag per object, stated once;
  emphasis weight on the action itself, not on secondary details.
- In static poses give the hands something to do; state the wanted state positively instead of
  negating its opposite.
- One concept LoRA per prompt, starting from the creator's own short trigger tags.
- Avoid `front view` except for embraces or point-of-view shots: three-quarter and side views
  render objects and backgrounds better.
- New actions go through Casting → Drafts (an isolated copy of the engine) and only reach the real
  engine after the user's explicit approval.

## Backgrounds

- Start with `indoors` or `outdoors`, then one weighted place `(…:1.2)`, then ten or more concrete
  tags (objects, materials, time of day, light, weather, colours, depth of field). No people, no
  school settings, nothing a character could pick up when the action uses her hands.

## Settings and tests

- Sampler, steps, CFG, scheduler, resolution and LoRA weights are the user's: ask which workflow
  holds their good settings, read them there, never modify that workflow.
- Fresh random seed for every real test; one image per variant; stop at the first defect.
- The user launches GPU tests and judges the images; agents read the PNG metadata, not the pixels.

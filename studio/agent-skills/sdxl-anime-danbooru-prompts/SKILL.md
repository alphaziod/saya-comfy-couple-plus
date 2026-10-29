---
name: sdxl-anime-danbooru-prompts
description: Write and fix Danbooru-tag prompts for SDXL anime checkpoints (Illustrious, NoobAI eps/v-pred, Pony, Animagine) in ComfyUI or Saya Studio. Use when the user asks for a prompt, a negative, or why a render ignores a tag on one of these models.
---

# SDXL anime prompts (Danbooru tags)

Answer in the user's language. Default to SFW; adult content only for clearly adult characters,
never for anything that reads as a minor. Read the exact checkpoint's model card first; it wins.

**Sampler, steps, CFG, scheduler and resolution are the user's choice.** Machines and setups react
differently and there is no real convention: never prescribe them. Ask the user which workflow holds
their good settings, read them from it, and never modify that workflow.

## 1. Identify the family

| Family | Recognise it | Quality prefix (positive) | Notes |
|---|---|---|---|
| Illustrious XL (and merges) | `illustrious` in the name, most anime SDXL merges since 2024 | `masterpiece, best quality, amazing quality, very aesthetic, absurdres` | Tags with spaces |
| NoobAI-XL eps | `noobai`, `eps` | `masterpiece, best quality, newest, absurdres, highres` | Illustrious base; `very awa` exists on some versions |
| NoobAI-XL v-pred | `vpred`, `v-pred` | same as NoobAI | Must be loaded in v-prediction mode; washed-out or burnt images usually mean the wrong prediction mode |
| Pony Diffusion V6 | `pony` | `score_9, score_8_up, score_7_up` + `source_anime` | `rating_safe/questionable/explicit` |
| Animagine XL | `animagine` | `masterpiece, best quality, very aesthetic, absurdres` | Character + series tags strongest |

## 2. Prompt order (positive)

`[quality] , [count: 1girl / 1boy / 2girls / 1futanari, solo] , [character (series)] , [artist] ,
[appearance: hair colour + cut, eyes, face, body] , [outfit or nude] , [action, in the order it
happens] , [pose and camera] , [background] , [light / mood]`

- Real Danbooru tags, lowercase, spaces instead of underscores (`long hair`, not `long_hair`).
  Parentheses in Danbooru names must be escaped, or they become weights:
  `tifa lockhart \(final fantasy\)`, or write `tifa lockhart, final fantasy`.
- Weights `(tag:1.1)` to `(tag:1.3)`; above ~1.4 SDXL burns. Weight the act or the key object, not
  every tag.
- One object = one tag, said once. Repeating `dildo` or putting it in the negative gives two
  objects or removes it.
- Complex actions: simple tags in chronological order (place → entry → act → depth), not sentences.
- Say the state you want in the positive (`closed mouth`, `flaccid penis`); do not negate its
  opposite.
- Camera: prefer `from side`, `three-quarter view`, `from above/below`; `front view` flattens the
  scene and the background — use it only for hugs/POV.
- Hands busy (`holding …`, `hands on own thighs`) stop invented partners in waiting poses.

## 3. Negative

Short: `worst quality, low quality, bad anatomy, bad hands, extra digits, fewer digits, text,
watermark, signature, blurry`. Add family specifics (Pony: `score_4, score_5, score_6`). Do not add
the inverse of a positive tag. Add a specific defect only after you saw it.

## 4. LoRA

Start from the creator's own short tags and trigger (`.metadata.json` of LoRA Manager); one concept
LoRA at a time. The LoRA weight, like every sampling setting, is the user's to set.

## 5. Debug a render

1. Read the PNG metadata (prompt actually sent, seed, checkpoint, LoRAs) before blaming the model.
2. Tag ignored → is it a real Danbooru tag (ask the user for the wiki page if unsure; anonymous
   API access is often blocked)? Weight +0.1, move it earlier.
3. Model drifts somewhere → remove the tag pushing it there, state the wanted state positively.
4. Change one thing per test, fresh seed each time, one image per variant.

# Saya Comfy Couple+

Saya Comfy Couple+ is a modified Comfy Couple node for ComfyUI.

It is made for cleaner solo, duo, and dual-character workflows where you want to keep:

* the main scene prompt
* Person 1 identity prompt
* Person 2 identity prompt
* negative prompt
* automasks
* IPAdapter routing
* detailer routing

separated and easier to control.

The goal is simple:

```text
MAIN = shared scene, composition, action, mood, background
PERSON 1 = first character identity
PERSON 2 = second character identity
NEGATIVE = shared negative prompt
```

Instead of forcing everything into two regional prompts, Saya Comfy Couple+ gives you cleaner routing and stronger prompt reading.

## Why this fork exists

The original Comfy Couple node works well for two-character regional prompting, but its structure is limited:

```text
positive_1
positive_2
negative
```

That means the main scene prompt, character identity, IPAdapter logic, and detailer routing can easily become mixed together.

Saya Comfy Couple+ changes the structure to:

```text
main_positive
person_1_positive
person_2_positive
negative
```

This makes the workflow much clearer.

You can keep global scene logic in `main_positive`, then keep character-specific identity inside `person_1_positive` and `person_2_positive`.

## Main internal logic

Saya Comfy Couple+ does not simply send `main_positive`, `person_1_positive`, and `person_2_positive` as weak separated conditions.

Instead, it builds stronger regional contexts internally:

```text
Person 1 region = main_positive + person_1_positive
Person 2 region = main_positive + person_2_positive
```

This means each region receives both:

* the shared scene context
* the correct character identity

This usually gives stronger prompt reading than keeping the main prompt and character prompt as separate attention conditions.

## Inputs

### model

The model to patch with Comfy Couple attention logic.

Connect your checkpoint or LoRA-loaded model here.

### main_positive

The shared positive prompt.

Use this for global image logic:

* number of characters
* scene
* pose
* framing
* mood
* lighting
* background
* shared action
* global style

Examples:

```text
solo, one character, on bed, intimate framing, modern gamer bedroom
```

or:

```text
two characters, sitting together, bedroom scene, soft lighting, medium shot
```

### person_1_positive

Prompt for Person 1.

Use this for the first character identity:

* face
* hair
* eyes
* ears
* body type
* outfit
* accessories
* character-specific details

### person_2_positive

Prompt for Person 2.

Use this for the second character identity.

For solo generation, this can be routed to an empty or disabled conditioning depending on your workflow.

### negative

Shared negative conditioning.

Use your normal negative prompt here.

### orientation

Controls the mask split direction.

Available values:

```text
horizontal
vertical
```

### center

Controls where the split happens.

Examples:

```text
0.5 = centered split
0.4 = one side smaller, the other larger
0.6 = opposite balance
```

### width / height

Canvas size used for the internal automasks.

These should match the generation canvas or the canvas used by your workflow.

## Outputs

### model

The patched model.

Connect this to the main sampler model input.

### full_positive

Main generation positive conditioning.

This is the output you normally connect to the main sampler positive input.

Internally, it contains:

```text
Person 1 region = main_positive + person_1_positive
Person 2 region = main_positive + person_2_positive
```

### negative

Negative conditioning passthrough.

Connect this to the main sampler negative input and to other nodes that need the same negative conditioning.

### main_positive

Raw main prompt passthrough.

Useful for debug, extra routing, or workflow logic.

### person_1_positive

Raw Person 1 prompt passthrough.

Useful for Person 1 detailers, debug routing, or identity-specific workflow branches.

### person_2_positive

Raw Person 2 prompt passthrough.

Useful for Person 2 detailers, debug routing, or identity-specific workflow branches.

### duo_positive

Person 1 + Person 2 conditioning without the main scene prompt.

Recommended use:

```text
duo_positive -> detailer positive
```

This is useful because detailers often should focus on character identity without being influenced too much by the full scene prompt, background prompt, or lighting prompt.

### mask_positive_1

Automask for Person 1.

Recommended use:

```text
mask_positive_1 -> IPAdapter Person 1 attn_mask
```

### mask_positive_2

Automask for Person 2.

Recommended use:

```text
mask_positive_2 -> IPAdapter Person 2 attn_mask
```

## Recommended wiring

### Main generation

```text
Checkpoint / LoRA model -> model

main prompt conditioning -> main_positive
Person 1 conditioning -> person_1_positive
Person 2 conditioning -> person_2_positive
negative conditioning -> negative

Saya Comfy Couple+ model -> sampler model
Saya Comfy Couple+ full_positive -> sampler positive
Saya Comfy Couple+ negative -> sampler negative
```

### IPAdapter

```text
Saya Comfy Couple+ mask_positive_1 -> IPAdapter Person 1 attn_mask
Saya Comfy Couple+ mask_positive_2 -> IPAdapter Person 2 attn_mask
```

This lets each IPAdapter reference affect its own region.

### Detailers

Recommended simple setup:

```text
Saya Comfy Couple+ duo_positive -> detailer positive
Saya Comfy Couple+ negative -> detailer negative
```

This gives detailers access to both character identity prompts without making them read the full scene prompt.

This is useful when you do not want to split detailers into separate Person 1 and Person 2 branches, because splitting detailers can increase generation time.

## Solo generation

Saya Comfy Couple+ can also work well for solo generation.

Recommended solo setup:

```text
main_positive = solo, one character, scene, pose, framing, background
person_1_positive = character identity
person_2_positive = empty or disabled conditioning
negative = normal negative prompt
```

The node still builds:

```text
Person 1 region = main_positive + person_1_positive
Person 2 region = main_positive + person_2_positive
```

If `person_2_positive` is empty and `main_positive` clearly says `solo`, the workflow can behave like a normal solo generation setup while still keeping the clean routing structure.

## Duo generation

Recommended duo setup:

```text
main_positive = two characters, shared scene, pose, action, background
person_1_positive = first character identity
person_2_positive = second character identity
negative = normal negative prompt
```

The node builds:

```text
Person 1 region = main_positive + person_1_positive
Person 2 region = main_positive + person_2_positive
```

This keeps the global scene shared while letting each character keep its own identity.

## Prompt organization advice

Use `main_positive` for shared image logic.

Good for `main_positive`:

```text
two characters, bedroom scene, sitting together, soft lighting, medium shot
```

Good for `person_1_positive`:

```text
short blue hair, white eyes, rabbit ears, petite body
```

Good for `person_2_positive`:

```text
long pink hair, red eyes, horns, taller body
```

Avoid putting character-specific identity details into `main_positive` unless both characters should share them.

For example, do not put this in `main_positive`:

```text
blue hair, white eyes
```

unless both characters should have blue hair and white eyes.

## Difference from original Comfy Couple

Original Comfy Couple:

```text
positive_1
positive_2
negative
```

Saya Comfy Couple+:

```text
main_positive
person_1_positive
person_2_positive
negative
```

Extra routing outputs:

```text
main_positive
person_1_positive
person_2_positive
duo_positive
mask_positive_1
mask_positive_2
```

Main internal improvement:

```text
Person 1 region = main_positive + person_1_positive
Person 2 region = main_positive + person_2_positive
```

This makes the prompt structure easier to understand and often improves prompt reading strength.

## Token length safety patch

This fork also keeps a safety patch for different encoded conditioning lengths.

When regional prompts have different token lengths, shorter context tensors are padded before concatenation.

This helps avoid tensor size mismatch errors when one character prompt is longer than the other.

This does not remove the CLIP token limit. It only makes the internal attention logic safer when the encoded conditioning lengths differ.

## Notes

Saya Comfy Couple+ is mainly useful for advanced workflows using:

* two characters
* solo or duo prompt routing
* regional attention control
* IPAdapter with attn_mask
* detailers
* separated character identity prompts
* reusable main / character prompt blocks

It is not meant to replace every regional prompting method.

It is a cleaner Comfy Couple fork for users who want stronger routing control between scene prompt, character prompts, automasks, IPAdapter, and detailers.

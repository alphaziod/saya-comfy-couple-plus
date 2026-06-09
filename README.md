# Saya Comfy Couple+

Saya Comfy Couple+ is a modified Comfy Couple node for ComfyUI.

It is made for cleaner solo, duo, and dual-character workflows where you want better control over:

* shared scene prompts
* Person 1 identity prompts
* Person 2 identity prompts
* negative prompts
* automatic region masks
* IPAdapter `attn_mask` routing
* detailer prompt routing

The main goal is simple:

```text
MAIN = shared scene, composition, pose, action, mood, background
PERSON 1 = first character identity
PERSON 2 = second character identity
NEGATIVE = shared negative prompt
```

Instead of forcing scene context and character identity into only two regional prompts, Saya Comfy Couple+ gives you cleaner prompt separation and stronger routing.

## Why this fork exists

The original Comfy Couple node is useful for two-character regional prompting, but its prompt structure is limited:

```text
positive_1
positive_2
negative
```

That often forces users to mix too much logic together:

```text
scene + character identity + regional prompt + detailer logic
```

Saya Comfy Couple+ changes the node structure to:

```text
main_positive
person_1_positive
person_2_positive
negative
```

This makes the workflow easier to understand and easier to control.

You can keep the global scene in `main_positive`, then keep each character identity in its own input.

## What the node does

Saya Comfy Couple+ builds two regional contexts internally.

For Person 1:

```text
Person 1 region = main_positive + person_1_positive
```

For Person 2:

```text
Person 2 region = main_positive + person_2_positive
```

This means each region receives:

* the shared scene context
* the correct character identity

This usually gives stronger prompt reading than sending the main prompt and character prompts as weak separated conditions.

The node also exposes useful routing outputs:

```text
full_positive
main_positive
person_1_positive
person_2_positive
duo_positive
mask_positive_1
mask_positive_2
```

## Main features

* Main / Person 1 / Person 2 prompt routing
* Stronger regional context using `main + person` prompt concatenation
* `full_positive` output for the main sampler
* `duo_positive` output for detailers
* `mask_positive_1` and `mask_positive_2` outputs for automask routing
* IPAdapter-friendly `attn_mask` workflow support
* Solo and duo workflow support
* Safer handling of different encoded CLIP context lengths

## Installation

Install it like a normal ComfyUI custom node.

Clone the repository inside your ComfyUI `custom_nodes` folder, then restart ComfyUI.

Repository:

```text
https://github.com/alphaziod/saya-comfy-couple-plus.git
```

## Windows installation

### Standard ComfyUI install

Open PowerShell.

Go to your ComfyUI `custom_nodes` folder:

```powershell
cd C:\ComfyUI\custom_nodes
```

Clone the node:

```powershell
git clone https://github.com/alphaziod/saya-comfy-couple-plus.git
```

Restart ComfyUI.

### ComfyUI portable

If you use ComfyUI portable, the path is usually:

```powershell
cd C:\ComfyUI_windows_portable\ComfyUI\custom_nodes
```

Clone the node:

```powershell
git clone https://github.com/alphaziod/saya-comfy-couple-plus.git
```

Restart ComfyUI portable.

## macOS installation

Open Terminal.

Go to your ComfyUI `custom_nodes` folder:

```bash
cd ~/ComfyUI/custom_nodes
```

Clone the node:

```bash
git clone https://github.com/alphaziod/saya-comfy-couple-plus.git
```

Restart ComfyUI.

If your ComfyUI folder is somewhere else, use your own path:

```bash
cd /path/to/ComfyUI/custom_nodes
```

## Linux installation

Open a terminal.

Go to your ComfyUI `custom_nodes` folder:

```bash
cd ~/ComfyUI/custom_nodes
```

Clone the node:

```bash
git clone https://github.com/alphaziod/saya-comfy-couple-plus.git
```

Restart ComfyUI.

If your ComfyUI folder is somewhere else, use your own path:

```bash
cd /path/to/ComfyUI/custom_nodes
```

## Arch Linux installation

Install Git if needed:

```bash
sudo pacman -S git
```

Clone the node:

```bash
cd ~/ComfyUI/custom_nodes
git clone https://github.com/alphaziod/saya-comfy-couple-plus.git
```

Restart ComfyUI.

## Fedora installation

Install Git if needed:

```bash
sudo dnf install git
```

Clone the node:

```bash
cd ~/ComfyUI/custom_nodes
git clone https://github.com/alphaziod/saya-comfy-couple-plus.git
```

Restart ComfyUI.

## Debian / Ubuntu installation

Install Git if needed:

```bash
sudo apt update
sudo apt install git
```

Clone the node:

```bash
cd ~/ComfyUI/custom_nodes
git clone https://github.com/alphaziod/saya-comfy-couple-plus.git
```

Restart ComfyUI.

## NixOS installation

If Git is already available:

```bash
cd ~/ComfyUI/custom_nodes
git clone https://github.com/alphaziod/saya-comfy-couple-plus.git
```

If Git is not available, open a temporary shell with Git:

```bash
nix shell nixpkgs#git
```

Then clone:

```bash
cd ~/ComfyUI/custom_nodes
git clone https://github.com/alphaziod/saya-comfy-couple-plus.git
```

Restart ComfyUI.

## Updating

To update the node later:

```bash
cd ~/ComfyUI/custom_nodes/saya-comfy-couple-plus
git pull
```

Then restart ComfyUI.

On Windows portable, the path may be:

```powershell
cd C:\ComfyUI_windows_portable\ComfyUI\custom_nodes\saya-comfy-couple-plus
git pull
```

Then restart ComfyUI portable.

## Uninstalling

Remove the folder from `custom_nodes`.

Linux / macOS:

```bash
rm -rf ~/ComfyUI/custom_nodes/saya-comfy-couple-plus
```

Windows PowerShell:

```powershell
Remove-Item -Recurse -Force C:\ComfyUI\custom_nodes\saya-comfy-couple-plus
```

Then restart ComfyUI.

## Inputs

### model

The model to patch with Comfy Couple attention logic.

Connect your checkpoint-loaded or LoRA-loaded model here.

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

Useful for debug, extra routing, or custom workflow logic.

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

## Troubleshooting

### The node does not appear in ComfyUI

Check that the folder is inside:

```text
ComfyUI/custom_nodes/
```

The folder should look like this:

```text
ComfyUI/custom_nodes/saya-comfy-couple-plus/
```

Then restart ComfyUI.

### The old Comfy Couple node still appears

Restart ComfyUI fully.

If your workflow already had the old node loaded, you may need to delete and recreate the node inside the workflow so ComfyUI refreshes the ports.

### The masks seem inverted

Check your `orientation` and `center` values.

Also check that:

```text
mask_positive_1 -> IPAdapter Person 1 attn_mask
mask_positive_2 -> IPAdapter Person 2 attn_mask
```

### The character prompt feels weak

Make sure your character identity is in `person_1_positive` or `person_2_positive`, not only in `main_positive`.

The node internally builds:

```text
main_positive + person_1_positive
main_positive + person_2_positive
```

so both the main prompt and the person prompt matter.

### Detailers read too much scene context

Use:

```text
duo_positive -> detailer positive
```

instead of:

```text
full_positive -> detailer positive
```

`duo_positive` contains Person 1 + Person 2 without the main scene prompt.

## Notes

Saya Comfy Couple+ is mainly useful for advanced workflows using:

* two characters
* solo or duo prompt routing
* regional attention control
* IPAdapter with `attn_mask`
* detailers
* separated character identity prompts
* reusable main / character prompt blocks

It is not meant to replace every regional prompting method.

It is a cleaner Comfy Couple fork for users who want stronger routing control between scene prompt, character prompts, automasks, IPAdapter, and detailers.

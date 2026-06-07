# ComfyCouple IPAdapter AutoMask Patch

A patched version of the ComfyUI Comfy Couple node designed for dual-character and dual-persona workflows.

This patch improves Comfy Couple so it can work more cleanly with IPAdapter, expose automatic region masks, and handle prompts with different token lengths without crashing.

## What it does

This patch adds three main improvements:

1. **IPAdapter compatibility**
   - Makes Comfy Couple attention patches compatible with IPAdapter attention patches.
   - Allows Comfy Couple and IPAdapter to be used together in the same model chain.

2. **Automatic mask outputs**
   - Adds `mask_positive_1` and `mask_positive_2` outputs to the Comfy Couple node.
   - These masks can be connected directly to IPAdapter `attn_mask` inputs.
   - This makes it possible to guide different characters with different IPAdapter references without manually painting masks.

3. **Safer prompt length handling**
   - Adds padding for conditioning tensors when positive prompts have different token lengths.
   - Prevents crashes caused by mismatched prompt sizes when one regional prompt is longer than the other.

## Main use case

This patch is useful for workflows like:

```text
Main prompt = scene, framing, mood, lighting
Positive 1 = left character identity
Positive 2 = right character identity
Comfy Couple = spatial separation
IPAdapter 1 = visual reference for character 1
IPAdapter 2 = visual reference for character 2
Auto masks = region control for each IPAdapter

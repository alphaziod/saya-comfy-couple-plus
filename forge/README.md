# Saya Comfy Couple - Forge Coarse Main 2.3

Version: `SAYA-FORGE-COARSE-MAIN-2.3`

Additive ComfyUI candidate. It does not replace or modify `comfy_saya_couple`.

## What changed

This starts from **Contact 2.1**, not Contact 2.2/2.2.1.

At detail cross-attention resolutions, routing is exactly Contact 2.1:

- P1 branch = `MAIN + P1`
- P2 branch = `MAIN + P2`
- MAIN-only branch exists only in the soft contact band
- self-attention is fully native

The background fix is isolated to **coarse attn2 resolutions only**. When the current
attention map has at most 1024 spatial tokens, MAIN receives 22% of the remaining
non-contact budget. At finer resolutions that extra global share is exactly zero.

The intent is to restore scene/background information early in denoising without
reintroducing a permanent MAIN branch at the resolutions where hair, anatomy and
other private details are resolved.

Internal constants (no workflow sockets):

- contact MAIN target: `0.75`
- contact width: `0.15`
- coarse MAIN share: `0.22`
- coarse threshold: `<= 1024 tokens`

## UI

No new sockets. Required visible controls remain:

- use_couple_attention
- orientation
- center
- transition
- mask_floor
- swap_person_positions

## Public masks

`mask_person_1` and `mask_person_2` are unchanged for detailers/IPAdapter.
The coarse MAIN blend exists only inside Forge attn2 output routing.

## Tests

- Contact 2.1 fine routing unchanged outside contact
- coarse-only MAIN share activates only below threshold
- no extra required workflow sockets
- node smoke test
- Forge `k == v` precondition remains hard failure
- with coarse mode disabled, attention output is numerically identical to upstream Forge

## Source / license

Attention routing math and mask sizing are adapted from `Haoming02/sd-forge-couple`,
GPL-3.0. The upstream LICENSE is bundled.


## Naturalize I/O extension

The MASTER node now has four optional CONDITIONING inputs:
`naturalize_main_positive`, `naturalize_person_1_positive`,
`naturalize_person_2_positive`, `naturalize_negative`.

It appends three outputs without changing any legacy slot index:
`naturalize_patched_model_main`, `naturalize_positive_final`,
`naturalize_negative`. Existing COPY nodes retain their original 11-output
contract. The Naturalize branch reuses the exact same Forge couple config/masks
and patches only the main model.

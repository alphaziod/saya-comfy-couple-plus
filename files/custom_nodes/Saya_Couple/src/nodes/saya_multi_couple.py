"""Phase 1 couple node and the shared MultiMaskCouple regional core.

MultiMaskCouple is an external pack. A fresh AttentionCouple instance per
patched model guarantees that the conditionings it captures (raw_positive /
raw_negative, read when the patch runs) are never shared between models.
"""

from __future__ import annotations

import functools
import logging
import re

import torch
from nodes import ConditioningCombine, ConditioningConcat, ConditioningSetMask

import comfy.ldm.modules.attention as attention

from .couple_imprint_v2 import DEFAULT_ATTENTION_PARAMS
from .ownership_anchors import discriminant_anchors, distinctive_anchor_words, hybrid_anchor_words, phrase_anatomy_anchors
from .saya_dual_attention import enable_dual_attention


LOGGER = logging.getLogger(__name__)

CLIP_COMMA = 267
# Generic "a person is here" anchor of the dynamic ownership: zones below it are background.
PERSON_ANCHOR = "woman"
# person_anchor = "auto": "woman", plus "man" when a P1/P2 prompt names a male character ("1boy", "adult man"...).
# Opt-in only: on 2 girl+boy pairs (GPU, 2026-10-05, 20_fable_audit/person_anchor_1005_1130) the historic "woman"
# anchor already owned the man's body (6-20 % of the frame) and "woman, man" brought nothing, so the default stays
# "woman" (bit-identical to the historic payload).
MALE_WORDS = frozenset({"man", "men", "boy", "boys", "male", "males", "guy", "guys", "dude", "dudes", "husband", "husbands",
                        "boyfriend", "boyfriends", "father", "dad", "gentleman", "gentlemen", "femboy", "femboys", "otoko",
                        "crossdresser"})  # adults only: no child words, ever


def person_anchor_text(person_anchor, *texts):
    """The generic person anchor: ``person_anchor`` verbatim, or for "auto" PERSON_ANCHOR plus "man" when any of
    the P1/P2 ``texts`` names a male character ("1boy" counts: digits are not letters)."""
    if person_anchor.strip().lower() != "auto":
        return person_anchor.strip()
    male = any(word in MALE_WORDS for text in texts if text for word in re.findall(r"[a-z]+", text.lower()))
    return f"{PERSON_ANCHOR}, man" if male else PERSON_ANCHOR


def _attention_grid_32(mask):
    """(height, width) of the /32 attention grid of a pixel mask: /8 to the latent, then the two SDXL
    downsamples, each rounded up like the UNet does (832x1216 -> 26x38)."""
    height, width = (int(v) for v in mask.shape[-2:])
    for _ in range(5):
        height, width = -(-height // 2), -(-width // 2)
    return height, width


def _warn_dynamic_never_starts(mask_1, anchors):
    """The engine falls back to the static split silently in two cases the node can see before sampling:
    no anchors (nothing to read the ownership from) and a frame whose /32 grid has more tokens than the
    engine's max_tokens (no block ever contributes evidence). Both are logged, never raised: the
    generation stays valid, only its ownership is the historic static split."""
    if anchors is None:
        LOGGER.warning("[Saya Couple] ownership=dynamic but no P1/P2 anchor could be derived (wire p1_text/p2_text, "
                       "or set p1_anchors and p2_anchors): Sampler 1 keeps the static split everywhere")
    grid = _attention_grid_32(mask_1)
    max_tokens = int(getattr(attention, "SAYA_DYNAMIC_DEFAULTS", {}).get("max_tokens", 1100))
    if grid[0] * grid[1] > max_tokens:
        LOGGER.warning("[Saya Couple] ownership=dynamic never starts at this size: the /32 attention grid %dx%d = %d "
                       "tokens exceeds the engine's max_tokens=%d, so Sampler 1 keeps the static split everywhere "
                       "(1536x832 and larger presets; 832x1216, 1024x1024, 1344x768 are fine)",
                       grid[0], grid[1], grid[0] * grid[1], max_tokens)


def _anchor(clip, text, keep=None):
    """Dynamic-ownership anchor: the encoded short prompt and the positions of its attended words (all its
    words, or only the tokens of ``keep``)."""
    tokens = clip.tokenize(text, return_word_ids=True)
    kept = None if keep is None else {t for w in keep for t, _, word in clip.tokenize(w, return_word_ids=True)["l"][0] if word > 0}
    positions = [i for i, (token, _, word) in enumerate(tokens["l"][0]) if word > 0 and token != CLIP_COMMA and (kept is None or token in kept)]
    if not positions:
        raise RuntimeError(f"SayaMultiCouple: anchor {text!r} has no word token")
    return clip.encode_from_tokens_scheduled(clip.tokenize(text))[0][0], positions


def _prompt_text(clip, prompt, cond):
    """The prompt string whose encoding by ``clip`` is exactly ``cond``, or None."""
    target = cond[0][0]
    texts = {v for node in (prompt or {}).values() for v in node.get("inputs", {}).values() if isinstance(v, str) and len(v) > 2}
    for text in texts:
        encoded = clip.encode_from_tokens_scheduled(clip.tokenize(text))[0][0]
        if encoded.shape == target.shape and torch.equal(encoded.to(target.device, target.dtype), target):
            return text
    return None


def _anchor_texts(clip, prompt, pos_1, pos_2, p1_text="", p2_text=""):
    """The P1 and P2 prompt texts: wired in explicitly (identity + anatomy concatenated in the graph), else found
    in the queued prompt by re-encoding its strings; None when a text cannot be found."""
    if p1_text.strip() and p2_text.strip():
        return p1_text, p2_text
    return _prompt_text(clip, prompt, pos_1), _prompt_text(clip, prompt, pos_2)


def _ownership_anchors(clip, prompt, pos_1, pos_2, p1_anchors, p2_anchors, anchor_tokens, p1_text="", p2_text="", texts=None):
    """(text, attended words or None) for P1 and P2: the debug/oracle fields when both are set, else derived
    from the P1/P2 prompts (``texts`` if already resolved); None when either person has no anchor."""
    if p1_anchors.strip() and p2_anchors.strip():
        return (p1_anchors, None), (p2_anchors, None)
    text_1, text_2 = texts if texts is not None else _anchor_texts(clip, prompt, pos_1, pos_2, p1_text, p2_text)
    if text_1 is None or text_2 is None:
        return None
    if anchor_tokens in ("distinctive", "hybrid"):
        extract = distinctive_anchor_words if anchor_tokens == "distinctive" else hybrid_anchor_words
        picks = extract(text_1, text_2), extract(text_2, text_1)
        if not picks[0] or not picks[1]:
            return None
        return tuple((", ".join(seg for seg, _ in p), [w for _, ws in p for w in ws]) for p in picks)
    extract = phrase_anatomy_anchors if anchor_tokens == "phrase_anatomy" else discriminant_anchors
    anchors_1, anchors_2 = extract(text_1, text_2), extract(text_2, text_1)
    if not anchors_1 or not anchors_2:
        return None
    return (", ".join(anchors_1), None), (", ".join(anchors_2), None)


def _masked_cond(cond, mask, strength):
    return ConditioningSetMask().append(cond, mask, "default", float(strength))[0]


def _on_real_grid(patch):
    """MultiMaskCouple sizes its masks from ``original_shape`` (the latent) by guessing the
    downsampling factor; SDXL rounds each downsample up, so at sizes that are not a multiple
    of 32 the guess fails and its fallback lays the mask out transposed (1584x2320: grid 73x50
    read as 50x73). Hand it the block's real activation grid instead: the guess is then exact
    at factor 1, and unchanged wherever it was already right."""
    @functools.wraps(patch)
    def on_real_grid(q, k, v, extra_options):
        return patch(q, k, v, {**extra_options, "original_shape": extra_options["activations_shape"]})
    return on_real_grid


MULTIMASK_HINT = "install the MultiMaskCouple custom node (custom_nodes/MultiMaskCouple); Solo does not need it"


def _multimask_couple():
    """MultiMaskCouple is an external pack: imported here, at use, so that its absence disables Couple with a
    clear error instead of taking the whole Saya pack down at ComfyUI start (Solo included)."""
    try:
        from custom_nodes.MultiMaskCouple.attention_couple import AttentionCouple
    except ImportError as error:
        raise RuntimeError(f"Saya Couple: MultiMaskCouple is missing ({error}); {MULTIMASK_HINT}") from error
    return AttentionCouple


def _attention_couple_patch(model, clip, positive, negative):
    # Fresh instance on every call: the prompts captured by the patch
    # stay private to this model, regardless of execution order.
    patched, positive, negative = _multimask_couple()().attention_couple(
        model=model,
        clip=clip,
        positive=positive,
        negative=negative,
        mode="Attention",
    )
    replace = patched.model_options["transformer_options"]["patches_replace"]["attn2"]
    for key, patch in replace.items():
        replace[key] = _on_real_grid(patch)
    return patched, positive, negative


def apply_multimask_couple(
    model,
    clip,
    mask_1,
    mask_2,
    pos_1,
    neg_1,
    pos_2,
    neg_2,
    strength_1,
    strength_2,
    base_weight,
    person_weight,
    main=None,
    couple_fn=_attention_couple_patch,
    scene=None,
    background_mask=None,
):
    """MultiMaskCouple regional attention on ``model``; returns the patched MODEL.

    Per region: MAIN masked at ``base_weight`` + the person masked at
    ``person_weight * strength``, coupled with MultiMaskCouple's
    ``AttentionCouple`` (``couple_fn`` is the test seam, same
    (model, clip, positive, negative) -> (model, positive, negative) contract).
    Used for Phase 1's MODEL_2 and the full-frame reconstruct passes
    (``couple_reconstruct.py``). The attn2 patch replaces cross-attention
    entirely, so callers keep MAIN as the sampler positive.

    ``background_mask`` (Sampler 1 map background, where mask_1 = mask_2 = 0): a third region,
    ``scene`` (MAIN without ACTION) alone, with the negative -- like Sampler 1's background.
    """
    pos_regions = []
    for pos, mask, strength in ((pos_1, mask_1, strength_1), (pos_2, mask_2, strength_2)):
        if main is not None:
            pos_regions += _masked_cond(main, mask, base_weight)
            strength = person_weight * strength
        pos_regions += _masked_cond(pos, mask, strength)
    neg_regions = ConditioningCombine().combine(
        _masked_cond(neg_1, mask_1, strength_1),
        _masked_cond(neg_2, mask_2, strength_2),
    )[0]
    if background_mask is not None:
        if scene is None:
            raise ValueError("apply_multimask_couple: background_mask needs the scene conditioning")
        pos_regions += _masked_cond(scene, background_mask, 1.0)
        neg_regions = ConditioningCombine().combine(neg_regions, _masked_cond(neg_1, background_mask, 1.0))[0]
    return couple_fn(model, clip, pos_regions, neg_regions)[0]


class SayaMultiCouple:
    """Phase 1 couple: MODEL_1 runs the core Saya dual attention, MODEL_2 the MultiMaskCouple patch.

    Solo: MAIN + pos_1 only, both models returned unpatched.
    """

    @classmethod
    def INPUT_TYPES(cls):
        return {
            "required": {
                "model_1": ("MODEL",),
                "clip": ("CLIP",),
                "mask_1": ("MASK",),
                "mask_2": ("MASK",),
                "pos_1": ("CONDITIONING",),
                "neg_1": ("CONDITIONING",),
                "pos_2": ("CONDITIONING",),
                "neg_2": ("CONDITIONING",),
                "strength_1": ("FLOAT", {"default": 1.0, "min": 0.0, "max": 2.0, "step": 0.05}),
                "strength_2": ("FLOAT", {"default": 1.0, "min": 0.0, "max": 2.0, "step": 0.05}),
            },
            "optional": {
                "model_2": ("MODEL",),
                "main": ("CONDITIONING", {"tooltip": "Required in Couple mode (base of the dual attention)."}),
                "action": ("CONDITIONING", {"tooltip":"Optional pose / act prompt, kept apart from MAIN (scene, framing, "
                                                        "style). Persons get MAIN + ACTION. With ownership dynamic the "
                                                        "background gets MAIN only, so the act is never drawn into the scenery."}),
                "solo": ("BOOLEAN", {
                    "default": False,
                    "tooltip": "OFF = Couple. ON = Solo: MAIN + pos_1 only, pos_2/neg_2/mask_2 "
                               "ignored, models returned unpatched.",
                }),
                "ownership": (["static_split", "dynamic"], {
                    "default": "static_split",
                    "tooltip": "static_split = mask_1/mask_2 as drawn (historic). dynamic = below "
                               "dynamic_start_sigma, each pixel's owner is read from the model's own "
                               "attention (identity anchors spread along each body by self-attention). "
                               "MODEL_1 only.",
                }),
                "dynamic_start_sigma": ("FLOAT", {
                    "default": 5.0, "min": 0.0, "max": 100.0, "step": 0.1,
                    "tooltip": "dynamic only: the static split drives every step with a higher sigma.",
                }),
                "background_main": ("BOOLEAN", {
                    "default": False,
                    "tooltip": "dynamic only: zones where the model sees no person (and no person owns them) get MAIN "
                               "only, so the P1/P2 prompts (acts, anatomy) are never drawn into the background.",
                }),
                "zone_fallback": ("BOOLEAN", {
                    "default": False,
                    "tooltip": "dynamic only: a person zone left to the static split takes the split side holding the "
                               "majority of its pixels as a whole, instead of being cut by the split line. Background, "
                               "contested and head anchor zones stay pixel-wise.",
                }),
                "anchor_tokens": (["phrase", "phrase_anatomy", "distinctive", "hybrid"], {
                    "default": "phrase",
                    "tooltip": "dynamic only. phrase = each anchor attends all words of its phrase (deep dark pink hair). "
                               "phrase_anatomy = phrase, plus the anatomy phrase whose words differ between the two persons "
                               "(small penis / medium penis), attended whole. "
                               "distinctive = only the words the other person does not have in the same category: a "
                               "noun both have (hair, penis) is dropped, sizes count (small penis vs medium penis). "
                               "hybrid = whole phrase when its noun is the person's own (ears vs horns), distinct words "
                               "only when the noun is shared (pink hair -> pink, small penis -> small).",
                }),
                "p1_anchors": ("STRING", {"default": "", "tooltip": "Debug / oracle, dynamic only: overrides the anchors derived from the P1/P2 prompts when both fields are set."}),
                "p2_anchors": ("STRING", {"default": "", "tooltip": "Debug / oracle, dynamic only: overrides the anchors derived from the P1/P2 prompts when both fields are set."}),
                "p1_text": ("STRING", {"forceInput": True, "tooltip": "dynamic only: the exact P1 text encoded in pos_1, when it is built in the graph "
                                                                     "(identity + anatomy concatenated). Anchors are read from it instead of being guessed."}),
                "p2_text": ("STRING", {"forceInput": True, "tooltip": "dynamic only: the exact P2 text encoded in pos_2 (see p1_text)."}),
                "person_anchor": ("STRING", {"default": PERSON_ANCHOR, "tooltip": "dynamic only: the generic 'a person is here' anchor "
                                                                            "(historic: woman; it already covers men). auto = woman, "
                                                                            "plus man when a P1/P2 text names a male character "
                                                                            "(experimental). Any other value is used as is."}),
                "quality": ("CONDITIONING", {"tooltip": "Optional quality tags + LoRA triggers, encoded alone. When wired, "
                                                         "MAIN must be the background only (SayaMainPrompt 'scene' output): "
                                                         "the positive reads QUALITY first and the background last. "
                                                         "Unwired = historic behaviour, bit-identical."}),
            },
            "hidden": {"prompt": "PROMPT"},
        }

    # COUPLE_RECIPE is appended last: saved workflows resolve outputs by slot index.
    RETURN_TYPES = ("MODEL", "MODEL", "CONDITIONING", "CONDITIONING", "SAYA_COUPLE_RECIPE")
    RETURN_NAMES = ("MODEL_1_PATCHED", "MODEL_2_PATCHED", "CONDITIONING", "NEGATIVE", "COUPLE_RECIPE")
    FUNCTION = "apply"
    CATEGORY = "saya/couple"
    DESCRIPTION = (
        "Couple: MODEL_1 gets the core Saya dual attention (positive = MAIN, P1/P2 and "
        "masks travel as its payload); MODEL_2 gets the MultiMaskCouple regional patch "
        "(None if model_2 is not connected). Solo: MAIN + P1, models unpatched. "
        "COUPLE_RECIPE lets SayaOwnershipMapCapture rebuild MODEL_2 on the Sampler 1 map."
    )

    def apply(
        self,
        model_1,
        clip,
        mask_1,
        mask_2,
        pos_1,
        neg_1,
        pos_2,
        neg_2,
        strength_1=1.0,
        strength_2=1.0,
        model_2=None,
        main=None,
        action=None,
        solo=False,
        ownership="static_split",
        dynamic_start_sigma=5.0,
        background_main=False,
        zone_fallback=False,
        anchor_tokens="phrase",
        p1_anchors="",
        p2_anchors="",
        p1_text="",
        p2_text="",
        person_anchor=PERSON_ANCHOR,
        prompt=None,
        quality=None,
    ):
        def _concat(parts):
            out = None
            for part in parts:
                if part is not None:
                    out = part if out is None else ConditioningConcat().concat(out, part)[0]
            return out

        if solo:
            # Solo = ONE conditioning MAIN ++ ACTION ++ P1 (token concat), neg_1; P2 is never read.
            # Same as the phases rebuilt from the imprint (one text "MAIN, ACTION, PERSON 1", encoded in 77-token
            # chunks). Never ConditioningCombine: it averages two separate predictions, so half of the guidance came
            # from MAIN + ACTION alone (scenery + pose, no person, no nudity): clothes, censorship, wrong person.
            # With QUALITY wired, MAIN is the background only: QUALITY ++ ACTION ++ P1 ++ MAIN (quality read
            # first, scenery read last).
            if quality is not None:
                return (model_1, model_2, _concat((quality, action, pos_1, main)), neg_1, None)
            return (model_1, model_2, _concat((main, action, pos_1)), neg_1, None)

        if quality is not None:
            # MAIN = background only: the sampler MAIN is QUALITY ++ ACTION ++ MAIN, the scene-only MAIN of the
            # background pixels QUALITY ++ MAIN (quality first, scenery last). P1 / P2 stay separate contexts.
            scene = _concat((quality, main))
            main = _concat((quality, action, main))
        else:
            scene = main
            if action is not None:
                main = ConditioningConcat().concat(main, action)[0]
        params = {}
        if ownership == "dynamic":
            params["start_sigma"] = float(dynamic_start_sigma)
            params["zone_fallback"] = bool(zone_fallback)
            params["background_main"] = bool(background_main)
            oracle = bool(p1_anchors.strip() and p2_anchors.strip())
            # The texts are needed for the anchors (unless oracle) and for the male detection of "auto": resolved
            # once; the prompt scan (CLIP re-encoding) is skipped when oracle anchors are set and no text is wired.
            texts = (None, None) if oracle and not ((p1_text or "").strip() and (p2_text or "").strip()) \
                else _anchor_texts(clip, prompt, pos_1, pos_2, p1_text or "", p2_text or "")
            anchors = _ownership_anchors(clip, prompt, pos_1, pos_2, p1_anchors, p2_anchors, anchor_tokens, texts=texts)
            _warn_dynamic_never_starts(mask_1, anchors)
            if anchors is not None:
                params["p1_anchor"] = _anchor(clip, *anchors[0])
                params["p2_anchor"] = _anchor(clip, *anchors[1])
                params["person_anchor"] = _anchor(clip, person_anchor_text(person_anchor or PERSON_ANCHOR, *texts))
            if action is not None:
                params["main_scene"] = scene[0][0]
                params["background_main"] = True
        model_1_patched = enable_dual_attention(model_1, main, pos_1, pos_2, mask_1, mask_2, ownership, params)
        model_2_patched = None
        if model_2 is not None:
            model_2_patched = apply_multimask_couple(
                model_2, clip, mask_1, mask_2, pos_1, neg_1, pos_2, neg_2,
                strength_1, strength_2,
                DEFAULT_ATTENTION_PARAMS["base_weight"], DEFAULT_ATTENTION_PARAMS["person_weight"],
                main=main, couple_fn=_attention_couple_patch,
            )
        recipe = None
        if model_2 is not None:
            # Everything MODEL_2's patch is built from, so it can be rebuilt on other masks after Sampler 1.
            recipe = {"model_2": model_2, "model_2_patched": model_2_patched, "clip": clip,
                      "pos_1": pos_1, "neg_1": neg_1, "pos_2": pos_2, "neg_2": neg_2,
                      "strength_1": strength_1, "strength_2": strength_2, "main": main, "scene": scene,
                      "dynamic": ownership == "dynamic"}
        return (model_1_patched, model_2_patched, main, neg_1, recipe)

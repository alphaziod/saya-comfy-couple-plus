"""ComfyUI node registration for this pack.

ComfyUI discovers custom nodes through two module-level dicts that the
package's ``__init__.py`` re-exports: ``NODE_CLASS_MAPPINGS`` (internal node
name -> node class) and ``NODE_DISPLAY_NAME_MAPPINGS`` (internal node name ->
label shown in the UI). A saved workflow references nodes by the
``NODE_CLASS_MAPPINGS`` key, so once a node ships that key must never change
or old workflows stop loading; the display name is free to change at any
time. Every node used anywhere in the pack must be imported here and added
to both dicts, or ComfyUI simply never sees it.
"""

from __future__ import annotations

from typing import Any

from .src.nodes.chroma_anchor import SayaChromaAnchor
from .src.nodes.naturalize_postprocess import SayaNaturalizePostProcess
from .src.nodes.hidream_lora import SayaHiDreamLoraLoader, SayaHiDreamLoraSettings
from .src.nodes.hidream_shadow_control import SayaHiDreamShadowControlMask
from .src.nodes.image_phases import (
    SayaImageGenerationReview,
    SayaImagePhaseCheckpointLoad,
    SayaImagePhaseCheckpointStop,
    SayaImagePhaseController,
    SayaImagePhase1Stop,
    SayaImagePhase2Load,
    SayaImagePhase2Stop,
    SayaImagePhase3Load,
    SayaImagePhase3Stop,
    SayaImagePhase4Load,
    SayaImagePhase4Stop,
    SayaImagePhase5Load,
    SayaImagePhase5Stop,
    SayaImagePhase6Load,
    SayaImagePhase6Stop,
    SayaLazyCheckpointLoader,
)
from .src.nodes.sampling_config import SayaKSamplerConfig
from .src.nodes.saya_resolution_scale import (
    SayaNear4KTargetCalculator,
    SayaResolutionScaleCalculator,
    SayaUpscalePresetModelLoader,
    SayaUpscaleTargetCalculator,
)
from .src.nodes.detailer import SayaDuoSegsDetail
from .src.nodes.latent_shape import SayaDuoLatentShape
from .src.nodes.ppm_masks import SayaPPMMasks
from .src.nodes.saya_attention_couple import SayaAttentionCouplePPM
from .src.nodes.saya_multi_couple import SayaMultiCouple
from .src.nodes.saya_split_mask import SayaSplitMask
from .src.nodes.saya_upscale_mode import SayaUpscaleMode
from .src.nodes.couple_imprint import SayaCoupleImprintPack, SayaCoupleImprintUnpack
from .src.nodes.couple_imprint_v2 import SayaCoupleImprintPackV2
from .src.nodes.couple_imprint_resolve import SayaCoupleImprintResolve
from .src.nodes.couple_hidream_reconstruct import SayaCoupleHiDreamReconstruct
from .src.nodes.hidream_safe_scale import SayaHiDreamSafeScale
from .src.nodes.hires_fix_resize import SayaHiresFixResize
from .src.nodes.warmup_gate import SayaWarmupGate
from .src.nodes.hires_fix_target import SayaHiresFixTarget
from .src.nodes.couple_reconstruct import (
    SayaCoupleCheckpointIdentities,
    SayaCoupleImprintDerive,
    SayaCoupleImprintLoad,
    SayaCoupleImprintRetarget,
    SayaCoupleReconstruct,
    SayaLazyBooleanSelect,
    SayaLazyModelSelect,
    SayaValueToString,
)
from .src.nodes.region_masks import SayaCoupleRegionMasks
from .src.duo_geometry.nodes import SayaDuoTiledUpscale
from .src.duo_geometry.usdu_pass_node import SayaCoupleUSDUPass

NODE_CLASS_MAPPINGS: dict[str, type[Any]] = {
    "SayaChromaAnchor": SayaChromaAnchor,
    "SayaAttentionCouplePPM": SayaAttentionCouplePPM,
    "SayaDuoLatentShape": SayaDuoLatentShape,
    "SayaPPMMasks": SayaPPMMasks,
    "SayaDuoSegsDetail": SayaDuoSegsDetail,
    "SayaDuoTiledUpscale": SayaDuoTiledUpscale,
    "SayaNaturalizePostProcess": SayaNaturalizePostProcess,
    "SayaHiDreamShadowControlMask": SayaHiDreamShadowControlMask,
    "SayaHiDreamLoraLoader": SayaHiDreamLoraLoader,
    "SayaHiDreamLoraSettings": SayaHiDreamLoraSettings,
    "SayaImageGenerationReview": SayaImageGenerationReview,
    "SayaImagePhaseController": SayaImagePhaseController,
    "SayaLazyCheckpointLoader": SayaLazyCheckpointLoader,
    "SayaImagePhaseCheckpointLoad": SayaImagePhaseCheckpointLoad,
    "SayaImagePhaseCheckpointStop": SayaImagePhaseCheckpointStop,
    "SayaImagePhase1Stop": SayaImagePhase1Stop,
    "SayaImagePhase2Load": SayaImagePhase2Load,
    "SayaImagePhase2Stop": SayaImagePhase2Stop,
    "SayaImagePhase3Load": SayaImagePhase3Load,
    "SayaImagePhase3Stop": SayaImagePhase3Stop,
    "SayaImagePhase4Load": SayaImagePhase4Load,
    "SayaImagePhase4Stop": SayaImagePhase4Stop,
    "SayaImagePhase5Load": SayaImagePhase5Load,
    "SayaImagePhase5Stop": SayaImagePhase5Stop,
    "SayaImagePhase6Load": SayaImagePhase6Load,
    "SayaImagePhase6Stop": SayaImagePhase6Stop,
    "SayaKSamplerConfig": SayaKSamplerConfig,
    "SayaResolutionScaleCalculator": SayaResolutionScaleCalculator,
    "SayaNear4KTargetCalculator": SayaNear4KTargetCalculator,
    "SayaUpscalePresetModelLoader": SayaUpscalePresetModelLoader,
    "SayaUpscaleTargetCalculator": SayaUpscaleTargetCalculator,
    "SayaSplitMask": SayaSplitMask,
    "SayaUpscaleMode": SayaUpscaleMode,
    "SayaMultiCouple": SayaMultiCouple,
    "SayaCoupleImprintPack": SayaCoupleImprintPack,
    "SayaCoupleImprintPackV2": SayaCoupleImprintPackV2,
    "SayaCoupleImprintUnpack": SayaCoupleImprintUnpack,
    "SayaCoupleImprintLoad": SayaCoupleImprintLoad,
    "SayaCoupleImprintResolve": SayaCoupleImprintResolve,
    "SayaCoupleHiDreamReconstruct": SayaCoupleHiDreamReconstruct,
    "SayaHiDreamSafeScale": SayaHiDreamSafeScale,
    "SayaHiresFixTarget": SayaHiresFixTarget,
    "SayaHiresFixResize": SayaHiresFixResize,
    "SayaWarmupGate": SayaWarmupGate,
    "SayaCoupleReconstruct": SayaCoupleReconstruct,
    "SayaCoupleCheckpointIdentities": SayaCoupleCheckpointIdentities,
    "SayaCoupleImprintDerive": SayaCoupleImprintDerive,
    "SayaCoupleImprintRetarget": SayaCoupleImprintRetarget,
    "SayaLazyBooleanSelect": SayaLazyBooleanSelect,
    "SayaLazyModelSelect": SayaLazyModelSelect,
    "SayaValueToString": SayaValueToString,
    "SayaCoupleRegionMasks": SayaCoupleRegionMasks,
    "SayaCoupleUSDUPass": SayaCoupleUSDUPass,
}

NODE_DISPLAY_NAME_MAPPINGS = {
    "SayaChromaAnchor": "Saya Chroma Anchor",
    "SayaAttentionCouplePPM": "Saya Attention Couple PPM",
    "SayaDuoLatentShape": "Saya Duo Latent Shape · Current Pass",
    "SayaPPMMasks": "Saya PPM Masks · Layout",
    "SayaDuoSegsDetail": "Saya Duo SEGS Detail · Identity Safe",
    "SayaDuoTiledUpscale": "Saya Duo Tiled Upscale · Identity Safe",
    "SayaNaturalizePostProcess": "Saya Naturalize · Fine Grain / Dither",
    "SayaHiDreamShadowControlMask": "Saya HiDream · Shadow Control Mask",
    "SayaHiDreamLoraLoader": "Saya HiDream LoRA · MODEL Loader",
    "SayaHiDreamLoraSettings": "Saya HiDream LoRA · Settings & Trigger Prompt",
    "SayaImageGenerationReview": "Saya Image Review · Continue / Restart New Seed",
    "SayaImagePhaseController": "Saya Image Auto Phase Controller",
    "SayaLazyCheckpointLoader": "Saya Lazy Checkpoint Loader",
    "SayaImagePhaseCheckpointLoad": "Saya Image Phase LOAD Previous Checkpoint",
    "SayaImagePhaseCheckpointStop": "Saya Image Phase AUTO STOP / UNLOAD",
    "SayaImagePhase1Stop": "AUTO PASS 1 · STOP / UNLOAD",
    "SayaImagePhase2Load": "AUTO PASS 2 · LOAD",
    "SayaImagePhase2Stop": "AUTO PASS 2 · STOP / UNLOAD",
    "SayaImagePhase3Load": "AUTO PASS 3 · LOAD",
    "SayaImagePhase3Stop": "AUTO PASS 3 · STOP / UNLOAD",
    "SayaImagePhase4Load": "AUTO PASS 4 · LOAD",
    "SayaImagePhase4Stop": "AUTO PASS 4 · STOP / UNLOAD",
    "SayaImagePhase5Load": "AUTO PASS 5 · LOAD",
    "SayaImagePhase5Stop": "AUTO PASS 5 · STOP / UNLOAD",
    "SayaImagePhase6Load": "AUTO PASS 6 · LOAD",
    "SayaImagePhase6Stop": "AUTO PASS 6 · STOP / UNLOAD",
    "SayaKSamplerConfig": "Saya Sampling Config · beta45 compatible",
    "SayaResolutionScaleCalculator": "Saya Resolution Scale Calculator · AI Buckets v1.0.1",
    "SayaNear4KTargetCalculator": "Saya Dynamic Near-4K Target · Preserve Ratio",
    "SayaUpscalePresetModelLoader": "Saya Final Upscale · Preset + Model",
    "SayaUpscaleTargetCalculator": "Saya Dynamic Upscale Target · Preserve Ratio",
    "SayaSplitMask": "Saya Split Mask · Exact Complement",
    "SayaUpscaleMode": "Saya Upscale Mode",
    "SayaMultiCouple": "Saya Multi Couple · 2 models",
    "SayaCoupleImprintPack": "Saya Couple Imprint · Pack (DATA-ONLY v1)",
    "SayaCoupleImprintPackV2": "Saya Couple Imprint · Pack (DATA-ONLY v2)",
    "SayaCoupleImprintUnpack": "Saya Couple Imprint · Unpack (rebuild)",
    "SayaCoupleImprintLoad": "Saya Couple Imprint · Load (checkpoint v2)",
    "SayaCoupleImprintResolve": "Saya Couple Imprint · Resolve (ancestry)",
    "SayaCoupleHiDreamReconstruct": "Saya Couple Reconstruct · Phase 03 HiDream",
    "SayaHiDreamSafeScale": "Saya HiDream · Safe Scale (auto, reduce only)",
    "SayaHiresFixTarget": "Saya Hires Fix · Target (original x output_scale)",
    "SayaHiresFixResize": "Saya Hires Fix · Resize (one coherent rise)",
    "SayaWarmupGate": "Saya Warmup Gate (discarded frame -1)",
    "SayaCoupleReconstruct": "Saya Couple Reconstruct · Phase 02 (F4)",
    "SayaCoupleCheckpointIdentities": "Saya Couple · Checkpoint Identities (live hub)",
    "SayaCoupleImprintDerive": "Saya Couple Imprint · Derive Prompt Variant",
    "SayaCoupleImprintRetarget": "Saya Couple Imprint · Retarget (Model 1/2 hub)",
    "SayaLazyBooleanSelect": "Saya Lazy Boolean Select",
    "SayaLazyModelSelect": "Saya Lazy Model Select (Model 1/2, no eager compute)",
    "SayaValueToString": "Saya Value to String",
    "SayaCoupleRegionMasks": "Saya Couple Region Masks · Current Pass (F2/F3)",
    "SayaCoupleUSDUPass": "Saya Couple USDU Pass · Explicit Config (F5)",
}

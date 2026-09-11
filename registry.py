from __future__ import annotations

from typing import Any

from .src.nodes.hidream_lora import SayaHiDreamLoraLoader, SayaHiDreamLoraSettings

from .src.nodes.couple_conditioning import CoupleConditioningNode

from .src.nodes.couple_conditioning_hidream import (
    SayaComfyCoupleHiDream,
    SayaComfyCoupleHiDreamCopy,
)

from .src.nodes.couple_phase_prompts import (
    SayaCouplePromptBundlePack,
    SayaCouplePromptBundleUnpack,
    SayaLatentShapeFromImage,
)


from .src.nodes.image_phases import (
    SayaImageGenerationReview,
    SayaImageModelHubSettings,
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
    SayaImageVAERouteSettings,
    SayaLazyCheckpointLoader,
)

from .src.nodes.detailers.retry import SayaDetailerForEachAutoRetry
from .src.nodes.detailers.standard import SayaDetailerForEach

from .src.nodes.text_and_model_routing import (
    DualClipTextEncoderNode,
    HiresModelRouterNode,
)

from .src.nodes.sampling_config import SayaKSamplerConfig

from .src.nodes.saya_resolution_scale import (
    SayaNear4KTargetCalculator,
    SayaResolutionScaleCalculator,
    SayaUpscalePresetModelLoader,
    SayaUpscaleTargetCalculator,
)

from .forge.node import (
    SayaComfyCoupleForge,
    SayaComfyCoupleForgeCopy,
)

from .forge.usdu_bridge import (
    SayaUSDU1IdentitySafe,
    SayaUSDU2IdentitySafe,
)


NODE_CLASS_MAPPINGS: dict[str, type[Any]] = {
    "SayaHiDreamLoraLoader": SayaHiDreamLoraLoader,
    "SayaHiDreamLoraSettings": SayaHiDreamLoraSettings,
    "SayaImageGenerationReview": SayaImageGenerationReview,
    "SayaImagePhaseController": SayaImagePhaseController,
    "SayaImageModelHubSettings": SayaImageModelHubSettings,
    "SayaImageVAERouteSettings": SayaImageVAERouteSettings,
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

    "SayaComfyCouple": CoupleConditioningNode,
    "SayaComfyCoupleHiDream": SayaComfyCoupleHiDream,
    "SayaComfyCoupleHiDreamCopy": SayaComfyCoupleHiDreamCopy,
    "SayaCouplePromptBundlePack": SayaCouplePromptBundlePack,
    "SayaCouplePromptBundleUnpack": SayaCouplePromptBundleUnpack,
    "SayaLatentShapeFromImage": SayaLatentShapeFromImage,
    "SayaDualCLIPTextEncode": DualClipTextEncoderNode,
    "SayaHiresTrioRouterSharedPrompt": HiresModelRouterNode,
    "SayaKSamplerConfig": SayaKSamplerConfig,

    "SayaResolutionScaleCalculator": SayaResolutionScaleCalculator,
    "SayaNear4KTargetCalculator": SayaNear4KTargetCalculator,
    "SayaUpscalePresetModelLoader": SayaUpscalePresetModelLoader,
    "SayaUpscaleTargetCalculator": SayaUpscaleTargetCalculator,

    "SayaDetailerForEach": SayaDetailerForEach,
    "SayaDetailerForEachAutoRetry": SayaDetailerForEachAutoRetry,

    "SayaComfyCoupleForge": SayaComfyCoupleForge,
    "SayaComfyCoupleForgeCopy": SayaComfyCoupleForgeCopy,
    "SayaUSDU1IdentitySafe": SayaUSDU1IdentitySafe,
    "SayaUSDU2IdentitySafe": SayaUSDU2IdentitySafe,
}


NODE_DISPLAY_NAME_MAPPINGS = {
    "SayaHiDreamLoraLoader": "Saya HiDream LoRA · MODEL Loader",
    "SayaHiDreamLoraSettings": "Saya HiDream LoRA · Settings & Trigger Prompt",
    "SayaImageGenerationReview": "Saya Image Review · Continue / Restart New Seed",
    "SayaImagePhaseController": "Saya Image Auto Phase Controller",
    "SayaImageModelHubSettings": "Saya Image Model Hub · Settings Only",
    "SayaImageVAERouteSettings": "Saya Image VAE Routes · Settings Only",
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

    "SayaComfyCouple": "Saya Comfy Couple",
    "SayaComfyCoupleHiDream": "Saya Comfy Couple - HiDream (native regional)",
    "SayaComfyCoupleHiDreamCopy": "Saya Comfy Couple - HiDream - COPY",
    "SayaCouplePromptBundlePack": "Saya Couple Prompt Bundle PACK",
    "SayaCouplePromptBundleUnpack": "Saya Couple Prompt Bundle UNPACK",
    "SayaLatentShapeFromImage": "Saya Latent Shape From Image",
    "SayaDualCLIPTextEncode": "Saya Dual CLIP Text Encode",
    "SayaHiresTrioRouterSharedPrompt": "Saya Hires Trio Router Shared Prompt RESCUE",
    "SayaKSamplerConfig": "Saya Sampling Config · beta45 compatible",

    "SayaResolutionScaleCalculator": "Saya Resolution Scale Calculator · Exact Only",
    "SayaNear4KTargetCalculator": "Saya Dynamic Near-4K Target · Preserve Ratio",
    "SayaUpscalePresetModelLoader": "Saya Final Upscale · Preset + Model",
    "SayaUpscaleTargetCalculator": "Saya Dynamic Upscale Target · Preserve Ratio",

    "SayaDetailerForEach": "Saya Detailer For Each · Couple Crop",
    "SayaDetailerForEachAutoRetry": "Saya Detailer For Each AutoRetry · Couple Crop",

    "SayaComfyCoupleForge": "Saya Comfy Couple - Forge Coarse Main 2.3",
    "SayaComfyCoupleForgeCopy": "Saya Comfy Couple - Forge Coarse Main 2.3 - COPY",
    "SayaUSDU1IdentitySafe": "Saya Forge Couple · USDU 1 · Identity Safe V2.1",
    "SayaUSDU2IdentitySafe": "Saya Forge Couple · USDU 2 · Identity Safe V2.1",
}

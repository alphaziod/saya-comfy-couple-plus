"""ComfyUI nodes for strictly isolated image-generation phases."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Self

from ..services.image_phases import (
    atomic_write_json,
    DETAILERS,
    checkpoint_paths,
    emit_phase_complete,
    load_validated_source,
    normalize_detailer,
    parse_json_widget,
    parse_phase,
    phase_status,
    save_and_promote_candidate,
    save_candidate,
    unload_everything,
)
from ..services.models import (
    build_model_choice_list,
    load_vae_or_fallback,
)

CATEGORY = "Saya/Image Phases"
DEFAULT_CHECKPOINT_ROOT = "image/checkpoints"
PHASE_CHOICES = [
    "0 — IDLE",
    "1 — BASE GENERATION",
    "2 — HIRES / USDU",
    "3 — REFINER HIDREAM",
    "4 — PRE-DETAIL UPSCALE",
    "5 — DETAILERS",
    "6 — FINAL UPSCALE / COLOR / SAVE",
]
DETAILER_CHOICES = ["none", *DETAILERS]

class SayaImagePhaseController:
    """One-click automatic sequence controller with no phase selector."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        """Expose no phase/detailer widget: GO always starts at pass 1."""
        return {"required": {}}

    RETURN_TYPES = ("STRING", "STRING", "STRING", "STRING")
    RETURN_NAMES = ("status", "checkpoint", "vram", "message")
    FUNCTION = "inspect"
    CATEGORY = CATEGORY

    @classmethod
    def IS_CHANGED(cls: type[Self], **kwargs: Any) -> Any:
        """Refresh status whenever the graph explicitly evaluates the node."""
        del kwargs
        return float("nan")

    def inspect(self: Self) -> tuple[str, str, str, str]:
        """Return an IDLE snapshot without loading any model."""
        status = phase_status(0, "none", DEFAULT_CHECKPOINT_ROOT)
        memory = json.dumps(status.get("memory", {}), ensure_ascii=False)
        return (
            json.dumps(status, ensure_ascii=False),
            "",
            memory,
            "IDLE — GO automatically runs phases 1 through 6",
        )


class SayaLazyCheckpointLoader:
    """Load one checkpoint bundle only when its isolated phase is scheduled."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        """Return lazy checkpoint and VAE selectors."""
        checkpoints = build_model_choice_list("checkpoints")
        vaes = build_model_choice_list(
            "vae", extras=("none", "use checkpoint VAE")
        )
        return {
            "required": {
                "ckpt_name": (checkpoints,),
                "vae_name": (vaes,),
                "load_clip": ("BOOLEAN", {"default": True}),
                "load_checkpoint_vae": ("BOOLEAN", {"default": True}),
            }
        }

    RETURN_TYPES = ("MODEL", "CLIP", "VAE", "STRING")
    RETURN_NAMES = ("model", "clip", "vae", "loaded_json")
    FUNCTION = "load"
    CATEGORY = CATEGORY

    @classmethod
    def IS_CHANGED(
        cls: type[Self],
        ckpt_name: str,
        vae_name: str,
        load_clip: bool,
        load_checkpoint_vae: bool,
    ) -> str:
        """Include file stats in ComfyUI's cache token when resolvable."""
        from ..services.models import resolve_registered_model_path

        parts = [ckpt_name, vae_name, str(load_clip), str(load_checkpoint_vae)]
        for kind, name in (("checkpoints", ckpt_name), ("vae", vae_name)):
            path = resolve_registered_model_path(kind, name)
            if path:
                try:
                    stat = Path(path).stat()
                    parts.extend((str(stat.st_mtime_ns), str(stat.st_size)))
                except OSError:
                    pass
        return ":".join(parts)

    def load(
        self: Self,
        ckpt_name: str,
        vae_name: str,
        load_clip: bool,
        load_checkpoint_vae: bool,
    ) -> tuple[Any, Any, Any, str]:
        """Load exactly one checkpoint and only the requested CLIP/VAE pieces."""
        import comfy.sd
        import folder_paths

        from ..services.models import resolve_registered_model_path

        checkpoint_path = resolve_registered_model_path("checkpoints", ckpt_name)
        if not checkpoint_path:
            raise RuntimeError(f"Checkpoint not found: {ckpt_name}")
        use_checkpoint_vae = bool(load_checkpoint_vae) or vae_name == "use checkpoint VAE"
        model, clip, checkpoint_vae, _ = comfy.sd.load_checkpoint_guess_config(
            checkpoint_path,
            output_vae=use_checkpoint_vae,
            output_clip=bool(load_clip),
            embedding_directory=folder_paths.get_folder_paths("embeddings"),
        )

        if vae_name not in {"", "none", "use checkpoint VAE"}:
            vae = load_vae_or_fallback(vae_name)
            if vae is None:
                raise RuntimeError(f"VAE not found: {vae_name}")
        elif vae_name == "use checkpoint VAE" or load_checkpoint_vae:
            vae = checkpoint_vae
        else:
            vae = None
        loaded = {
            "checkpoint": ckpt_name,
            "clip_loaded": clip is not None,
            "vae": vae_name,
            "checkpoint_vae_loaded": checkpoint_vae is not None,
        }
        return model, clip, vae, json.dumps(loaded, ensure_ascii=False)




class SayaImageGenerationReview:
    """Phase-1 review output with Continue / Restart-new-seed popup."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        return {
            "required": {
                "image": ("IMAGE",),
                "seed": ("INT", {"default": 0, "min": 0, "max": 0xFFFFFFFFFFFFFFFF}),
                "positive_prompt": ("STRING", {"default": "", "multiline": True}),
                "negative_prompt": ("STRING", {"default": "", "multiline": True}),
                "models_json": ("STRING", {"default": "[]", "multiline": True}),
                "vaes_json": ("STRING", {"default": "[]", "multiline": True}),
                "samplers_json": ("STRING", {"default": "{}", "multiline": True}),
                "checkpoint_root": ("STRING", {"default": "image/checkpoints"}),
            },
            "optional": {
                "imprint_json": ("STRING", {"forceInput": True}),
            },
        }

    RETURN_TYPES = ("IMAGE", "STRING")
    RETURN_NAMES = ("image", "candidate_manifest")
    FUNCTION = "review"
    OUTPUT_NODE = True
    CATEGORY = CATEGORY

    @classmethod
    def IS_CHANGED(cls: type[Self], **kwargs: Any) -> Any:
        del kwargs
        return float("nan")

    def review(
        self: Self,
        image: Any,
        seed: int,
        positive_prompt: str,
        negative_prompt: str,
        models_json: Any,
        vaes_json: Any,
        samplers_json: Any,
        checkpoint_root: str,
        imprint_json: str | None = None,
    ) -> dict[str, Any]:
        """Save a phase-1 candidate and expose a real preview to the frontend."""
        models = parse_json_widget(models_json, list, "models_json")
        vaes = parse_json_widget(vaes_json, list, "vaes_json")
        samplers = parse_json_widget(samplers_json, dict, "samplers_json")
        candidate_path, manifest = save_candidate(
            images=image,
            phase=1,
            detailer="none",
            checkpoint_root=checkpoint_root,
            source_path="",
            seed=int(seed),
            positive_prompt=str(positive_prompt),
            negative_prompt=str(negative_prompt),
            models=models,
            vaes=vaes,
            samplers=samplers,
            imprint_json=imprint_json,
        )

        from nodes import PreviewImage

        preview = PreviewImage().save_images(
            image, filename_prefix="saya_phase1_review"
        )
        ui = dict(preview.get("ui", {}))
        ui["saya_review"] = [{
            "phase": 1,
            "seed": int(seed),
            "checkpoint_root": checkpoint_root,
            "candidate_path": str(candidate_path),
            "transaction_uuid": str(manifest.get("transaction_uuid", "")),
        }]
        return {
            "ui": ui,
            "result": (image, json.dumps(manifest, ensure_ascii=False)),
        }


class SayaImagePhaseCheckpointLoad:
    """Load a validated image checkpoint without unloading models."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        """Return the checkpoint-start widgets."""
        return {
            "required": {
                "phase": (PHASE_CHOICES[1:],),
                "detailer": (DETAILER_CHOICES,),
                "checkpoint_root": ("STRING", {"default": "image/checkpoints"}),
            }
        }

    RETURN_TYPES = ("IMAGE", "STRING", "STRING", "INT", "STRING", "STRING")
    RETURN_NAMES = (
        "image",
        "manifest_json",
        "checkpoint_path",
        "seed",
        "positive_prompt",
        "negative_prompt",
    )
    FUNCTION = "load"
    CATEGORY = CATEGORY

    @classmethod
    def IS_CHANGED(
        cls: type[Self], phase: str, detailer: str, checkpoint_root: str
    ) -> str:
        """Invalidate cache when the selected validated manifest changes."""
        phase_number = parse_phase(phase)
        try:
            source = checkpoint_paths(phase_number - 1, checkpoint_root)
            stat = source.validated_manifest.stat()
            return f"{phase_number}:{detailer}:{stat.st_mtime_ns}:{stat.st_size}"
        except Exception as error:
            return f"missing:{phase_number}:{detailer}:{error}"

    def load(
        self: Self, phase: str, detailer: str, checkpoint_root: str
    ) -> tuple[Any, str, str, int, str, str]:
        """Restore image/text/seed from a validated manifest without unloading."""
        phase_number = parse_phase(phase)
        normalized_detailer = normalize_detailer(detailer)
        image, manifest, path = load_validated_source(
            phase_number, normalized_detailer, checkpoint_root
        )
        return (
            image,
            json.dumps(manifest, ensure_ascii=False),
            str(path),
            int(manifest.get("seed", 0)),
            str(manifest.get("positive_prompt", "")),
            str(manifest.get("negative_prompt", "")),
        )


class SayaImagePhaseCheckpointStop:
    """Atomic phase boundary with optional model offload before automatic handoff."""

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        """Return the terminal checkpoint widgets and optional save receipt."""
        return {
            "required": {
                "image": ("IMAGE",),
                "phase": (PHASE_CHOICES[1:],),
                "detailer": (DETAILER_CHOICES,),
                "checkpoint_root": ("STRING", {"default": "image/checkpoints"}),
                "seed": ("INT", {"default": 0, "min": 0, "max": 0xFFFFFFFFFFFFFFFF}),
                "positive_prompt": ("STRING", {"default": "", "multiline": True}),
                "negative_prompt": ("STRING", {"default": "", "multiline": True}),
                "models_json": ("STRING", {"default": "[]", "multiline": True}),
                "vaes_json": ("STRING", {"default": "[]", "multiline": True}),
                "samplers_json": ("STRING", {"default": "{}", "multiline": True}),
            },
            "optional": {
                "source_path": ("STRING", {"default": ""}),
                "save_receipt": ("STRING", {"forceInput": True}),
            },
        }

    RETURN_TYPES = ("IMAGE", "STRING", "STRING")
    RETURN_NAMES = ("image", "validated_path", "manifest_json")
    FUNCTION = "stop"
    OUTPUT_NODE = True
    CATEGORY = CATEGORY

    @classmethod
    def IS_CHANGED(cls: type[Self], **kwargs: Any) -> Any:
        """Always create a fresh atomic transaction for a targeted phase."""
        del kwargs
        return float("nan")

    def stop(
        self: Self,
        image: Any,
        phase: str,
        detailer: str,
        checkpoint_root: str,
        seed: int,
        positive_prompt: str,
        negative_prompt: str,
        models_json: str,
        vaes_json: str,
        samplers_json: str,
        source_path: str = "",
        save_receipt: str = "",
    ) -> dict[str, Any]:
        """Save, validate, unload, and signal the next run.

        Phase 1 reaches this node only after the existing Image Filter popup has
        been accepted.  Phases 2-6 have no popup: they stop here, unload, then
        the frontend starts the next phase two seconds later.
        """
        del save_receipt
        phase_number = parse_phase(phase)
        normalized_detailer = normalize_detailer(detailer)
        models = parse_json_widget(models_json, list, "models_json")
        vaes = parse_json_widget(vaes_json, list, "vaes_json")
        samplers = parse_json_widget(samplers_json, dict, "samplers_json")
        unload_report: dict[str, Any] = {}
        validated: dict[str, Any] = {}
        validated_path = ""
        try:
            validated, validated_image = save_and_promote_candidate(
                images=image,
                phase=phase_number,
                detailer=normalized_detailer,
                checkpoint_root=checkpoint_root,
                source_path=source_path,
                seed=seed,
                positive_prompt=positive_prompt,
                negative_prompt=negative_prompt,
                models=models,
                vaes=vaes,
                samplers=samplers,
            )
            validated_path = str(validated_image)
        finally:
            # Every STOP is a phase boundary: unload models and drop the cached
            # references, otherwise patchers pile up in RAM until OOM. The disk
            # conditioning cache is kept.
            unload_report = unload_everything()

        validated["unload"] = unload_report
        validated_manifest = checkpoint_paths(
            phase_number, checkpoint_root
        ).validated_manifest
        atomic_write_json(validated_manifest, validated)
        next_phase = phase_number + 1 if phase_number < 6 else 0
        notified = emit_phase_complete(
            phase=phase_number,
            next_phase=next_phase,
            manifest=validated,
            unload_report=unload_report,
        )
        message = (
            f"Phase {phase_number} saved. "
            + (f"Phase {next_phase} in 2 s." if next_phase else "Back to IDLE.")
        )
        return {
            "ui": {"text": [message, f"Checkpoint: {validated_path}"]},
            "result": (
                image,
                validated_path,
                json.dumps(
                    {**validated, "frontend_notified": notified},
                    ensure_ascii=False,
                ),
            ),
        }

class _FixedPhaseLoad(SayaImagePhaseCheckpointLoad):
    """Hidden fixed-phase loader used by the automatic sequence."""

    PHASE = 0

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        return {"required": {}}

    @classmethod
    def IS_CHANGED(cls: type[Self], **kwargs: Any) -> str:
        del kwargs
        try:
            source = checkpoint_paths(cls.PHASE - 1, DEFAULT_CHECKPOINT_ROOT)
            stat = source.validated_manifest.stat()
            return f"{cls.PHASE}:{stat.st_mtime_ns}:{stat.st_size}"
        except Exception as error:
            return f"missing:{cls.PHASE}:{error}"

    def load(self: Self) -> tuple[Any, str, str, int, str, str]:
        return super().load(
            PHASE_CHOICES[self.PHASE], "none", DEFAULT_CHECKPOINT_ROOT
        )


class SayaImagePhase2Load(_FixedPhaseLoad):
    PHASE = 2


class SayaImagePhase3Load(_FixedPhaseLoad):
    PHASE = 3


class SayaImagePhase4Load(_FixedPhaseLoad):
    PHASE = 4


class SayaImagePhase5Load(_FixedPhaseLoad):
    PHASE = 5


class SayaImagePhase6Load(_FixedPhaseLoad):
    PHASE = 6


class _FixedPhaseStop(SayaImagePhaseCheckpointStop):
    """Hidden fixed-phase STOP node: no phase/detailer selector exists."""

    PHASE = 0

    @classmethod
    def INPUT_TYPES(cls: type[Self]) -> dict[str, Any]:
        return {
            "required": {
                "image": ("IMAGE",),
                "seed": ("INT", {"default": 0, "min": 0, "max": 0xFFFFFFFFFFFFFFFF}),
                "positive_prompt": ("STRING", {"default": "", "multiline": True}),
                "negative_prompt": ("STRING", {"default": "", "multiline": True}),
                "models_json": ("STRING", {"default": "[]", "multiline": True}),
                "vaes_json": ("STRING", {"default": "[]", "multiline": True}),
                "samplers_json": ("STRING", {"default": "{}", "multiline": True}),
            },
            "optional": {
                "source_path": ("STRING", {"default": ""}),
                "save_receipt": ("STRING", {"forceInput": True}),
            },
        }

    def stop(
        self: Self,
        image: Any,
        seed: int,
        positive_prompt: str,
        negative_prompt: str,
        models_json: Any,
        vaes_json: Any,
        samplers_json: Any,
        source_path: str = "",
        save_receipt: str = "",
    ) -> dict[str, Any]:
        return super().stop(
            image=image,
            phase=PHASE_CHOICES[self.PHASE],
            detailer="none",
            checkpoint_root=DEFAULT_CHECKPOINT_ROOT,
            seed=seed,
            positive_prompt=positive_prompt,
            negative_prompt=negative_prompt,
            models_json=models_json,
            vaes_json=vaes_json,
            samplers_json=samplers_json,
            source_path=source_path,
            save_receipt=save_receipt,
        )


class SayaImagePhase1Stop(_FixedPhaseStop):
    PHASE = 1


class SayaImagePhase2Stop(_FixedPhaseStop):
    PHASE = 2


class SayaImagePhase3Stop(_FixedPhaseStop):
    PHASE = 3


class SayaImagePhase4Stop(_FixedPhaseStop):
    PHASE = 4


class SayaImagePhase5Stop(_FixedPhaseStop):
    PHASE = 5


class SayaImagePhase6Stop(_FixedPhaseStop):
    PHASE = 6

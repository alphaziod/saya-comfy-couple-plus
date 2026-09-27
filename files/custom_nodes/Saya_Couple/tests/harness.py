"""Shared test harness for the Saya Couple pack.

Loads the pack the way ComfyUI does (package import with a stubbed
PromptServer, so route registration works without a running server) and
provides tiny assert helpers so tests stay dependency-free.

Run everything with:
    PYTHONDONTWRITEBYTECODE=1 <ComfyUI python> tests/run_all.py
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import os

# ComfyUI root = custom_nodes/<pack>/tests/../../.. ; override with SAYA_COMFY_ROOT.
COMFY_ROOT = Path(os.environ.get("SAYA_COMFY_ROOT") or Path(__file__).resolve().parents[3])
PACK_ROOT = Path(__file__).resolve().parents[1]

if str(COMFY_ROOT) not in sys.path:
    sys.path.insert(0, str(COMFY_ROOT))

_PACK = None


def load_pack():
    """Import the pack once, with PromptServer stubbed for route registration."""
    global _PACK
    if _PACK is not None:
        return _PACK

    import server

    class _Routes:
        def get(self, *a, **k):
            return lambda f: f

        def post(self, *a, **k):
            return lambda f: f

    class _FakeServer:
        routes = _Routes()

        @staticmethod
        def send_sync(event, payload):
            pass

    if getattr(server.PromptServer, "instance", None) is None:
        server.PromptServer.instance = _FakeServer()

    spec = importlib.util.spec_from_file_location(
        "saya_couple",
        PACK_ROOT / "__init__.py",
        submodule_search_locations=[str(PACK_ROOT)],
    )
    module = importlib.util.module_from_spec(spec)
    sys.modules["saya_couple"] = module
    spec.loader.exec_module(module)
    _PACK = module
    return module


# --- tiny assertion helpers ---------------------------------------------------


class Check:
    """Collects failures instead of stopping at the first one."""

    def __init__(self, name: str):
        self.name = name
        self.failures: list[str] = []
        self.skip_reason: str | None = None

    def ok(self, condition: bool, label: str) -> None:
        if not condition:
            self.failures.append(label)

    def eq(self, got, want, label: str) -> None:
        if got != want:
            self.failures.append(f"{label}: got {got!r}, want {want!r}")

    def raises(self, exc_types, fn, label: str) -> None:
        try:
            fn()
        except exc_types:
            return
        except Exception as error:  # wrong exception type still fails the check
            self.failures.append(f"{label}: raised {type(error).__name__}, want {exc_types}")
            return
        self.failures.append(f"{label}: no exception, want {exc_types}")

    def skip(self, reason: str) -> None:
        """Mark this check as skipped (e.g. an optional external resource is absent).

        A skipped check must carry no failures: skip() is meant to be called
        as the check function's only action (early return right after).
        """
        self.skip_reason = reason

    def report(self) -> tuple[str, list[str], str | None]:
        return self.name, self.failures, self.skip_reason


def sample_v2_imprint_json() -> str:
    """A minimal, valid v2 imprint for tests that need a real embedded imprint chunk."""
    load_pack()
    from saya_couple.src.nodes.couple_imprint_v2 import (
        ORIENTATION_LEFT_RIGHT,
        ROLE_BASE_CLIP,
        ROLE_PHASE_MODEL,
        build_identity,
        build_imprint_v2,
        build_prompts,
        build_provenance,
        build_reconstruction_recipe,
        build_strengths,
        build_two_region_geometry,
        canonical_imprint_json,
    )

    imprint = build_imprint_v2(
        prompts=build_prompts(main="scene", person_1="left", person_2="right", negative="bad"),
        geometry=build_two_region_geometry(
            orientation=ORIENTATION_LEFT_RIGHT,
            reference_width=64,
            reference_height=64,
            feather_unit="axis_fraction",
        ),
        strengths=build_strengths(),
        reconstruction_recipe=build_reconstruction_recipe(
            checkpoint_identity=build_identity(role=ROLE_PHASE_MODEL, identifier="base.safetensors"),
            clip_identity=build_identity(role=ROLE_BASE_CLIP, identifier="base.safetensors"),
        ),
        provenance=build_provenance(
            workflow_version="test",
            pack_discriminant="sha256:test",
            created_at="2026-09-23T00:00:00+00:00",
        ),
    )
    return canonical_imprint_json(imprint)

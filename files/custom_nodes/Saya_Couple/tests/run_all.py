"""Run every test module and report a compact PASS/FAIL summary.

Usage:
    PYTHONDONTWRITEBYTECODE=1 <ComfyUI python> tests/run_all.py
"""

from __future__ import annotations

import sys
import tempfile
import time
import traceback
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

import test_couple_crop_f8
import test_detailer_crop
import test_naturalize_postprocess
import test_geometry
import test_upscale_mode
import test_multi_couple
import test_core_saya_dual
import test_dual_wiring
import test_nodes_compute
import test_phase_nodes
import test_phase_services
import test_phase_restart_restore
import test_imprint_resolve
import test_conditioning_cache
import test_hidream_reconstruct
import test_hidream_attention
import test_usdu_solo_crop
import test_removed_paths
import test_hidream_safe_scale
import test_hires_fix_resize
import test_warmup_gate
import test_hires_fix_target
import test_registry
import test_split_mask
import test_workflow_compat
import test_workflow_v030
import test_audit_regressions


def guarded(tests):
    """Run each test; an exception is reported as a failure of that test, never ends the run."""
    for fn in tests:
        try:
            yield fn()
        except Exception:
            yield fn.__name__, [f"crashed:\n{traceback.format_exc(limit=4)}"], None


def main() -> int:
    started = time.monotonic()
    total = passed = skipped = 0
    failures: list[str] = []
    skips: list[str] = []

    def absorb(results):
        nonlocal total, passed, skipped
        for name, problems, skip_reason in results:
            total += 1
            if skip_reason is not None:
                skipped += 1
                skips.append(f"[{name}] {skip_reason}")
                print(f"  SKIP {name}: {skip_reason}")
            elif problems:
                for problem in problems:
                    failures.append(f"[{name}] {problem}")
            else:
                passed += 1
                print(f"  PASS {name}")

    print("== registry ==")
    absorb(guarded(test_registry.TESTS))

    print("== geometry ==")
    absorb(guarded(test_geometry.TESTS))

    print("== phase nodes ==")
    absorb(guarded(test_phase_nodes.TESTS))

    print("== compute nodes ==")
    absorb(guarded(test_nodes_compute.TESTS))
    absorb(guarded(test_split_mask.TESTS))
    absorb(guarded(test_upscale_mode.TESTS))
    absorb(guarded(test_multi_couple.TESTS))
    absorb(guarded(test_core_saya_dual.TESTS))
    absorb(guarded(test_dual_wiring.TESTS))

    print("== couple crop F8 ==")
    absorb(guarded(test_couple_crop_f8.TESTS))
    absorb(guarded(test_usdu_solo_crop.TESTS))

    print("== detailer crop ==")
    absorb(guarded(test_detailer_crop.TESTS))

    print("== naturalize post-process ==")
    absorb(guarded(test_naturalize_postprocess.TESTS))

    print("== phase services ==")
    with tempfile.TemporaryDirectory(prefix="saya_phase_test_") as tmp:
        absorb(test_phase_services.run(Path(tmp)))

    print("== phase restart restore ==")
    with tempfile.TemporaryDirectory(prefix="saya_restart_restore_") as tmp:
        absorb(test_phase_restart_restore.run(Path(tmp)))

    print("== imprint resolve ==")
    absorb(guarded(test_imprint_resolve.TESTS))

    print("== hidream reconstruct ==")
    absorb(guarded(test_hidream_reconstruct.TESTS))
    absorb(guarded(test_hidream_attention.TESTS))

    print("== conditioning cache ==")
    absorb(guarded(test_conditioning_cache.TESTS))

    print("== hidream safe scale ==")
    absorb(guarded(test_hidream_safe_scale.TESTS))

    print("== hires fix target ==")
    absorb(guarded(test_hires_fix_target.TESTS))

    print("== hires fix resize ==")
    absorb(guarded(test_hires_fix_resize.TESTS))
    absorb(guarded(test_warmup_gate.TESTS))

    print("== phase load purge ==")

    print("== workflow compat ==")
    absorb(guarded(test_workflow_compat.TESTS))
    absorb(guarded(test_workflow_v030.TESTS))
    absorb(guarded(test_removed_paths.TESTS))

    print("== audit regressions ==")
    absorb(guarded(test_audit_regressions.TESTS))

    print()
    if failures:
        print(f"FAILURES ({len(failures)}):")
        for failure in failures:
            print("  FAIL", failure)
    if skips:
        print(f"SKIPPED ({len(skips)}):")
        for skip in skips:
            print("  SKIP", skip)
    elapsed = time.monotonic() - started
    failed = total - passed - skipped
    print(
        f"\npassed={passed} failed={failed} skipped={skipped} total={total} "
        f"({elapsed:.1f}s)"
    )
    return 0 if not failures else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception:
        traceback.print_exc()
        sys.exit(2)

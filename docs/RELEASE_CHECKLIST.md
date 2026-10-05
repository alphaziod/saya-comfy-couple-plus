# Release checklist

Run before every Saya Couple release. Commands assume the repository root.

1. **Upstream ComfyUI**: note the latest commit/tag; decide whether it becomes the new tested base.
2. **Compatibility**
   - `./saya check-compat --comfyui <ComfyUI>` on the tested base and on the latest upstream.
   - `python3 tools/scan_history.py --repo <clone of comfy-org/ComfyUI>` → updates `compatibility.json`.
   - If the tested base changes: update `officially_tested` (commit, `key_files`, `files`, `files_patched` hashes).
3. **Patches** (2.0: no required core patch)
   - `patches/amd_vram_safety.patch` applies to the tested base and reverses to the exact upstream file.
   - `patches/legacy/saya_dual_attention_1.x.patch` still reverses the 1.x `attention.py` (`files_patched_legacy` hash)
     to the exact upstream file: the upgrade path depends on it.
   - `patches/third_party/*` match the versions actually in use.
4. **Tests**
   - Pack: `files/custom_nodes/Saya_Couple` is an rsync of the live pack **then neutralised** (no maintainer path, no
     personal LoRA trigger in `src/nodes/main_prompt.py` / `tests/test_main_prompt.py`; grep `/home/` and the private
     names file). `<ComfyUI python> files/custom_nodes/Saya_Couple/tests/run_all.py` with `SAYA_COMFYUI_ROOT=<ComfyUI>`
     → 0 failures.
   - Gain proofs: `<ComfyUI python> tests/proof_gain.py --comfyui <ComfyUI>` → ALL PASS.
   - Installer: `python3 tests/installer_scenarios.py --clean <clean ComfyUI> --python <ComfyUI python> --old <incompatible ComfyUI>`
     (install / verify / idempotence / restore / drift / modified core / **1.x upgrade** / NVIDIA & AMD simulations / dry-run / old version).
     The clean ComfyUI may hold symlinked custom nodes (MultiMaskCouple, RES4LYF, Ultimate SD Upscale, Impact Pack).
5. **Workflows**
   - Rebuild if the sampling setup changed: `python3 tools/build_demo_workflow.py --source <validated workflow>` (the
     API prompt is regenerated from the running server's `/object_info`) and
     `python3 tools/build_full_workflow.py --source <validated full workflow>` (structure must stay identical: same
     nodes, modes, links; the French MAIN node becomes the English one, anatomy prompts are emptied).
   - Load the full workflow in the pack suite: `SAYA_TEST_EXTERNAL_WORKFLOWS=1 SAYA_TEST_WORKFLOW_PATHS=workflows/Saya_Couple_Full.json`.
   - On a clean install: `python tests/live_demo_check.py --url <ComfyUI> --workflow workflows/Saya_Couple_Demo.json --models <m1>,<m2> --out docs/images/demo_example.png --save-api workflows/Saya_Couple_Demo_api.json --reference <validated API prompt>`
     → no missing node types, only `ckpt_name` changes when selecting models, Shark core IDENTICAL, one successful generation (image saved without metadata).
   - Full workflow: load it with `--no-generate` → no missing node types.
6. **Install / verify / restore on a clean copy** of the tested ComfyUI (not your own install), then restore and
   check the core is byte-identical to before.
7. **Licenses**: new vendored or adapted code? Update `THIRD_PARTY_NOTICES.md`, file headers, `tools/build_manifest.py`.
8. **Personal data scan**: `python3 tools/scan_personal_data.py --private <your private names file, kept OUTSIDE the repo>` → must print `CLEAN`.
9. **Hashes**: bump `files/custom_nodes/Saya_Couple/VERSION.txt`, then `python3 tools/build_manifest.py`.
10. **README / guides**: recommended gain, tested base, compatibility statuses, nothing marked available that is not;
    published prompt guides carry no personal model, LoRA or trigger name and no explicit wording.
11. Only then: tag and publish.

# Release checklist

Run before every Saya Couple release. Commands assume the repository root.

1. **Upstream ComfyUI**: note the latest commit/tag; decide whether it becomes the new tested base.
2. **Compatibility**
   - `./saya check-compat --comfyui <ComfyUI>` on the tested base and on the latest upstream.
   - `python3 tools/scan_history.py --repo <clone of comfy-org/ComfyUI>` → updates `compatibility.json`.
   - If the tested base changes: update `officially_tested` (commit, `key_files`, `files`, `files_patched` hashes).
3. **Patches**
   - `patches/saya_dual_attention.patch` applies to the tested base and reverses to the exact upstream file.
   - `patches/amd_vram_safety.patch` same.
   - `files/comfy/**` reference files == upstream + patch (byte-identical).
4. **Tests**
   - Pack: `<ComfyUI python> custom_nodes/Saya_Couple/tests/run_all.py` → 0 failures.
   - Gain proofs: `<ComfyUI python> tests/proof_gain.py --comfyui <ComfyUI>` → ALL PASS.
   - Installer: `python3 tests/installer_scenarios.py --clean <clean ComfyUI> --python <ComfyUI python> --old <incompatible ComfyUI>`
     (install / verify / idempotence / restore / drift / conflicts / NVIDIA & AMD simulations / dry-run / old version).
5. **Workflows**
   - Rebuild if the sampling setup changed: `python3 tools/build_demo_workflow.py --source <validated workflow>` and
     `python3 tools/build_full_workflow.py --source <validated full workflow>` (structure must stay identical: same nodes, modes, links).
   - On a clean install: `python tests/live_demo_check.py --url <ComfyUI> --workflow workflows/Saya_Couple_Demo.json --models <m1>,<m2> --out docs/images/demo_example.png --save-api workflows/Saya_Couple_Demo_api.json --reference <validated API prompt>`
     → no missing node types, only `ckpt_name` changes when selecting models, Shark core IDENTICAL, one successful generation (image saved without metadata).
   - Full workflow: load it with `--no-generate` → no missing node types.
6. **Install / verify / restore on a clean copy** of the tested ComfyUI (not your own install), then restore and
   check the core is byte-identical to before.
7. **Licenses**: new vendored or adapted code? Update `THIRD_PARTY_NOTICES.md`, file headers, `tools/build_manifest.py`.
8. **Personal data scan**: `python3 tools/scan_personal_data.py --private <your private names file, kept OUTSIDE the repo>` → must print `CLEAN`.
9. **Hashes**: bump `files/custom_nodes/Saya_Couple/VERSION.txt`, then `python3 tools/build_manifest.py`.
10. **README**: recommended gain, tested base, compatibility statuses, nothing marked available that is not.
11. Only then: tag and publish.

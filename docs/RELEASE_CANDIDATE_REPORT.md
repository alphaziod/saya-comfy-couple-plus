# Saya Couple 0.1.0: release candidate report

Status: **release candidate, NOT published**. No push, no remote, no tag, no GitHub release.
Date: 2026-09-27.

## 1. Package

- **Files:** 126 distributed files + MANIFEST.json (all listed in `MANIFEST.json` with sha256, origin, license and component; `MANIFEST.json` itself excluded).
- **Size:** 3.27 MiB (3425244 bytes).
- **Components:** amd_optional 2, docs 10, installer 15, saya_core 2, saya_node 86, test 6, third_party_optional 1, workflow 4.
- **Unexpected or unreferenced files:** none. `__pycache__`, backups and install records are excluded by `.gitignore`.

```
README.md  LICENSE (GPL-3.0)  LICENSES/{GPL-3.0,AGPL-3.0}.txt  THIRD_PARTY_NOTICES.md
MANIFEST.json  compatibility.json  .gitignore  saya  saya.bat
files/comfy/ldm/modules/attention.py        reference: upstream 41db8f4f + required patch (never copied blindly)
files/comfy/model_management.py             reference: upstream 41db8f4f + optional AMD patch
files/custom_nodes/Saya_Couple/             node pack, 86 files (src 47, web 5, tests 27, core_patch 1, 6 top-level)
patches/saya_dual_attention.patch           REQUIRED
patches/amd_vram_safety.patch               OPTIONAL (AMD / ROCm)
patches/third_party/ultimatesdupscale_saya_couple_crop.patch   optional, not installed, transparency only
installer/saya.py  installer/saya_installer/{cli,compat,gpu,patchlib,smoke}.py
workflows/Saya_Couple_Demo.json  Saya_Couple_Demo_api.json  Saya_Couple_Full.json  demo_prompt.json
tests/{proof_gain,installer_scenarios,live_demo_check,run_campaign,contact_sheet}.py  tests/themes_v1.json
tools/{build_manifest,build_demo_workflow,build_full_workflow,scan_history,scan_personal_data}.py
docs/{DEVELOPMENT_HISTORY,RELEASE_CHECKLIST,RELEASE_CANDIDATE_REPORT}.md  docs/images/demo_example.png
```

## 2. Licenses

| Part | License | Notes |
|---|---|---|
| Saya original code (pack, installer, tests, tools, docs) | **GPL-3.0-or-later** (`LICENSE`) | main license of the repository |
| Core patches and patched reference files | **GPL-3.0** (ComfyUI) | modifications of ComfyUI |
| `src/ppm_vendor/**` (7 files) | **AGPL-3.0-or-later** | from pamparamm/ComfyUI-ppm `6c6c360`, `SPDX` header in every file |
| `common.py`, `unet_couple.py` | **AGPL-3.0-or-later**, **MODIFIED** | per-tile mask cropping; change notice at the top of each file and in `ppm_vendor/README.md` |
| `src/nodes/saya_attention_couple.py` | **AGPL-3.0-or-later** | adapted from the ComfyUI-ppm node; provenance and `SPDX` header |
| `patches/third_party/…usdu…patch` | **GPL-3.0** | modification of ssitu/ComfyUI_UltimateSDUpscale |

- AGPL files stay AGPL. Section 13 of GPLv3 and AGPLv3 allows the combination, and nothing is relicensed.
- The upstream ComfyUI-ppm files had no header of their own, so nothing was lost.
- `README.md` (Licenses table), `LICENSES/` and `THIRD_PARTY_NOTICES.md` say the same thing.

## 3. External dependencies (not included)

| Dependency | Status | License | Used for |
|---|---|---|---|
| **MultiMaskCouple** (tumbowungus) | required | GPL-3.0 | the pack |
| **RES4LYF** (ClownsharkBatwing) | required by both workflows | AGPL-3.0 **+ commercial-service restriction** | Shark samplers; restriction documented |
| Ultimate SD Upscale, Impact Pack | full workflow only | GPL-3.0 | |
| Impact Subpack | full workflow only | AGPL-3.0 | |
| GGUF | full workflow only | Apache-2.0 | |
| KJNodes | full workflow only | GPL-3.0 | |
| rgthree | full workflow only | MIT | |
| LoRA Manager | full workflow only | GPL-3.0 | |
| Fearnworks | full workflow only | Apache-2.0 | |
| DaSiWa | full workflow only | Apache-2.0 | |
| JPS | full workflow only | see its repository | |
| EasyColorCorrector | full workflow only | MIT | |

All links and licenses are in `THIRD_PARTY_NOTICES.md`.

## 4. Core modifications

Exactly two ComfyUI files differ from upstream `41db8f4f`.

| File | Change | sha256 (upstream → patched) |
|---|---|---|
| `comfy/ldm/modules/attention.py` | REQUIRED: Saya forced attn2 path, `main_locked_delta`, `SAYA_LOCKED_DELTA_PERSON_GAIN = 0.78`, fail-closed hook checks; identical to upstream without the flag | `9cafaafa…2960` → `48dfedc4…dbc` |
| `comfy/model_management.py` | OPTIONAL AMD / ROCm: VRAM cap (free − 1 GiB), full unload, no re-promotion of partially loaded models | `2995ba87…fad` → `ad00a433…cf` |

## 5. AMD patch policy (tested)

| GPU | Behaviour | Scenario |
|---|---|---|
| NVIDIA | never offered | E1 |
| AMD ≥ 24 GB | information only, not applied (`saya amd --yes` to force) | F1 |
| AMD 16–24 GB | recommended, asks (default yes) | G1 |
| AMD ≤ 16 GB | strongly recommended, asks (default yes) | J1, and real 16 GB GPU |
| any AMD | the user can refuse, install still OK | K1 |

`saya amd --check | --yes | --revert`: backup before change, idempotent, compatibility checked first.

## 6. Compatibility

- **OFFICIALLY TESTED:** ComfyUI `41db8f4f` (v0.34.0+77), Linux, AMD RX 9060 XT 16 GB (RDNA4), ROCm 7.13, PyTorch 2.13, Python 3.12, PyTorch attention. The rule requires 10 key core files to be byte-identical to that commit.
- **POTENTIALLY SUPPORTED:** v0.23.0 → v0.37.4 (static capability checks pass on all three axes).
- **UNSUPPORTED** (the installer refuses and writes nothing):
  - v0.3.69 → v0.22.x: the pack uses APIs added later (`model_management.unload_model_and_clones`, `model_base.Anima`);
  - ≤ v0.3.59: attention backends don't accept `transformer_options`.
- Full scan: `compatibility.json` (45 releases).

## 7. Install / verify / restore (clean copy of the officially tested ComfyUI)

- **Manual sequence:**
  - `check-compat` → OFFICIALLY TESTED ×3;
  - `install` → OK;
  - `verify --full` → RESULT OK, pack tests 96 pass / 0 fail / 5 skip (models not present);
  - second `install` → idempotent (0 files copied, original backup kept);
  - `restore` → core byte-identical to upstream (`9cafaafa…`, `2995ba87…`), **0 tracked git changes**, custom node removed.
- **Scenarios:** `tests/installer_scenarios.py` **23/23 PASS**. They cover:
  - fresh install, verify, idempotence, restore, reinstall;
  - drift detected before overwrite, and `--force`;
  - core already modified → refused, nothing written;
  - MultiMaskCouple missing → refused;
  - NVIDIA / AMD 24 / 20 / 12 GB simulations, and user refusal;
  - `--dry-run`;
  - v0.22.3 → UNSUPPORTED, nothing written.
- **Gain proofs:** `tests/proof_gain.py` **ALL PASS**:
  - g = 1.0 bit-identical to the pre-gain formula (fp32 / bf16 / fp16);
  - only D1/D2 are scaled;
  - MAIN / P1 / P2 inputs are independent of g;
  - uncond rows are exact;
  - flag absent == native.
- **Pack suite:** 98 / 0 / 3 against the maintainer's install; 96 / 0 / 5 on a clean install.

## 8. Demo workflows

- **Simple demo** (`Saya_Couple_Demo.json`):
  - contents: 2 loaders (`SELECT_YOUR_MODEL` / `SELECT_YOUR_REFINER_MODEL`), MAIN/P1/P2/NEG, SplitMask, Multi Couple (dual ON), Epsilon 1.003 → CFGZeroStar → APG → PAG, DetailBoost, Shark 1 + Shark 2, decode, save, shared random seed, and a note "SELECT YOUR SDXL / ILLUSTRIOUS MODEL HERE — THEN CLICK GENERATE";
  - tested in the real frontend on a clean install: no missing node types; selecting two models changes **only `ckpt_name`**; Shark dual core vs the validated workflow **27/27 parameters identical, functional Shark parameters changed = 0**; one generation succeeded (48 s);
  - `docs/images/demo_example.png` was regenerated and saved **without metadata**.
- **Full workflow** (`Saya_Couple_Full.json`):
  - the complete 6-phase pipeline; same 21 graphs, same node ids / types / modes, **identical links**;
  - only models, VAEs, LoRA stacks (emptied), detector models, prompts, imprint file name, private notes and the old `dual_attention_enabled` value (0.5 → True) changed;
  - the 13 detailer slots are named Detailer 01–13 (titles, labels, sockets, rgthree toggles, hub ids renamed consistently);
  - the frontend `widgets_values_named` shadow copy was removed, because it would have restored the private values;
  - loaded in the real frontend: 89 nodes, **no missing node types**; phase-1 Shark samplers identical to the validated workflow;
  - not re-run end-to-end in its public form (its sanitised models are placeholders).
- **Prompts:** both workflows use the gain-campaign prompt. Only the negative was reworded to neutral quality terms.

## 9. Privacy scan

`tools/scan_personal_data.py --private <file outside the repo>` → **CLEAN**. It checked every text file and PNG metadata of the candidate for:
- local paths, e-mails, tokens / API keys / private keys, assistant scratch paths;
- 58 private names (hostname, personal checkpoints / VAEs / LoRAs / detector files, old workflow names, private prompt terms);
- explicit wording in public-facing files (README, docs, workflows, notices, themes).

Public component names that remain on purpose: HiDream text encoders and GGUF, SAM, RealESRGAN.

## 10. Neutral presentation

- Hub labels: Detailer 01–13. Demo, docs and README contain no NSFW wording, model, preset or image.
- Internal detailer identifiers in `src/services/image_phases.py` are kept unchanged, for compatibility with existing phase checkpoints and workflows.

## 11. Known limitations / not tested

- **Real Windows and real NVIDIA: NOT tested** (only static checks and an NVIDIA simulation). `saya.bat` has never run.
- Only one hardware / software combination validated with real generations.
- Full workflow not re-run end-to-end after sanitising.
- Core patch can break on ComfyUI updates (run `saya verify`); SDXL/UNet only; fail-closed with other attn2 hooks.
- Attribute swaps between the two characters can happen; hard themes can drop a third element.
- `g` is a code constant (restart to change).
- `tests/live_demo_check.py` needs `playwright` (test-only dependency).

## 12. Maintainer's active install (not modified in this gate)

- Identical to archive `_ARCHIVE_BASE_G078_2026-09-27`; `g = 0.78`.
- Documented residue, left in place on purpose: `custom_nodes/Saya_Couple_Upated/tools/` (`check_comfyui_compat.py`, `core_patches/`), from an earlier session. Inert; to clean separately.

## 13. Ready for publication

- **Ready:** package contents, licenses and notices, installer (install / verify / restore / check-compat / amd), both workflows, tests, docs, privacy scan.
- **Before publishing (maintainer's decision):**
  - create the repository;
  - optionally run the release checklist once more;
  - decide whether the untested Windows / NVIDIA paths are acceptable as "potentially supported".

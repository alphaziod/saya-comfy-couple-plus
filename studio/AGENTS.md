# AGENTS.md — Saya Studio

Read this first if you are an AI agent or a developer arriving with no context. It explains what the
project is for, how to build and test it from an empty machine, where things live, and the rules that
protect the owner's data. User-facing text in the app is French by default (14 languages); code,
comments and developer docs are English.

**Language rule for agents: this file is in English, but always answer the user in the language
they write in** (the original owner writes French).

**Before changing anything for a new user, run the questionnaire in §11** and write the answers
down (§11 says where). Most wrong turns come from guessing what the user wants.

## 1. What Saya Studio is

A **local desktop application** (Linux first, Arch/EndeavourOS package) for making anime-style
character images with **ComfyUI** as the external render engine. Nothing runs in the cloud.

The app is the interface; ComfyUI only executes graphs. Saya Studio:

- **Casting** — asks a separate *draw engine* (Python scripts, see §4) for N original characters
  (OCs): identity, outfit, pose/action, background. Renders **pass 1 only** for each one, lets the
  user keep or drop them in a grid (or one by one: *Une par une*), then sends kept ones through the
  full **six-pass "Saya Couple" pipeline** (passes 2–6: upscale, detailers, HiDream, naturalize).
- **Drafts (Brouillons)** — the user writes a new action, tests it in the casting from an isolated
  copy of the draw engine, then says **Yes** per action to add it to the real engine.
- **Gallery** — indexes image folders (incremental, SHA-256, ComfyUI PNG metadata), filters by
  checkpoint/LoRA/character, favourites, review, duplicates; organizes deliveries into
  `~/Images/saya studio/<day>/pull/<family>/` and finals into `<day>/pull/Final/`.
- **Characters** — sheets with revisions, variants, references, portraits.
- **Models / LoRA manager**, **graph editor** (native ComfyUI graph editing), **workflows**,
  pipelines, A/B lab, scenes, prompt engine.

Product principles (non-negotiable):

- Saya Studio is autonomous; ComfyUI is the external engine, not the UI. Never write a second
  inference engine.
- No approximate conversion presented as faithful: diagnose, refuse precisely, keep the original.
- The owner's data comes first (§6).
- Distinguish **simulation** (stub engine, mock ComfyUI), **native validation** (real app window,
  fake engine) and **real engine** (GPU). Never claim "validated on the real engine" without an
  authorized real run.
- Features are listed in `README.md`; none may be dropped silently.

## 2. Build and run from an empty machine

Requirements (Arch names, from `packaging/arch/PKGBUILD`): build `rust`, `nodejs>=22.18`, `npm`;
run `webkit2gtk-4.1`, `gtk3`, `libsoup3`, `wayland`, `python`, `curl`, `xdg-utils`,
`xdg-desktop-portal` (+ a portal backend such as `xdg-desktop-portal-gtk`). Optional: `ffmpeg`
(LoRA video previews).

```bash
npm ci
npm run tauri -- dev                     # development window
npm run build && cargo build -p saya-studio --features custom-protocol   # standalone debug binary
./target/debug/saya-studio
```

Arch package: `bash scripts/build-arch-worktree.sh`, then `python3 scripts/verify-arch-package.py`
(details: `docs/PACKAGING.md`, end-user install: `docs/INSTALLATION-ARCH.md`).

The app starts without ComfyUI and without the draw engine; those pages then say what is missing.
To generate images you need:

1. **ComfyUI** (default `~/AI/ComfyUI`, override `SAYA_COMFYUI_DIR`), listening on `127.0.0.1:8188`.
2. The **Saya Couple nodes** 2.3.0+ from this repository (`files/custom_nodes/Saya_Couple`, installed by
   `./saya install` into `ComfyUI/custom_nodes/`; phase nodes
   `SayaImageGenerationReview`, `SayaImagePhase<N>Stop`, `SayaFamilySave`; they write the PNG chunks
   `saya_family` and `saya_role` that the gallery reads).
3. The checkpoints/LoRAs named by the presets in `presets/passes/`.
4. For Casting and Drafts: the **draw engine** (§4).

## 3. Repository map

| Path | What |
|---|---|
| `crates/studio-core/` | All domain logic (Rust, no Tauri). `Database` = SQLite connection + `impl` blocks per module. Test with `cargo test -p studio-core`. |
| `src-tauri/src/lib.rs` | Tauri shell: `AppState`, the single IPC command `studio`, `enum Request` (serde `tag="action"`, snake_case), startup tasks, smoke hooks. |
| `src/` | Svelte 5 (runes) + TypeScript UI. Never touches files/SQL/ComfyUI directly: everything goes through `call(action, args)` in `src/lib/api.ts`. |
| `src/locales/*.json` | 14 locales; the French text is the key. See `docs/I18N.md`. |
| `migrations/` | SQLite migrations (`0001`…`0031`), embedded with `include_str!`; version in `storage.rs` `VERSION`. |
| `presets/` | Base workflows: the six-pass Saya preset (`presets/passes/ILUSTCOUPLE_*`). Installed to `/usr/share/saya-studio/presets` (override `SAYA_STUDIO_PRESETS`). |
| `scripts/` | Native smokes (`*-native-smoke.{sh,js}`), mock ComfyUI, stub engines, fixtures, packaging, i18n tools. |
| `packaging/arch/` | `PKGBUILD`. |
| `agent-skills/` | Prompt skills for agents (SDXL anime, Anima, house rules), installed on request (§9.10). |
| `docs/` | Developer docs (English): start with `docs/README.md` → `ARCHITECTURE.md`, `GENERATION_LIFECYCLE.md`, `PERSISTENCE.md`, `TESTING.md`, `I18N.md`, `PACKAGING.md`. Dated French files are work logs. |
| `test-results/` | Output of tests and benches (not product code). |

Key modules in `crates/studio-core/src/`: `saya.rs` (six-pass runs, run manifests
`pass-states/saya-runs/<run>.json`), `casting.rs` (batches, history, publish, `casting_previews`),
`lineage.rs` (one P1 / one final per lineage, preview cleanup at startup), `gallery.rs`
(indexing, filters, delivery organization, `saya_role`), `engine_runtime.rs` (draw engine
subprocesses), `actions_bank.rs`, `action_drafts.rs`, `jobs.rs`, `workflow.rs`, `storage.rs`.

## 4. External pieces and environment variables

The **draw engine** ("GROK BUILD", the maintainer's private Python project) is *not* distributed; it lives in the user's config folder when they have it. Studio calls it as
a subprocess: `python3 test_solo.py --casting N [--famille F] [--actions a,b] …` writes a catalogue
(`sortie/casting/catalogue.json`) and never sends images itself. Its maintenance CLI
`solo_cli.py` (`action add`, `sandbox create`, `rollback`) is used by Drafts. Without it, use the stub
`scripts/casting-stub-engine.py` (what all tests do).

| Variable | Default | Purpose |
|---|---|---|
| `SAYA_CONFIG_DIR` | `~/Documents/saya studio config` | The owner's folder: draw engine, workflows, settings (layout in its `INDEX.md`) |
| `SAYA_ENGINE_DIR` | `<config>/GROK BUILD/generez`, else `~/Documents/GROK BUILD/generez` | Draw engine directory |
| `SAYA_COMFYUI_DIR` | `~/AI/ComfyUI` | ComfyUI install (models, custom nodes) |
| `SAYA_COMFY_OUTPUT_DIR` | ComfyUI `output/` | Where ComfyUI writes; read by previews and cleanup |
| `SAYA_COMFY_WORKFLOWS` | ComfyUI user workflows | Workflow browser |
| `SAYA_STUDIO_PRESETS` | `<config>/workflows` if it has `preset.json`, else `/usr/share/saya-studio/presets` | Base presets (six-pass workflows) |
| `XDG_DATA_HOME` / `XDG_PICTURES_DIR` | `~/.local/share`, `~/Images` | App data (`studio.saya.desktop/`), capture root |
| `SAYA_STUDIO_TRACE=1` | off | Logs each IPC action, duration and error on stderr |
| `SAYA_STUDIO_SANDBOX` | set by smokes | Blocks image writes outside the isolated test dirs |

Personal paths must never be hard-coded: always a variable, a preference, autodetection, or a
folder picker (`crates/studio-core/src/paths.rs`).

**The config folder** (`~/Documents/saya studio config`, its `INDEX.md` describes it): `GROK BUILD/`
(the draw engine, if present), `workflows/` (base presets),
`parametres/preferences.json` (lasting app settings, mirrored by `user_config.rs` on every change and
read back by a fresh install). Copying this folder to another machine reproduces the setup. Tests
never write there: the mirror is off unless the app calls `user_config::enable()`, and off in sandboxes.

## 5. Tests

```bash
cargo test -p studio-core                 # ~450 tests, no GPU, no network
node --test scripts/*.test.mjs            # IPC contract, frontend logic, i18n parity
npx svelte-check                          # types (also run by npm run build)
npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
bash scripts/ui-tour-native-smoke.sh      # real window, fake engine, mock ComfyUI
bash scripts/action-drafts-native-smoke.sh
bash scripts/chaos-native-smoke.sh        # key spam + 2000 random actions
```

Native smokes run the real app in isolated XDG folders under `test-results/<name>/`, with
`scripts/mock-comfy.py` and a stub engine; reports in `test-results/<name>/<run>/`. Run each smoke
**once** per change; fix flaky ones at the source instead of re-running in loops.
Real GPU checks are never automatic: they need ComfyUI started by the owner, explicit approval,
SFW prompts and fresh seeds.

## 6. Rules that protect the owner (non-negotiable)

- Never modify or delete the owner's data without explicit permission: the live database
  (`~/.local/share/studio.saya.desktop/`), `~/Images`, ComfyUI workflows/models/LoRAs/config, the
  draw engine and its data, system configuration. Read-only unless asked.
- Tests use synthetic, **SFW** fixtures only; never copy personal images, characters or prompts
  into tests or commits.
- Deletions: exact paths only, never glob patterns in the owner's folders; prefer the trash; journal
  what was removed. Temporary previews are deleted physically by the app's own cleanup, by rule.
- ComfyUI is started and restarted by the owner (`fish -c comfy` on their machine); ask before
  killing or restarting it; one GPU job at a time.
- Never `pkill -f` with a pattern that can match your own shell.
- Do not commit, push, merge branches, install packages or touch the GPU without being asked.

## 7. Conventions when changing code

- IPC action and field names are a contract with the UI and the smokes: rename both sides or not at all.
  A field named `action` would clash with the serde tag.
- Database: new schema = new migration file + `VERSION` bump + `contracts/bootstrap-v1.json`,
  `scripts/installed-app-smoke.sh`, migration tests. A transaction that reads before it writes must be
  `TransactionBehavior::Immediate` (a deferred one fails at once with "database is locked" under
  concurrent writers).
- UI strings: write French in `$t(...)`, add the key to all 14 locales (`docs/I18N.md`), run
  `node --test scripts/i18n.test.mjs` (fails on missing, unused or altered keys).
- Every UI action guards against double submission (`if (busy) return`).
- Comment density and naming follow the surrounding code; English identifiers.
- After a change: run the matching tests, then one native smoke if the UI path changed; record the
  outcome (what passed, what was not verified) in the resume file below.

## 8. Where the work stands

- Release notes: `CHANGELOG.md` at the repository root (Saya Couple + Saya Studio) and the GitHub releases
  (`studio-v*` tags).
- Keep your own resume file `RESUME.local.md` at the root of `studio/` (git-ignored): what is done, what is
  open, decisions, exact paths of backups and test runs. The next agent starts from it.

## 9. How-to for agents

There is no external HTTP/MCP API yet (`docs/API.md`, "future contracts"). Everything goes through
the app's single IPC command `studio` (`call(action, args)` in `src/lib/api.ts`, `enum Request` in
`src-tauri/src/lib.rs`). An agent therefore works in one of three ways: (a) the app's UI, driven by
the user; (b) a scripted run of the real app window with `scripts/v12-native-smoke.py --script
your.js` (the script calls `api('action', {...})` and clicks the DOM; isolated data, mock or real
ComfyUI); (c) editing files the app reads (presets, the draw engine's action bank) with consent.
The page **Atelier → Agents & API** only explains this; the user does almost nothing there.

### 9.1 Convert a normal (UI) ComfyUI workflow to API, and build a pipeline

1. Start from the workflow saved by the ComfyUI frontend (UI format, with `nodes`/`links`), not a
   hand-edited API file. ComfyUI must be running so the app can read `/object_info`.
2. Import it (Atelier → Workflows, or Génération → open file). The app converts it with
   `workflow_ui_api::convert_ui`, a faithful port of the ComfyUI frontend's `graphToPrompt`
   (subgraphs expanded, mute/bypass applied, hidden widgets such as `control_after_generate` and
   rgthree buttons never serialized). Each run returns `{api, report}`.
3. **Only use the result if `report.complete == true`.** Anything unresolved is listed in the report
   and left out; never "fix" it by inventing values. "Vérifier l'API" re-checks against
   `/object_info`. ComfyUI's own "Export (API)" is a cross-check, not the source of truth.
4. Multi-pass workflows (the Saya preset) are split per phase (`workflow_split.rs`,
   `saya::split_phase_source`, `convert_ui_phase`): one API graph per pass, run one after the other.
5. A **pipeline** (Pipelines page, `orchestrator.rs`) chains separate API workflows; graphs are never
   merged. Manifest (`workflow_pipeline.rs`, format `saya-workflow-pipeline`, version 1):

   ```json
   {"format":"saya-workflow-pipeline","version":1,"id":"<uuid>","name":"Base then upscale",
    "workflows":[{"id":"<base revision id>"},{"id":"<upscale revision id>"}],
    "steps":[
     {"id":"base","workflowId":"<base>","kind":"image","outputs":[{"name":"image","portType":"IMAGE"}]},
     {"id":"upscale","workflowId":"<upscale>","kind":"image",
      "inputs":[{"name":"image","portType":"IMAGE"}],"outputs":[{"name":"image","portType":"IMAGE"}],
      "bindings":[{"port":"image","nodeId":"1","input":"image","source":{"step":"base","port":"image"}}]}]}
   ```

   Workflows must be imported first (their revision ids); `save_pipeline` refuses unknown or changed
   workflows (optional `sourceSha256` pin) and LATENT transport (only IMAGE passes between steps:
   each output is saved, hash-checked and uploaded to ComfyUI as the next step's `LoadImage`).
   Each pass is an ordinary job (prepare → confirmed submit → sync); a resume restarts a pass, never
   a sampler mid-way. Tests: `crates/studio-core/tests/orchestrator.rs`.

### 9.2 Replace a node in the graph editor

There is no single "replace" operation; it is three edits on a graph session, undoable as usual
(`graph_session::EditOp`, sent with `graph_edit {session, op}`):

1. `{"op":"add","classType":"NewClass","x":..,"y":..}` — the class must exist in `/object_info`.
2. `{"op":"connect","fromNode":"..","output":0,"toNode":"..","input":".."}` for every input and
   every downstream consumer of the old node; `{"op":"set_param","node":"..","input":"..","value":".."}`
   to copy widget values that still apply (check types and ranges; never coerce silently).
3. `{"op":"remove","nodes":["<old id>"]}`, then `graph_prepare_api` and read its report.

Inside a subgraph use `graph_enter_subgraph` / `graph_exit_subgraph {commit:true}`. Save the UI
version with `graph_save_ui` and the API version with `graph_save_api`; the user's original files are
never overwritten (a copy is saved).

### 9.3 Create a casting

A casting asks the draw engine (§4) for N characters, renders **pass 1 only** for each, and lets the
user keep some for the full six passes. In the UI: Génération → open the Saya preset (master +
six passes, `presets/passes/`), set it up, then Casting → "Lancer". By IPC:
`casting_lancer {graine:"<u64 as string>", nombre:N, source_path:"<preset file>", famille?,
subfamily?, genre?, persos?:[ids], actions?:[names], exclude?:[names], perso?+pose_fixe?
(rotations), all_actions?, step?, drafts?}`. Then `casting_history`, `casting_decide {run, keep:true}`
(launches passes 2-6 on that trial), `casting_publish`, `casting_enregistrer_perso` (saves a
character sheet: P1 + anatomy; the background becomes a variant; no action; a clothed or caged trial
gets its full anatomy rebuilt with a note), `casting_delete_lot`. Trials are queued one at a time
(deferred jobs), never flooded into ComfyUI. Seeds are fresh for every real test.

### 9.4 Add prompts (actions) to the casting

Casting prompts are the draw engine's **action bank** (`<engine>/data/actions/<family>.actions.json`,
order in `_ordre.json`, fields in `schema.json`; an action is mostly `{"add": "<tags>",
"valide": true}`). Two routes, both through the engine's own CLI `solo_cli.py`:

- **Drafts (preferred, reversible):** Casting → Brouillons, or `action_draft_save {id, famille,
  entry}` → the action goes into an isolated *copy* of the engine; `casting_lancer {drafts:true,
  actions:[ids]}` tests it; only the user's explicit "Oui → pull réel" (`action_draft_promote`) runs
  `solo_cli.py action add --apply` on the real engine, with a backup and a printed rollback command.
- **Directly in the engine:** only with the owner's consent: `python3 solo_cli.py action add
  --famille F --id ID --file action.json --dry-run --json`, then `--apply --yes`.

Prompt style that works with this engine: short Danbooru tags in the order of the act, weights
around 1.1-1.3, one object per tag, no long sentences. Never edit the user's existing characters or
prompts to make a test pass; add your own.

### 9.5 GPU load: one API workflow at a time

The app sends **one ComfyUI job at a time** (one Saya pass, one casting trial, one pipeline step;
`engine_operations` / `saya_operations` locks; casting trials are deferred). A whole workflow can run
as a single API graph only when its peak VRAM fits the card (e.g. one SDXL/Illustrious checkpoint
plus small LoRAs). Heavy chains (several checkpoints, HiDream/Flux-class models, big upscales) must
be **split into passes** with memory released between them. `vram_policy.rs` plans this:
two SDXL-class checkpoints may stay resident; a third model or a large architecture
(flux, hidream…) unloads first. If you see OOM, split further, or ask the user whether they want to change
their own settings; do not change them yourself.

### 9.6 Raw VRAM flush (how we do it)

ComfyUI's `POST /free` only *queues* a flag; its worker applies it at the next prompt. The app's
forced flush (`comfy::force_unload_models(min_free_mib)`):

1. `POST /free {"unload_models":true,"free_memory":true}`;
2. submit a tiny prompt (`EmptyImage 64×64` → `PreviewImage`, client id `saya-memory-flush`) and
   wait for it in `/history` so the worker runs and applies the flag;
3. `POST /free` again;
4. poll `/system_stats` until `devices[0].vram_free` ≥ the minimum (e.g. 12 000 MiB), else report it.

Measured release (`resources::release_measured`) records before/after Torch reservations and only
claims a release when they actually drop (`docs/RAM-VRAM.md`). Never kill or restart ComfyUI to free
memory without asking the owner.

### 9.7 Create characters

**The rule:** a character sheet is **P1 (who she is) + anatomy**; each **variant is P1 + anatomy +
one background (BG)**. Never put an action, a pose, P2, quality tags or a negative in a character.
Adults only (`adult`, `adult woman`… in P1).

- **P1 (identity):** subject count, adult age descriptors, hair colour and cut (weighted once),
  eyes, facial features, skin tone, build, chest. Clothing only when it is a permanent part of the
  character; otherwise the outfit comes from the generation.
- **Anatomy block:** the external male genitalia only, in the engine's clinical order
  (`moteur/anatomie.py anatomy_for`): penile size (tiny / small / medium), penile state (erect or
  flaccid), testicular size matching the penile size, scrotum, and pubic hair matching the scalp
  hair colour. Breasts, nipples and the rest of the body are not anatomy: they stay in P1. A
  character shown with a device or clothing over the genital area keeps her normal size in the
  sheet; the device or garment belongs to the action.
- **In the app:** Personnages → new sheet, or by IPC `save_character {id:null, name, identity:
  {identity_prompt, anatomy_prompt, …}}`, then variants with `save_variant {character, id:null,
  name, blocks:{"P1":"<P1>, <anatomy>","BG":"<background>"}, constraints:{}}`. Attach reference
  images from the gallery (`associate`).
- **From a casting trial:** `casting_enregistrer_perso {run, stem, nom}` (§9.3); same trial saved
  twice = same sheet.
- **From the draw engine's bank:** Personnages → import GROK (`grok_preview {paths}` then
  `grok_import {choices, confirmed:true}`): anatomy tags found in P1 move to the anatomy,
  `bg_options` become variants, a re-import never duplicates.
- **Engine side (owner's process, do not edit for them):** the engine draws original characters
  (`moteur/generateur_perso.py`: shades, eyes, face signs, hair details…) or uses the bank
  (`characters_ready.json`, read-only). When the owner likes an image, `valider <image>` (fish) adds
  the character to `negatifs_figes.json` with the next id `CH--NNNN` (identity without clothing, anatomy
  rebuilt when the image only showed it covered). Family prefixes in ids/stems: `hum bny kmn slm lcs cat dem elf
  cute edgy hor ard div sop lra` (`registre_ids.py`).
- **Check before saving:** the hair colour is readable (it drives pubic hair), no duplicate tags,
  weights between ~0.8 and 1.3, nothing that contradicts the anatomy.

### 9.8 Create backgrounds (BG)

- **In the app** a background is a **variant** of a character (`BG` block, §9.7), or part of a
  scene (Scenes page, `save_scene {id, name, manifest}`: character + BG + Couple settings, resolved
  against the registered workflow mappings; it never edits graphs).
- **In the draw engine** (read `GROK BUILD/generez/docs/GUIDE_DECORS.md`): BGs are never written by a
  command. Sources: the generic stock of the Saya custom node
  (`ComfyUI/custom_nodes/Saya_Couple/data/backgrounds*.json`, ~8 700 entries, **read-only**),
  each bank character's 3 `bg_options` (read-only), the pole tables `data/backgrounds/pole.json` and
  cleanup rules `data/backgrounds/regles.json` (**owner-editable only**). The draw takes the
  character's 3 options + 1 outdoor from the stock, then cleans the text (`finish_background`).
  Inspect with `./solo bg list|show|stock search <q>|origin --seed N|check --json`.
- **Writing a good BG prompt:** start with `indoors` or `outdoors` (the engine reads it in the first
  180 characters), then the place weighted once (`(temple moss garden:1.2)`), 10+ concrete tags
  (objects, materials, time of day, light, weather, colours, depth of field), no people, no school
  settings, nothing a character could hold if the action uses her hands. Prefer 3/4 or side camera
  angles over a front view.
- To add BGs for real, write them as character variants in the app, or propose the text to the owner
  for `pole.json`/the stock; never write the stock yourself.

### 9.9 Essentials that are easy to miss

**Glossary.** *Pass/phase* 1-6 of the Saya workflow (1 = composition, 2-5 = upscale/detailers/
HiDream/naturalize, 6 = final). *Casting batch (lot)* = N trials; *trial (essai)* = one pass-1
image. *Lineage* = one trial's P1 + its final (only those two stay visible). *Pull* = engine
delivery folder (`<day>/pull/<family>/`); *Final* = finished six-pass image. *OC* = original
character. *P1/P2* = person 1/2 prompt (P2 only in Couple mode). *BG* = background. *Rotation* =
one character × N poses, or one pose × N characters.

**Backups and restore.** `saya-studio-maintenance backup DATABASE NEW_DIR [--include-pass-states]`,
`verify DIR`, `restore DIR NEW_DIR` (`docs/SAUVEGARDES.md`). Before any write to the owner's
database: `sqlite3 studio.sqlite3 ".backup '<backups/…>.sqlite3'"` + `pragma quick_check`, app closed.
Copying `~/Documents/saya studio config` moves the setup to another machine.

**Diagnose before fixing.** Run the app with `SAYA_STUDIO_TRACE=1` (each IPC call, duration, error).
A slow gallery is usually SQL reading the large inline `metadata_json` (hundreds of KiB of workflow
per image): keep filters on covered columns (migration 0031) and measure on a *copy* of the real
database. "database is locked" = a transaction that reads then writes without `Immediate`.
"Classe absente de object_info" = a custom node is missing in ComfyUI (ask the owner to install it).

**Verify a real render.** Read the PNG metadata (the gallery's detail panel, or the `parameters`,
`prompt`, `workflow`, `saya_family`, `saya_role` chunks): seed, checkpoint, LoRAs and the pass must
match what was sent. Do not open generated images to judge them (token cost, owner's content):
ask the owner, who judges visually.

**Keep the owner in the loop.** One question = one concrete example ("Tifa: yes or no?"). Report
what passed and what was *not* verified. Do not re-ask decisions already recorded in the resume file.

**Hand over.** Before your context or quota runs out, update `RESUME.local.md` (done, open,
decisions, exact paths of backups and test runs) so the next agent can start cold. Never leave work
"done" only in a terminal.

### 9.10 Prompt skills (install only if the user agrees)

`agent-skills/` holds ready-made skills (one folder each, `SKILL.md` with a name/description
header, the Agent Skills format):

| Skill | For |
|---|---|
| `sdxl-anime-danbooru-prompts` | SDXL anime checkpoints: Illustrious, NoobAI (eps / v-pred), Pony, Animagine — tag order, quality prefixes, negatives, debugging |
| `anima-danbooru-prompts` | The Anima family (CircleStone Labs / Comfy Org, Danbooru + natural language, Qwen3 encoder): files, `@artist`, score/safety tags |
| `saya-studio-prompt-rules` | This project's house rules: P1 + anatomy, variants with BG, actions, backgrounds, tests |

Ask the user first ("Do you want me to install the prompt skills for <model family>?"). Then copy
the folder, never edit it in place:

- Claude Code: the `skills/<skill>/` folder of its user config directory (`~/.claude`, all
  projects) or of the project's `.claude` directory (this project only).
- Codex and other agents that support skills: their skills folder (e.g. `~/.codex/skills/<skill>/`);
  otherwise read the `SKILL.md` before writing prompts.

Install only the families the user actually uses (questionnaire §11, question 2), plus
`saya-studio-prompt-rules`. Check the model card of the user's exact checkpoint: it wins over the
skill. Sampler, steps, CFG, scheduler and resolution are never prescribed: every machine reacts
differently, the user finds their own; ask and keep what they use.

## 10. Things that bite (read before acting)

- ComfyUI is started only by the owner (`fish -c comfy`); never launch `main.py` yourself.
- The draw engine (GROK BUILD) is the owner's code: read-only unless asked; import it with
  `python3 -B` so no `__pycache__` is written there.
- One native smoke run per change; a smoke failing on old expectations is fixed in the smoke, not
  re-run in a loop. Never `pkill -f` with a pattern that matches your own shell.
- Characters: a sheet is **P1 + anatomy**; a variant is **P1 + anatomy + its background (BG)**.
  No action, P2, quality or negative in a character. The anatomy block always states penile size and
  state, testicles and pubic hair colour (a covered genital area keeps its normal size).
- The gallery shows only enabled library sources; sources reached through a symbolic link are
  refused. Deliveries go to the capture root (`~/Images/saya studio/<day>/pull/<family>/`, finals in
  `Final/`); `results/` is only a buffer.
- No symbolic links in the owner's setup unless they ask; delete only exact paths.
- Never claim "validated on the real engine" from a stub or mock run (§1).
- Codex or other external models are launched by the owner; you provide the prompt.
- Sampling settings come from the workflow the user named as having the good configuration
  (questionnaire 5b): read them there, copy them where needed, never edit that file. If none was
  named, ask before generating anything.

## 11. First-contact questionnaire (ask before refining the app)

Ask these in the user's language, a few at a time, with a concrete example each (e.g. "Tifa: yes or
no?"), and record the answers in the resume file (`RESUME.local.md`, section "User profile") and,
for lasting settings, in the config folder (`parametres/`). Do not re-ask what is already recorded.

**Workflow and engine**
1. Will you use the app's six-pass Saya workflow, or your own ComfyUI workflow? If yours: send the
   UI-format file and say which nodes must stay untouched.
2. Which model family: Illustrious, NoobAI, Pony, SDXL base, Flux, HiDream, other? Which exact
   checkpoints and VAE?
3. Do you want the Saya Couple nodes (two people, P1/P2 regions) or solo images only?
4. GPU and VRAM (e.g. 16 GB AMD/ROCm, 24 GB NVIDIA)? Is ComfyUI on this machine, and how do you
   start it? Is it OK for the app to unload models between passes?
5. Where are ComfyUI, its `output/` folder, your LoRAs and your workflows?
5b. **Which workflow holds your good settings** (sampler, steps, CFG, scheduler, resolution, LoRA
    weights)? Give its exact file path. The agent reads the settings from that file, records the
    path, and never modifies it: every machine reacts differently, agents never impose settings.

**Characters and casting**
6. Do you use the draw engine (GROK BUILD) or another source of characters? Which families
   (e.g. bny, cute, hum…) matter most?
7. Original characters only, licensed characters, or both? Any you want excluded?
8. For a saved character: what must always be written (anatomy details, eye colour, outfit or nude
   by default)? Which backgrounds should become variants?
9. How many trials per casting batch, and do you sort them one by one or in a grid?

**Output and storage**
10. Where should finished images go (default `~/Images/saya studio`)? Keep pass-1 images, or only
    finals? How many temporary files may stay on the SSD?
11. Which image folders should the gallery index, and which must it never touch?

**Safety and content**
12. Content rules: SFW only, or adult content for adult characters only? Any hard limits (ages,
    themes) the app must enforce or refuse?
13. What must never be modified or deleted without asking (folders, files, the engine)?

**Way of working**
14. Language for the app and for answers? Short answers or detailed reports?
15. Should agents commit to git, install packages, or touch the GPU on their own, or always ask?
16. Real GPU tests: who launches them, with which prompts, and how many images per test?
17. What is the first thing you want improved, and what does "done" look like for you?


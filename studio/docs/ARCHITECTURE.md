# Saya Studio — architecture

Developer overview, current as of the final pass (2026-10-28). Start here, then read
`GENERATION_LIFECYCLE.md`, `PERSISTENCE.md`, `TESTING.md`, `I18N.md` and `PACKAGING.md`.
Dated notes in `docs/` (`CASTING_*`, `REPRISE*`, `*_2026-10-*.md`) are historical work logs, mostly in French.

## Processes and boundaries

```mermaid
flowchart LR
  UI[Svelte 5 UI<br/>src/] -->|invoke 'studio' {action, ...}| Shell[Tauri 2 shell<br/>src-tauri/src/lib.rs]
  Shell --> Core[studio-core crate<br/>crates/studio-core]
  Core --> DB[(SQLite<br/>studio.sqlite3)]
  Core --> Files[Data dir: pass-states, casting,<br/>checkpoints, caches]
  Core -->|HTTP + WebSocket, loopback only| Comfy[ComfyUI]
  Core -->|python3 subprocess| Engine[Draw engine scripts<br/>SAYA_ENGINE_DIR]
  Core -->|read / organize| Images[Capture root<br/>~/Images/saya studio]
```

- **`crates/studio-core`** — all domain logic. No Tauri dependency, testable on its own
  (`cargo test -p studio-core`). Entry type: `Database` (a `rusqlite::Connection` plus helpers);
  most features are `impl Database` blocks spread over modules.
- **`src-tauri`** — application lifecycle, window, dialogs, the IPC dispatcher and a few
  platform commands (file pickers, smoke-test capture). It resolves the data directory with the
  Tauri path API and owns `AppState` (DB handle behind a mutex, ComfyUI port, background monitors).
- **`src/`** — Svelte 5 (runes) UI. It never touches files, SQL or ComfyUI directly; every
  operation goes through `call(action, args)` in `src/lib/api.ts`.

## IPC contract

One generic command, `studio`, receives `{ request: { action, ...fields } }`.
`src-tauri/src/lib.rs` deserializes it into `enum Request` (serde, `tag = "action"`,
snake_case). Action names and field names are a stable contract with the frontend and the
native smokes: never rename them without migrating both sides.

- Long or blocking work runs on the async runtime's blocking pool, never on the UI thread.
- Disk and network I/O is done outside the database mutex where possible (wave A).
- Errors cross the boundary as `ApiError { code, message, retryable }`; the UI translates
  known messages through `src/lib/translate-now.ts`.
- `SAYA_STUDIO_TRACE=1` logs the server-side duration of each action on stderr.

Other Tauri commands: `get_bootstrap`, `pick_image_file`, `pick_workflow_file`,
`pick_data_files`, `pick_directories`, `engine_dry_run`; with the `smoke-test` feature only:
`smoke_actions`, `smoke_capture`, `finish_smoke`.

## Core modules (by area)

| Area | Modules |
|---|---|
| Storage | `storage` (open, migrations, backups), `backup`, `fsutil` (sha256, atomic write, path-under-root), `paths` (XDG/`$HOME` defaults) |
| Workflows | `workflow` (import, bindings, `prepare_job`, graph validation), `workflow/`, `workflow_split`, `workflow_ui_api`, `workflow_pipeline`, `graph_editor`, `graph_session` (editable graph, bypass), `modules` |
| Generation | `comfy` (HTTP client, submit, sync), `jobs` (controls), `live`, `live_transport`, `orchestrator` (pipelines, pass-by-pass runs), `pass_states`, `vram_policy`, `engine`, `engine_runtime` |
| Saya six-pass workflow | `saya` (runs, restart, review, phase jobs), `saya_preset` (preset phases, widget overlay), `couple`, `universal_prepare` |
| Casting | `casting` (batches of pass-1 trials, publish, delete), `casting_bridge.py`, `generez`, `grok`, `grok_scenes`, `actions_bank` |
| Gallery | `gallery` (index, filters, captures, delivery), `image_cleanup` (exact duplicates), `generation_metadata`, `generation_store`, `generation_storage`, `visual_search`, `semantic_search`, `library_sources`, `origins` |
| Characters & models | `characters`, `character_gallery`, `references`, `model_library`, `lora_inventory`, `checkpoints`, `civitai`, `downloads`, `asset_management` |
| Tools | `ablab`, `ablab_runner`, `comparators`, `scenes`, `editor`, `editor_sessions`, `proposals`, `recipes`, `generation_tree` (backend only), `image_restore` |

## Frontend structure

- `src/App.svelte` — shell: icon rail (`[data-nav=<id>]`), sub-tabs for the *Atelier*
  (workflows, graph, pipelines, modules, scenes, A/B lab, prompts) and *Persos*
  (characters, people) groups, engine status pill, help popover. Screens stay mounted and are
  hidden with `[hidden]`, so in-progress state survives navigation.
- `src/components/` — one component per screen plus shared pieces (`Icon`, `StreamGrid`,
  `RunProgress`, `AssetActions`, ...).
- `src/lib/` — API wrapper, i18n, state helpers, graph editor model, blur editor.
- `src/style.css` — design tokens and workbench primitives (`.wb`, `.wb-panel`, `.wb-center`,
  `.sec`, `.chip`, `.seg`, `.kv`, ...). Visual rules: `docs/UI_V1_DESIGN.md`.

## Invariants worth knowing before changing code

- Original workflows and user images are never rewritten; edits produce new files.
- A job's intent is persisted before it is POSTed; a lost response is reconciled, never resubmitted blindly.
- Two jobs with the same seed are not duplicates; only identical bytes are (`image_cleanup`).
- `SAYA_STUDIO_SANDBOX` confines every image write of a test run; smoke builds refuse to start without it.
- No personal path is compiled in; defaults derive from `$HOME`/XDG (`paths.rs`).

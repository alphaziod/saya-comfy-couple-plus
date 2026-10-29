# Persistence

## Locations

| What | Where | Notes |
|---|---|---|
| Data directory | Tauri `app_data_dir` → `~/.local/share/studio.saya.desktop/` | Everything below lives here unless stated |
| Database | `studio.sqlite3` | SQLite (bundled rusqlite), schema version 31 |
| Migration backups | `backups/migration-<db stem>-v<N>-<uuid>.sqlite3` | Taken before each migration, last 2 kept per database; older `studio-v*` files belong to the user and are never rotated |
| Saya run manifests | `pass-states/saya-runs/<run>.json` | Written atomically (temp file + fsync + rename); an unreadable manifest is skipped and reported, never fatal |
| Casting batches | `casting/<run>/` | Catalogue, per-trial outputs |
| Generation buffer | `results/` | Raw ComfyUI outputs before delivery |
| Phase checkpoints | `checkpoints/` | Verified/reviewed pass checkpoints |
| Cleanup journals | `pass-states/duplicate-cleanup/`, `organize-journal/` | `VACUUM INTO` backup before any bulk removal |
| Civitai token | `civitai-token` | Plain file, optional |
| Capture root | `~/Images/saya studio` by default | Preference `captureRoot`; `$XDG_PICTURES_DIR` honoured |

External paths default from `$HOME`/XDG (`crates/studio-core/src/paths.rs`):
`SAYA_COMFYUI_DIR` (default `~/AI/ComfyUI`), `SAYA_ENGINE_DIR`, `XDG_PICTURES_DIR`.

## Schema and migrations

- SQL files in `migrations/0001_…` to `0031_gallery_cover_capture.sql`, embedded with `include_str!`
  and applied in `storage.rs::migrate` inside one `IMMEDIATE` transaction.
- `PRAGMA user_version` is the schema version. A database newer than the app is refused
  (no downgrade); an unversioned non-empty database is refused (never adopted implicitly);
  `schema_migrations` row 1 must be `saya-studio-initial`.
- Tables are `STRICT`, foreign keys are on, JSON columns are validated with `json_valid`.
- Adding a migration: new `migrations/00NN_name.sql`, bump `VERSION` in `storage.rs`, add the
  `if version < NN` step, add a test that opens a database at the previous version.

Main tables: `generation_jobs` (+ `job_events`, `job_outputs`, `job_characters`, `job_origins`),
`assets` (+ `asset_characters`, `asset_models`, `image_features`), `characters`
(+ variants, references, history), `workflow_revisions`, `workflow_bindings`,
`workflow_modules`, `pipeline_*`, `ab_*` (A/B lab), `preferences`, `libraries`, `index_runs`.

## Preferences

`preferences(key, value_json)`; read with `preferences`, written with `set_preference`.
Keys are camelCase and part of the persisted contract (`language`, `captureRoot`,
`generationWorkspace`, ...). Renaming a key requires a read-compatible migration.

## Safety rules for code that deletes

- Delete only exact paths you computed, never patterns.
- Prove a duplicate by content hash, never by seed or name.
- Stay inside the expected root (`fsutil::ensure_within`); `image_cleanup` only touches
  `results/`, `casting/` and the capture root.
- Back up the database (`VACUUM INTO`) and write a journal before bulk removals.

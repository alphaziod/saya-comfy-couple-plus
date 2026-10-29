# Generation lifecycle

How one image goes from the *Create* screen to the Gallery. Code references are in
`crates/studio-core/src/` unless stated otherwise.

## 1. Prepare (no network)

1. The UI picks a workflow (imported ComfyUI API graph or a Saya six-pass preset) and edits
   bindings: prompts, characters (P1/P2), checkpoint, LoRA, seed, node bypass.
2. `prepare` / `prepare_couple` → `Database::prepare_job*` (`workflow.rs`) applies the edits to a
   copy of the graph, validates it against ComfyUI's node schema (`validate_graph`) and snapshots
   the characters used (`jobs.rs::snapshot_characters`).
3. The result is a `generation_jobs` row in state **`draft`** holding the exact request snapshot
   and its hash. Nothing has been sent yet; the UI shows the diff (removed nodes need explicit
   confirmation).

## 2. Submit (exactly once)

`submit` → `Database::submit_job` (`comfy.rs`):

1. Refuses unless the row is still `draft` and the snapshot hash matches what the user confirmed.
2. Moves the row to **`submitting`** and records a `submission_requested` event *before* POSTing.
3. POSTs `/prompt` with `client_id` and `extra_data.saya_studio_job_id` so the job can be found
   again in ComfyUI's queue/history.
4. Success → **`queued`** with `comfy_prompt_id`. Network error or lost response →
   **`submission_unknown`**: the job is reconciled later by looking for its id in the queue and
   history, never resubmitted automatically.

## 3. Follow (live + reconciliation)

- `live_transport::Monitor` keeps a WebSocket to ComfyUI for progress and an HTTP poll for the
  queue (HTTP health is not inferred from the socket).
- `Database::sync_jobs` (`comfy.rs`) reconciles every non-terminal job (`queued`, `running`,
  `submitting`, `submission_unknown`) with `/queue` and `/history`, moving it to `running`,
  `succeeded`, `failed` or `cancelled`. Lookups use `jobs_by_ids`, so batches larger than the
  latest 100 jobs keep syncing.
- `jobs.rs` holds explicit controls (cancel/interrupt) that only ever target a prompt owned by
  Studio — never ComfyUI's global queue.

## 4. Deliver

On success, outputs are copied into the data dir `results/` buffer, then
`Database::organize_delivery` (`gallery.rs`) places them under the capture root
(`~/Images/saya studio` by default, `preferences.captureRoot`), grouped by day: trials in `<day>/pull/<family>/`, finals in `<day>/pull/Final/`.
A render whose bytes already exist is not duplicated; the kept copy is re-hashed first so a new
render is never dropped in favour of a vanished file. The asset is indexed and appears in the
Gallery with its provenance (job, pass, seed, config).

## Saya six-pass runs

A Saya run is a sequence of six phase jobs described by a manifest
`pass-states/saya-runs/<run>.json` (`saya.rs::read_run` / `write_run`, written atomically with fsync).

- Each phase is prepared from the preset (`saya_preset.rs`); widget overlays and bypass flags
  (`saya_bypass`) are propagated to every phase graph.
- `saya_restart` writes the manifest *before* submitting the new phase and moves only
  intermediate jobs (phases 2–5, and 1 once 6 is done) to `retiredJobs`, so finals stay visible.
- The Gallery shows the final image when the run is complete, otherwise the pass-1 image.

## Casting batches

Casting runs many cheap pass-1 trials (`casting.rs`, Python helper `casting_bridge.py`).
A batch tracks `N/total`; an interrupted batch can be resumed ("finish the batch — X remaining").
Selected trials are published to the Gallery (`casting_publish`) without duplicating assets, can
be finished through the remaining passes, or the whole batch can be deleted (`casting_delete_lot`),
which removes only assets no other record uses.

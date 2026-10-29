-- v19 : exécution persistée de pipelines multipasses (manifeste GLM v1).
-- Migration strictement additive. Les états intermédiaires vivent dans le
-- stockage pass-states (fichiers vérifiés) ; ces tables n'en gardent que l'empreinte.
CREATE TABLE pipeline_definitions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    manifest_json TEXT NOT NULL CHECK(json_valid(manifest_json)),
    manifest_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE TABLE pipeline_runs (
    id TEXT PRIMARY KEY,
    pipeline_id TEXT NOT NULL REFERENCES pipeline_definitions(id),
    manifest_sha256 TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'ready' CHECK(state IN ('ready','running','paused','completed','failed')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE TABLE pipeline_run_steps (
    run_id TEXT NOT NULL REFERENCES pipeline_runs(id),
    step_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK(position>=0),
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','submitted','completed','failed','invalidated')),
    job_id TEXT REFERENCES generation_jobs(id),
    input_fingerprint TEXT NOT NULL DEFAULT '',
    output_kind TEXT CHECK(output_kind IS NULL OR output_kind IN ('image','latent')),
    output_sha256 TEXT,
    output_bytes INTEGER,
    message TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(run_id, step_id)
) STRICT;
CREATE TABLE pipeline_run_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL REFERENCES pipeline_runs(id),
    step_id TEXT,
    kind TEXT NOT NULL,
    detail_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(detail_json)),
    at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE INDEX pipeline_run_events_run ON pipeline_run_events(run_id,id);
INSERT INTO schema_migrations(version,name) VALUES(19,'pipeline-runs');

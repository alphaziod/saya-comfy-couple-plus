-- A/B laboratory (module 2): persistent experiment campaigns, preparation only.
-- These tables never submit a generation, never call ComfyUI and never start a server.
-- Cell images are references to existing provenance rows, never file copies.
CREATE TABLE ab_campaigns (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    workflow_revision_id TEXT NOT NULL REFERENCES workflow_revisions(id),
    workflow_name TEXT NOT NULL,
    workflow_source_sha256 TEXT NOT NULL,
    model TEXT NOT NULL DEFAULT '',
    loras_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(loras_json)),
    prompts_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(prompts_json)),
    base_seed TEXT NOT NULL CHECK(length(base_seed) BETWEEN 1 AND 20 AND base_seed NOT GLOB '*[^0-9]*' AND (base_seed='0' OR substr(base_seed,1,1)!='0') AND (length(base_seed)<20 OR base_seed<='18446744073709551615')),
    steps INTEGER NOT NULL CHECK(steps>0),
    cfg REAL NOT NULL CHECK(cfg>=0),
    sampler TEXT NOT NULL DEFAULT '',
    grid_limit INTEGER NOT NULL CHECK(grid_limit BETWEEN 1 AND 512),
    notes TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE TABLE ab_parameters (
    id TEXT PRIMARY KEY,
    campaign_id TEXT NOT NULL REFERENCES ab_campaigns(id),
    slot TEXT NOT NULL CHECK(slot IN ('model','loras','prompt_P1','prompt_P2','prompt_QUALITY','prompt_ACTION','prompt_BG','prompt_NEG','base_seed','steps','cfg','sampler')),
    kind TEXT NOT NULL CHECK(kind IN ('constant','variant')),
    label TEXT NOT NULL,
    position INTEGER NOT NULL CHECK(position>=0),
    UNIQUE(campaign_id,slot)
) STRICT;
CREATE TABLE ab_parameter_values (
    id TEXT PRIMARY KEY,
    parameter_id TEXT NOT NULL REFERENCES ab_parameters(id),
    value_json TEXT NOT NULL CHECK(json_valid(value_json)),
    position INTEGER NOT NULL CHECK(position>=0),
    UNIQUE(parameter_id,position)
) STRICT;
CREATE TABLE ab_cells (
    id TEXT PRIMARY KEY,
    campaign_id TEXT NOT NULL REFERENCES ab_campaigns(id),
    position INTEGER NOT NULL CHECK(position>=0),
    config_json TEXT NOT NULL CHECK(json_valid(config_json)),
    choices_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(choices_json)),
    UNIQUE(campaign_id,position)
) STRICT;
CREATE TABLE ab_cell_images (
    cell_id TEXT NOT NULL REFERENCES ab_cells(id),
    job_id TEXT NOT NULL REFERENCES generation_jobs(id),
    asset_id TEXT NOT NULL REFERENCES assets(id),
    node_id TEXT,
    output_index INTEGER CHECK(output_index IS NULL OR output_index>=0),
    output_kind TEXT NOT NULL CHECK(output_kind IN ('saved','preview','unknown')),
    PRIMARY KEY(cell_id,asset_id)
) STRICT;
CREATE INDEX ab_cell_images_job ON ab_cell_images(job_id,cell_id);
CREATE TABLE ab_cell_verdicts (
    cell_id TEXT PRIMARY KEY REFERENCES ab_cells(id),
    campaign_id TEXT NOT NULL REFERENCES ab_campaigns(id),
    favorite INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1)),
    best INTEGER NOT NULL DEFAULT 0 CHECK(best IN (0,1)),
    rank INTEGER CHECK(rank IS NULL OR rank>0),
    note TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
-- Manual verdicts only: one "best" per campaign and one cell per manual rank are database constraints.
CREATE UNIQUE INDEX ab_single_best ON ab_cell_verdicts(campaign_id) WHERE best=1;
CREATE UNIQUE INDEX ab_unique_manual_rank ON ab_cell_verdicts(campaign_id,rank) WHERE rank IS NOT NULL;
CREATE TABLE ab_presets (
    id TEXT PRIMARY KEY,
    campaign_id TEXT NOT NULL REFERENCES ab_campaigns(id),
    name TEXT NOT NULL,
    cell_id TEXT REFERENCES ab_cells(id),
    config_json TEXT NOT NULL CHECK(json_valid(config_json)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
INSERT INTO schema_migrations(version,name) VALUES(14,'ab-laboratory');

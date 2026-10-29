-- Immutable migration. Future changes belong in 0002, never in this file.
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
INSERT INTO schema_migrations(version,name) VALUES(1,'saya-studio-initial');

CREATE TABLE characters (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    identity_json TEXT NOT NULL CHECK(json_valid(identity_json)),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
    archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE TABLE character_variants (
    id TEXT PRIMARY KEY,
    character_id TEXT NOT NULL REFERENCES characters(id),
    name TEXT NOT NULL,
    prompt_blocks_json TEXT NOT NULL CHECK(json_valid(prompt_blocks_json)),
    constraints_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(constraints_json)),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0)
) STRICT;
CREATE TABLE libraries (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    root_path TEXT NOT NULL UNIQUE,
    platform TEXT NOT NULL CHECK(platform IN ('linux','windows','macos')),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1))
) STRICT;
CREATE TABLE assets (
    id TEXT PRIMARY KEY,
    library_id TEXT NOT NULL REFERENCES libraries(id),
    relative_path TEXT NOT NULL,
    content_sha256 TEXT,
    byte_size INTEGER CHECK(byte_size>=0),
    modified_at TEXT,
    availability TEXT NOT NULL DEFAULT 'unknown' CHECK(availability IN ('unknown','present','missing','unreadable')),
    metadata_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(metadata_json)),
    metadata_state TEXT NOT NULL DEFAULT 'unknown' CHECK(metadata_state IN ('unknown','read','absent','invalid')),
    UNIQUE(library_id,relative_path)
) STRICT;
CREATE INDEX assets_hash ON assets(content_sha256);
CREATE TABLE asset_characters (
    asset_id TEXT NOT NULL REFERENCES assets(id),
    character_id TEXT NOT NULL REFERENCES characters(id),
    role TEXT NOT NULL CHECK(role IN ('reference','depicted')),
    evidence TEXT NOT NULL CHECK(evidence IN ('human','embedded','sidecar','inferred')),
    PRIMARY KEY(asset_id,character_id,role)
) STRICT;
CREATE INDEX asset_characters_filter ON asset_characters(character_id,asset_id);

CREATE TABLE model_resources (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('checkpoint','lora','vae','upscaler','text_encoder')),
    name TEXT NOT NULL,
    family TEXT,
    content_sha256 TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(metadata_json))
) STRICT;
CREATE TABLE asset_models (
    asset_id TEXT NOT NULL REFERENCES assets(id),
    model_id TEXT NOT NULL REFERENCES model_resources(id),
    evidence TEXT NOT NULL CHECK(evidence IN ('human','embedded','sidecar','inferred')),
    PRIMARY KEY(asset_id,model_id)
) STRICT;
CREATE INDEX asset_models_filter ON asset_models(model_id,asset_id);
CREATE TABLE lora_groups (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE) STRICT;
CREATE TABLE lora_group_members (
    group_id TEXT NOT NULL REFERENCES lora_groups(id),
    model_id TEXT NOT NULL REFERENCES model_resources(id),
    PRIMARY KEY(group_id,model_id)
) STRICT;
CREATE TRIGGER lora_only BEFORE INSERT ON lora_group_members
WHEN (SELECT kind FROM model_resources WHERE id=NEW.model_id) != 'lora'
BEGIN SELECT RAISE(ABORT,'LoRA group requires a LoRA'); END;
CREATE TRIGGER lora_only_update BEFORE UPDATE OF model_id ON lora_group_members
WHEN (SELECT kind FROM model_resources WHERE id=NEW.model_id) != 'lora'
BEGIN SELECT RAISE(ABORT,'LoRA group requires a LoRA'); END;
CREATE TRIGGER preserve_lora_kind BEFORE UPDATE OF kind ON model_resources
WHEN NEW.kind != 'lora' AND EXISTS(SELECT 1 FROM lora_group_members WHERE model_id=OLD.id)
BEGIN SELECT RAISE(ABORT,'Resource is used by a LoRA group'); END;

CREATE TABLE workflow_revisions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    source_path TEXT,
    source_sha256 TEXT NOT NULL,
    format TEXT NOT NULL CHECK(format IN ('comfy_api','comfy_ui')),
    document_json TEXT NOT NULL CHECK(json_valid(document_json)),
    adapter_id TEXT,
    adapter_version INTEGER,
    capability_state TEXT NOT NULL DEFAULT 'unverified' CHECK(capability_state IN ('unverified','unsupported','verified'))
) STRICT;
CREATE TABLE workflow_bindings (
    workflow_revision_id TEXT NOT NULL REFERENCES workflow_revisions(id),
    semantic_key TEXT NOT NULL,
    graph_path_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(graph_path_json)),
    node_id TEXT NOT NULL,
    expected_class TEXT NOT NULL,
    input_name TEXT NOT NULL,
    value_type TEXT NOT NULL CHECK(value_type IN ('string','seed','integer','number','boolean')),
    PRIMARY KEY(workflow_revision_id,semantic_key)
) STRICT;
CREATE TABLE generation_jobs (
    id TEXT PRIMARY KEY,
    workflow_revision_id TEXT NOT NULL REFERENCES workflow_revisions(id),
    parent_job_id TEXT REFERENCES generation_jobs(id),
    state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','ready','submitting','submission_unknown','queued','running','awaiting_review','succeeded','failed','cancelled','interrupted')),
    seed TEXT NOT NULL CHECK(length(seed) BETWEEN 1 AND 20 AND seed NOT GLOB '*[^0-9]*' AND (seed='0' OR substr(seed,1,1)!='0') AND (length(seed)<20 OR seed<='18446744073709551615')),
    request_snapshot_json TEXT NOT NULL CHECK(json_valid(request_snapshot_json)),
    endpoint_url TEXT,
    comfy_prompt_id TEXT,
    failure_json TEXT CHECK(failure_json IS NULL OR json_valid(failure_json)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE INDEX jobs_state ON generation_jobs(state,created_at);
CREATE TABLE pass_attempts (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES generation_jobs(id),
    phase_key TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK(attempt>0),
    state TEXT NOT NULL CHECK(state IN ('planned','submitting','submission_unknown','queued','running','awaiting_review','succeeded','failed','interrupted','cancelled')),
    comfy_prompt_id TEXT,
    effective_parameters_json TEXT CHECK(effective_parameters_json IS NULL OR json_valid(effective_parameters_json)),
    UNIQUE(job_id,phase_key,attempt)
) STRICT;
CREATE TABLE pass_artifacts (
    id TEXT PRIMARY KEY,
    pass_attempt_id TEXT NOT NULL REFERENCES pass_attempts(id),
    asset_id TEXT REFERENCES assets(id),
    kind TEXT NOT NULL CHECK(kind IN ('image_boundary','latent_state','sampler_state')),
    status TEXT NOT NULL DEFAULT 'candidate' CHECK(status IN ('candidate','validated','rejected','corrupt')),
    manifest_json TEXT NOT NULL CHECK(json_valid(manifest_json)),
    content_sha256 TEXT,
    resume_capability TEXT NOT NULL DEFAULT 'unverified' CHECK(resume_capability IN ('unverified','unsupported','verified'))
) STRICT;
CREATE TABLE human_reviews (
    id TEXT PRIMARY KEY,
    artifact_id TEXT NOT NULL REFERENCES pass_artifacts(id),
    decision TEXT NOT NULL CHECK(decision IN ('accept','reject')),
    note TEXT NOT NULL DEFAULT '',
    reviewed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE TABLE job_events (
    id INTEGER PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES generation_jobs(id),
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
    observed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE TABLE import_records (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL,
    source_sha256 TEXT NOT NULL,
    source_key TEXT NOT NULL,
    character_id TEXT REFERENCES characters(id),
    imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(source_path,source_sha256,source_key)
) STRICT;

-- Searchable projections; immutable request/output snapshots remain the source of truth.
CREATE TABLE job_characters (
    job_id TEXT NOT NULL REFERENCES generation_jobs(id),
    role TEXT NOT NULL CHECK(role IN ('P1','P2')),
    character_id TEXT NOT NULL REFERENCES characters(id),
    variant_id TEXT,
    snapshot_json TEXT NOT NULL CHECK(json_valid(snapshot_json)),
    PRIMARY KEY(job_id,role)
) STRICT;
CREATE INDEX job_characters_character ON job_characters(character_id,job_id);
CREATE INDEX job_characters_variant ON job_characters(variant_id,job_id);
CREATE TABLE job_outputs (
    job_id TEXT NOT NULL REFERENCES generation_jobs(id),
    asset_id TEXT NOT NULL REFERENCES assets(id),
    node_id TEXT,
    output_index INTEGER CHECK(output_index>=0),
    output_kind TEXT NOT NULL CHECK(output_kind IN ('saved','preview','unknown')),
    PRIMARY KEY(job_id,asset_id)
) STRICT;
CREATE INDEX job_outputs_asset ON job_outputs(asset_id,job_id);
CREATE INDEX generation_jobs_workflow ON generation_jobs(workflow_revision_id,id);
INSERT INTO job_characters(job_id,role,character_id,variant_id,snapshot_json)
SELECT j.id,json_extract(p.value,'$.role'),json_extract(p.value,'$.characterId'),json_extract(p.value,'$.variant.id'),p.value
FROM generation_jobs j,json_each(j.request_snapshot_json,'$.characters') p
WHERE json_extract(p.value,'$.role') IN ('P1','P2')
  AND EXISTS(SELECT 1 FROM characters c WHERE c.id=json_extract(p.value,'$.characterId'));
INSERT INTO job_outputs(job_id,asset_id,node_id,output_index,output_kind)
SELECT j.id,json_extract(o.value,'$.assetId'),json_extract(o.value,'$.nodeId'),json_extract(o.value,'$.outputIndex'),
CASE WHEN json_extract(o.value,'$.kind') IN ('saved','preview') THEN json_extract(o.value,'$.kind') ELSE 'unknown' END
FROM generation_jobs j,json_each(j.outputs_json) o
WHERE EXISTS(SELECT 1 FROM assets a WHERE a.id=json_extract(o.value,'$.assetId'));
INSERT OR IGNORE INTO asset_characters(asset_id,character_id,role,evidence)
SELECT o.asset_id,c.character_id,'depicted','inferred' FROM job_outputs o JOIN job_characters c ON c.job_id=o.job_id;
INSERT INTO schema_migrations(version,name) VALUES(6,'generation-provenance');

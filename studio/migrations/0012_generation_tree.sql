-- Additive generation tree. Relations only reference existing rows; no image file is ever copied.
-- parent_job_id carries no foreign key on purpose: a parent removed outside this application must
-- leave its children reachable and visibly orphaned instead of blocking cleanup or erasing the trace.
CREATE TABLE generation_relations (
    id TEXT PRIMARY KEY,
    parent_job_id TEXT NOT NULL,
    child_job_id TEXT REFERENCES generation_jobs(id) ON DELETE CASCADE,
    parent_checkpoint_id TEXT REFERENCES pass_artifacts(id),
    relation TEXT NOT NULL CHECK(relation IN ('variant','detailer','upscale','seed_change','correction','checkpoint_restore')),
    label TEXT NOT NULL DEFAULT '',
    state TEXT NOT NULL DEFAULT 'linked' CHECK(state IN ('linked','planned')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    CHECK(parent_job_id != child_job_id),
    CHECK((state='planned') = (child_job_id IS NULL))
) STRICT;
CREATE INDEX generation_relations_parent ON generation_relations(parent_job_id,id);
CREATE INDEX generation_relations_checkpoint ON generation_relations(parent_checkpoint_id);
-- One parent per child: the tree shape is enforced in the schema, cycles are refused by the application.
CREATE UNIQUE INDEX generation_relations_single_parent ON generation_relations(child_job_id) WHERE child_job_id IS NOT NULL;
INSERT INTO schema_migrations(version,name) VALUES(12,'generation-tree');

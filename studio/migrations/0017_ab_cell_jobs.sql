-- v17 : lien cellule A/B → job préparé (brouillon puis éventuellement soumis).
-- Migration strictement additive ; aucune table existante modifiée.
CREATE TABLE ab_cell_jobs (
    cell_id TEXT NOT NULL REFERENCES ab_cells(id),
    job_id TEXT NOT NULL REFERENCES generation_jobs(id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(cell_id, job_id)
) STRICT;
CREATE INDEX ab_cell_jobs_job ON ab_cell_jobs(job_id);
INSERT INTO schema_migrations(version,name) VALUES(17,'ab-cell-jobs');

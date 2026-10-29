-- v18 : origine vérifiable d'un job préparé (recette, scène, cellule A/B, module, manuel).
-- Migration strictement additive. La source est un identifiant ; aucune copie.
CREATE TABLE job_origins (
    job_id TEXT PRIMARY KEY REFERENCES generation_jobs(id),
    kind TEXT NOT NULL CHECK(kind IN ('recipe','scene','ab_cell','module','manual')),
    source_id TEXT,
    source_label TEXT NOT NULL DEFAULT '',
    source_version INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    CHECK((kind='manual') = (source_id IS NULL))
) STRICT;
CREATE INDEX job_origins_source ON job_origins(kind,source_id);
INSERT INTO schema_migrations(version,name) VALUES(18,'job-origins');

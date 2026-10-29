-- v16 : bibliothèque de modules ComfyUI réutilisables (m4).
-- Migration strictement additive : aucune table existante n'est modifiée.
CREATE TABLE workflow_modules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    current_version INTEGER NOT NULL DEFAULT 1 CHECK(current_version >= 1),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE TABLE workflow_module_versions (
    module_id TEXT NOT NULL REFERENCES workflow_modules(id),
    version INTEGER NOT NULL CHECK(version >= 1),
    manifest_json TEXT NOT NULL CHECK(json_valid(manifest_json)),
    document_json TEXT NOT NULL CHECK(json_valid(document_json)),
    source_workflow_id TEXT,
    source_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(module_id, version)
) STRICT;
-- Les versions sont immuables : le pointeur current_version de workflow_modules
-- est la seule chose qui bouge (rollback). Aucune réécriture ni purge possible.
CREATE TRIGGER workflow_module_versions_no_update
BEFORE UPDATE ON workflow_module_versions
BEGIN SELECT RAISE(ABORT, 'module versions are immutable'); END;
CREATE TRIGGER workflow_module_versions_no_delete
BEFORE DELETE ON workflow_module_versions
BEGIN SELECT RAISE(ABORT, 'module versions are immutable'); END;
INSERT INTO schema_migrations(version,name) VALUES(16,'workflow-modules');

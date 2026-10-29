CREATE TABLE scene_manifests (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    document_json TEXT NOT NULL CHECK(json_valid(document_json)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE INDEX scene_manifests_updated ON scene_manifests(updated_at);
INSERT INTO schema_migrations(version,name) VALUES(15,'scene-manifests');

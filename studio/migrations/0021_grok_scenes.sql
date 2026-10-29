CREATE TABLE grok_scenes (
 id TEXT PRIMARY KEY,
 source_path TEXT NOT NULL,
 source_sha256 TEXT NOT NULL,
 source_key TEXT NOT NULL,
 name TEXT NOT NULL,
 document_json TEXT NOT NULL CHECK(json_valid(document_json)),
 imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 UNIQUE(source_path,source_sha256,source_key)
) STRICT;
CREATE INDEX grok_scenes_source ON grok_scenes(source_path,source_key);
INSERT INTO schema_migrations(version,name) VALUES(21,'grok-scene-data');

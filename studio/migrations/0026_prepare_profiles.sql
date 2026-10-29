CREATE TABLE IF NOT EXISTS prepare_profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  workflow_revision_id TEXT NOT NULL REFERENCES workflow_revisions(id),
  mode TEXT NOT NULL CHECK(mode IN ('solo','duo')),
  config_json TEXT NOT NULL CHECK(json_valid(config_json)),
  signature_json TEXT NOT NULL CHECK(json_valid(signature_json)),
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE INDEX IF NOT EXISTS prepare_profiles_workflow ON prepare_profiles(workflow_revision_id, updated_at DESC);
INSERT OR IGNORE INTO schema_migrations(version,name) VALUES(26,'prepare-profiles');

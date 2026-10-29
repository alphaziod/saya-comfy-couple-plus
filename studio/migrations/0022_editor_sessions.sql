-- Interactive graph editor sessions, persisted across restarts.
-- The source workflow is never rewritten; this table only stores the editor
-- document and the preserved source snapshot needed to resume editing.
CREATE TABLE IF NOT EXISTS editor_sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    workflow TEXT NOT NULL DEFAULT '',
    source_format TEXT NOT NULL CHECK (source_format IN ('comfy_ui','comfy_api')),
    document_json TEXT NOT NULL,
    ui_source_json TEXT,
    original_api_json TEXT,
    original_ids_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS editor_sessions_updated ON editor_sessions(updated_at DESC);
INSERT INTO schema_migrations(version,name) VALUES(22,'editor-sessions');

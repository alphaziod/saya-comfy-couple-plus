CREATE TABLE IF NOT EXISTS image_blur_documents (
  asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
  document_json TEXT NOT NULL,
  document_sha256 TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
INSERT OR IGNORE INTO schema_migrations(version,name) VALUES(25,'image-blur-documents');

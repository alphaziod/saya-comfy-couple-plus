CREATE TABLE IF NOT EXISTS image_semantic_features (
    asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version>0),
    dim INTEGER NOT NULL CHECK(dim=1024),
    vector BLOB NOT NULL CHECK(length(vector)=4096),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE INDEX IF NOT EXISTS image_semantic_model ON image_semantic_features(model,version,asset_id);
INSERT OR IGNORE INTO schema_migrations(version,name) VALUES(24,'image-semantic-features');

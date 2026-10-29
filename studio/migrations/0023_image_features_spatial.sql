-- v2 embedding: 1 byte version + 8 pHash + 128 HSV + 48 spatial + 1 gradient = 717 bytes.
-- Keep v1 rows (525 bytes) readable as pending for the versioned backfill.
CREATE TABLE image_features_v2 (
    asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    extractor_version INTEGER NOT NULL CHECK(extractor_version>0),
    phash BLOB NOT NULL CHECK(length(phash)=8),
    vector BLOB NOT NULL CHECK(length(vector) IN (525,717)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
INSERT INTO image_features_v2(asset_id,extractor_version,phash,vector,created_at)
    SELECT asset_id,extractor_version,phash,vector,created_at FROM image_features;
DROP TABLE image_features;
ALTER TABLE image_features_v2 RENAME TO image_features;
CREATE INDEX image_features_extractor ON image_features(extractor_version,asset_id);
INSERT INTO schema_migrations(version,name) VALUES(23,'image-features-spatial');

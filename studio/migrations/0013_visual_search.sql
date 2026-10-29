-- Additive migration v13 : recherche visuelle locale.
-- Un vecteur de caractéristiques fixe (extracteur perceptuel pur Rust) par image indexée.
-- Aucune donnée existante n'est modifiée ; les images sans vecteur restent utilisables
-- et sont listées comme « en attente d'analyse visuelle ».
CREATE TABLE image_features (
    asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    extractor_version INTEGER NOT NULL CHECK(extractor_version>0),
    -- Empreinte perceptuelle DCT 8x8 (64 bits), stockée séparément pour les régressions.
    phash BLOB NOT NULL CHECK(length(phash)=8),
    -- Vecteur complet versionné : 1 octet de version + 8 octets pHash (u64 big-endian)
    -- + 128 f32 little-endian (histogramme HSV 8x4x4) + 1 f32 little-endian (gradient moyen).
    vector BLOB NOT NULL CHECK(length(vector)=525),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE INDEX image_features_extractor ON image_features(extractor_version,asset_id);
INSERT INTO schema_migrations(version,name) VALUES(13,'visual-search-features');

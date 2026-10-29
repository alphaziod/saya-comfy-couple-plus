CREATE TABLE character_reference_history (
    id INTEGER PRIMARY KEY,
    character_id TEXT NOT NULL REFERENCES characters(id),
    operation TEXT NOT NULL,
    changes_json TEXT NOT NULL CHECK(json_valid(changes_json)),
    saved_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE INDEX reference_history_character ON character_reference_history(character_id,id DESC);
-- Only the migration-time state is known; earlier edits are not reconstructed.
INSERT INTO character_reference_history(character_id,operation,changes_json)
SELECT character_id,'migration',json_object('before',json('[]'),'after',json_group_array(json_object(
    'assetId',asset_id,'variantId',nullif(variant_id,''),'variantName',variant_name,'position',position,'primary',json(CASE is_primary WHEN 1 THEN 'true' ELSE 'false' END))))
FROM character_references GROUP BY character_id;
CREATE TRIGGER reference_history_no_update BEFORE UPDATE ON character_reference_history BEGIN
    SELECT RAISE(ABORT,'Reference history is immutable');
END;
CREATE TRIGGER reference_history_no_delete BEFORE DELETE ON character_reference_history BEGIN
    SELECT RAISE(ABORT,'Reference history is immutable');
END;
INSERT INTO schema_migrations(version,name) VALUES(8,'reference-change-history');

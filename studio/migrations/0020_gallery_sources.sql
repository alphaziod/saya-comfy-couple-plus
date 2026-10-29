-- Preserve all asset IDs, annotations and historical references.
ALTER TABLE libraries ADD COLUMN scan_state TEXT NOT NULL DEFAULT 'idle';
ALTER TABLE libraries ADD COLUMN scan_error TEXT;
ALTER TABLE assets ADD COLUMN absolute_path TEXT NOT NULL DEFAULT '';
ALTER TABLE assets ADD COLUMN catalog_removed INTEGER NOT NULL DEFAULT 0 CHECK(catalog_removed IN (0,1));
UPDATE assets SET absolute_path=(SELECT rtrim(root_path,'/') || '/' || assets.relative_path FROM libraries WHERE id=assets.library_id);
CREATE INDEX assets_absolute_path ON assets(absolute_path);
CREATE TRIGGER asset_absolute_insert AFTER INSERT ON assets BEGIN
 UPDATE assets SET absolute_path=(SELECT rtrim(root_path,'/') || '/' || NEW.relative_path FROM libraries WHERE id=NEW.library_id) WHERE id=NEW.id;
END;
CREATE TRIGGER asset_absolute_move AFTER UPDATE OF library_id,relative_path ON assets BEGIN
 UPDATE assets SET absolute_path=(SELECT rtrim(root_path,'/') || '/' || NEW.relative_path FROM libraries WHERE id=NEW.library_id) WHERE id=NEW.id;
END;
INSERT INTO schema_migrations(version,name) VALUES(20,'gallery-sources');

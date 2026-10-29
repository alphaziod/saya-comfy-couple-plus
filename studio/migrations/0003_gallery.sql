ALTER TABLE assets ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1));
ALTER TABLE assets ADD COLUMN note TEXT NOT NULL DEFAULT '';
ALTER TABLE assets ADD COLUMN review TEXT NOT NULL DEFAULT 'unreviewed' CHECK(review IN ('unreviewed','accepted','rejected'));
CREATE INDEX assets_gallery_order ON assets(availability,relative_path,id);
INSERT INTO schema_migrations(version,name) VALUES(3,'gallery-reviews-and-filters');

-- The main gallery filters on capture_role/capture_phase (passes 2-5 hidden) and shows
-- capture_day/capture_famille. Those columns were missing from assets_gallery_cover, so
-- every page read went back to the table and through each row's inline metadata_json
-- (hundreds of KiB of ComfyUI workflow per image): 0.8 to 1.6 s per filter on a real
-- 6k-image library, 7 ms with the columns covered.
DROP INDEX IF EXISTS assets_gallery_cover;
CREATE INDEX assets_gallery_cover ON assets(availability,catalog_removed,absolute_path,modified_at,id,relative_path,width,height,metadata_state,favorite,note,review,content_sha256,byte_size,capture_role,capture_phase,capture_day,capture_famille);
-- Per-run copies of the Saya phase graphs (one per casting trial) were listed as user
-- workflows; mark them internal so the list hides them (job provenance is kept).
UPDATE workflow_revisions SET adapter_id='saya-phase'
 WHERE adapter_id='explicit-api' AND name GLOB 'Saya Couple · phase [1-6]';
INSERT OR IGNORE INTO schema_migrations(version,name) VALUES(31,'gallery-cover-capture');

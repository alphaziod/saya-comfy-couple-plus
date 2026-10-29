-- Lineage of imported Saya passes: the `saya_role` PNG chunk written by the Saya nodes
-- carries {"role","phase","total","lineage"}. The main gallery hides an imported pass 1
-- once the final of the same lineage is indexed; this expression index keeps that
-- lookup O(log n). The query must use exactly the same expression.
CREATE INDEX IF NOT EXISTS assets_saya_lineage ON assets(json_extract(metadata_json,'$.text.saya_role.lineage'));
INSERT OR IGNORE INTO schema_migrations(version,name) VALUES(30,'capture-lineage');

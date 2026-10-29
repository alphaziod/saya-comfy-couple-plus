CREATE TABLE preferences(key TEXT PRIMARY KEY, value_json TEXT NOT NULL CHECK(json_valid(value_json))) STRICT;
ALTER TABLE assets ADD COLUMN width INTEGER;
ALTER TABLE assets ADD COLUMN height INTEGER;
ALTER TABLE generation_jobs ADD COLUMN outputs_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(outputs_json));
INSERT INTO schema_migrations(version,name) VALUES(2,'functional-gallery-characters-generation');

-- Generation recipes: portable capture of a full generation's parameters.
-- Additive migration; existing tables and snapshots are untouched.
CREATE TABLE recipes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    -- Anchor to the origin revision when it exists locally; imported portable
    -- documents keep their origin inside content_json instead.
    workflow_id TEXT REFERENCES workflow_revisions(id),
    version INTEGER NOT NULL DEFAULT 1 CHECK(version>0),
    content_json TEXT NOT NULL CHECK(json_valid(content_json)),
    content_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE TABLE recipe_versions (
    recipe_id TEXT NOT NULL REFERENCES recipes(id),
    version INTEGER NOT NULL CHECK(version>0),
    content_json TEXT NOT NULL CHECK(json_valid(content_json)),
    content_sha256 TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    saved_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(recipe_id,version)
) STRICT;
CREATE INDEX recipe_versions_recipe ON recipe_versions(recipe_id,version);
INSERT INTO schema_migrations(version,name) VALUES(11,'generation-recipes');

CREATE TABLE character_references (
    character_id TEXT NOT NULL REFERENCES characters(id),
    variant_id TEXT NOT NULL DEFAULT '',
    variant_name TEXT,
    asset_id TEXT NOT NULL REFERENCES assets(id),
    position INTEGER NOT NULL CHECK(position>=0),
    is_primary INTEGER NOT NULL DEFAULT 0 CHECK(is_primary IN (0,1)),
    PRIMARY KEY(character_id,variant_id,asset_id),
    UNIQUE(character_id,variant_id,position)
) STRICT;
CREATE UNIQUE INDEX character_reference_primary ON character_references(character_id,variant_id) WHERE is_primary=1;
INSERT INTO character_references(character_id,asset_id,position)
SELECT character_id,asset_id,row_number() OVER(PARTITION BY character_id ORDER BY asset_id)-1
FROM asset_characters WHERE role='reference';
-- Legacy imports and gallery links still create an ordinary, unranked-by-user reference.
CREATE TRIGGER reference_link_added AFTER INSERT ON asset_characters
WHEN NEW.role='reference' AND NOT EXISTS(SELECT 1 FROM character_references WHERE character_id=NEW.character_id AND asset_id=NEW.asset_id)
BEGIN
    INSERT INTO character_references(character_id,asset_id,position)
    SELECT NEW.character_id,NEW.asset_id,coalesce(max(position)+1,0) FROM character_references WHERE character_id=NEW.character_id AND variant_id='';
END;
CREATE TRIGGER reference_link_removed AFTER DELETE ON asset_characters
WHEN OLD.role='reference'
BEGIN
    DELETE FROM character_references WHERE character_id=OLD.character_id AND asset_id=OLD.asset_id;
END;
INSERT INTO schema_migrations(version,name) VALUES(7,'ordered-character-references');

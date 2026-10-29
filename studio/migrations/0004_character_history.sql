CREATE TABLE character_history (
    character_id TEXT NOT NULL REFERENCES characters(id),
    revision INTEGER NOT NULL,
    name TEXT NOT NULL,
    identity_json TEXT NOT NULL CHECK(json_valid(identity_json)),
    archived INTEGER NOT NULL,
    variants_json TEXT NOT NULL CHECK(json_valid(variants_json)),
    saved_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY(character_id,revision)
) STRICT;
INSERT INTO character_history(character_id,revision,name,identity_json,archived,variants_json)
SELECT c.id,c.revision,c.name,c.identity_json,c.archived,
    (SELECT json_group_array(json_object('id',v.id,'name',v.name,'promptBlocks',json(v.prompt_blocks_json),'constraints',json(v.constraints_json),'revision',v.revision)) FROM character_variants v WHERE v.character_id=c.id)
FROM characters c;
CREATE TRIGGER character_history_insert AFTER INSERT ON characters
BEGIN
    INSERT INTO character_history(character_id,revision,name,identity_json,archived,variants_json)
    VALUES(NEW.id,NEW.revision,NEW.name,NEW.identity_json,NEW.archived,'[]');
END;
CREATE TRIGGER character_history_update AFTER UPDATE OF revision ON characters
BEGIN
    INSERT INTO character_history(character_id,revision,name,identity_json,archived,variants_json)
    SELECT NEW.id,NEW.revision,NEW.name,NEW.identity_json,NEW.archived,
        (SELECT json_group_array(json_object('id',v.id,'name',v.name,'promptBlocks',json(v.prompt_blocks_json),'constraints',json(v.constraints_json),'revision',v.revision)) FROM character_variants v WHERE v.character_id=NEW.id);
END;
INSERT INTO schema_migrations(version,name) VALUES(4,'character-history-and-portable-records');

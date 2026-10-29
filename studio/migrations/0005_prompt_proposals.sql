CREATE TABLE prompt_proposals (
 id TEXT PRIMARY KEY,
 engine_root TEXT NOT NULL,
 request_json TEXT NOT NULL CHECK(json_valid(request_json)),
 response_json TEXT NOT NULL CHECK(json_valid(response_json)),
 decision TEXT NOT NULL DEFAULT 'pending' CHECK(decision IN ('pending','accept','reject')),
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
 decided_at TEXT
) STRICT;
INSERT INTO schema_migrations(version,name) VALUES(5,'local-prompt-proposals');

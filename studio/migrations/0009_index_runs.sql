CREATE TABLE index_runs (
    id INTEGER PRIMARY KEY,
    root_path TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('discovering','indexing','completed','failed','interrupted')),
    completed INTEGER NOT NULL DEFAULT 0 CHECK(completed>=0),
    total INTEGER NOT NULL DEFAULT 0 CHECK(total>=0),
    indexed INTEGER NOT NULL DEFAULT 0 CHECK(indexed>=0),
    unchanged INTEGER NOT NULL DEFAULT 0 CHECK(unchanged>=0),
    error_count INTEGER NOT NULL DEFAULT 0 CHECK(error_count>=0),
    last_path TEXT,
    started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
) STRICT;
CREATE TABLE index_errors (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL REFERENCES index_runs(id),
    path TEXT NOT NULL,
    message TEXT NOT NULL
) STRICT;
CREATE INDEX index_errors_run ON index_errors(run_id,id);
INSERT INTO schema_migrations(version,name) VALUES(9,'index-run-recovery');

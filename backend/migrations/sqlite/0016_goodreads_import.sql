CREATE TABLE goodreads_import_log (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    filename     TEXT NOT NULL,
    source       TEXT NOT NULL DEFAULT 'goodreads'
                 CHECK(source IN ('goodreads', 'storygraph')),
    status       TEXT NOT NULL DEFAULT 'pending'
                 CHECK(status IN ('pending', 'running', 'complete', 'failed')),
    total_rows   INTEGER,
    matched      INTEGER NOT NULL DEFAULT 0,
    unmatched    INTEGER NOT NULL DEFAULT 0,
    errors       TEXT,
    created_at   TEXT NOT NULL,
    completed_at TEXT
);

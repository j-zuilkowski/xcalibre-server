CREATE TABLE IF NOT EXISTS watch_folder_log (
    id           TEXT    PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    file_path    TEXT    NOT NULL,
    file_name    TEXT    NOT NULL,
    status       TEXT    NOT NULL DEFAULT 'pending',  -- pending | ingested | duplicate | error
    error        TEXT,
    book_id      TEXT    REFERENCES books(id) ON DELETE SET NULL,
    detected_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    processed_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_watch_folder_log_detected_at  ON watch_folder_log(detected_at);
CREATE INDEX IF NOT EXISTS idx_watch_folder_log_status       ON watch_folder_log(status);

CREATE TABLE IF NOT EXISTS watch_folder_log (
    id           VARCHAR(32)  NOT NULL PRIMARY KEY DEFAULT (LOWER(HEX(RANDOM_BYTES(16)))),
    file_path    TEXT         NOT NULL,
    file_name    VARCHAR(512) NOT NULL,
    status       VARCHAR(16)  NOT NULL DEFAULT 'pending',
    error        TEXT,
    book_id      VARCHAR(36)  REFERENCES books(id) ON DELETE SET NULL,
    detected_at  BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
    processed_at BIGINT
);

CREATE INDEX idx_watch_folder_log_detected_at ON watch_folder_log(detected_at);
CREATE INDEX idx_watch_folder_log_status      ON watch_folder_log(status);

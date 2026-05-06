CREATE TABLE IF NOT EXISTS google_drive_files (
    id            VARCHAR(32)  NOT NULL PRIMARY KEY DEFAULT (LOWER(HEX(RANDOM_BYTES(16)))),
    book_id       VARCHAR(36)  NOT NULL,
    local_path    TEXT         NOT NULL,
    drive_file_id VARCHAR(255) NOT NULL,
    drive_name    VARCHAR(512) NOT NULL,
    synced_at     BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
    bytes         BIGINT       NOT NULL DEFAULT 0,
    status        VARCHAR(16)  NOT NULL DEFAULT 'synced',
    CONSTRAINT fk_gdf_book FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

CREATE INDEX idx_gdf_book_id       ON google_drive_files(book_id);
CREATE INDEX idx_gdf_drive_file_id ON google_drive_files(drive_file_id);

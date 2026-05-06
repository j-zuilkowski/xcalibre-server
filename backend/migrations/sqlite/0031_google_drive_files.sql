CREATE TABLE IF NOT EXISTS google_drive_files (
    id           TEXT    PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    book_id      TEXT    NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    local_path   TEXT    NOT NULL,
    drive_file_id TEXT   NOT NULL,
    drive_name   TEXT    NOT NULL,
    synced_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    bytes        INTEGER NOT NULL DEFAULT 0,
    status       TEXT    NOT NULL DEFAULT 'synced'  -- synced | error | deleted
);

CREATE INDEX IF NOT EXISTS idx_gdf_book_id      ON google_drive_files(book_id);
CREATE INDEX IF NOT EXISTS idx_gdf_drive_file_id ON google_drive_files(drive_file_id);

CREATE TABLE kobo_devices (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id     TEXT NOT NULL UNIQUE,
    device_name   TEXT NOT NULL DEFAULT 'Kobo',
    sync_token    TEXT,
    last_sync_at  TEXT,
    created_at    TEXT NOT NULL
);

CREATE INDEX idx_kobo_devices_user_id ON kobo_devices(user_id);
CREATE INDEX idx_kobo_devices_device_id ON kobo_devices(device_id);

CREATE TABLE kobo_reading_state (
    id             TEXT PRIMARY KEY,
    device_id      TEXT NOT NULL REFERENCES kobo_devices(id) ON DELETE CASCADE,
    book_id        TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    kobo_position  TEXT,
    percent_read   REAL,
    last_modified  TEXT NOT NULL,
    UNIQUE(device_id, book_id)
);

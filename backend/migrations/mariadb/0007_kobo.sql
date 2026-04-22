CREATE TABLE kobo_devices (
    id           CHAR(36) PRIMARY KEY,
    user_id      CHAR(36) NOT NULL,
    device_id    VARCHAR(255) NOT NULL,
    device_name  VARCHAR(255) NOT NULL DEFAULT 'Kobo',
    sync_token   TEXT NULL,
    last_sync_at DATETIME NULL,
    created_at   DATETIME NOT NULL,
    UNIQUE KEY uq_kobo_devices_device_id (device_id),
    KEY idx_kobo_devices_user_id (user_id),
    KEY idx_kobo_devices_device_id (device_id),
    CONSTRAINT fk_kobo_devices_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE TABLE kobo_reading_state (
    id            CHAR(36) PRIMARY KEY,
    device_id     CHAR(36) NOT NULL,
    book_id       CHAR(36) NOT NULL,
    kobo_position TEXT NULL,
    percent_read  DOUBLE NULL,
    last_modified DATETIME NOT NULL,
    UNIQUE KEY uq_kobo_reading_state_device_book (device_id, book_id),
    CONSTRAINT fk_kobo_reading_state_device FOREIGN KEY (device_id) REFERENCES kobo_devices(id) ON DELETE CASCADE,
    CONSTRAINT fk_kobo_reading_state_book FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

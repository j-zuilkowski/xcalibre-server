CREATE TABLE download_history (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id       TEXT NOT NULL,
    format        TEXT NOT NULL,
    downloaded_at TEXT NOT NULL
);

CREATE INDEX idx_download_history_user ON download_history(user_id);
CREATE INDEX idx_download_history_book ON download_history(book_id);

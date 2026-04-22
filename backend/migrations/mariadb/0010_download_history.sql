CREATE TABLE download_history (
    id            CHAR(36) PRIMARY KEY,
    user_id       CHAR(36) NOT NULL,
    book_id       CHAR(36) NOT NULL,
    format        TEXT NOT NULL,
    downloaded_at DATETIME NOT NULL,
    CONSTRAINT fk_download_history_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_download_history_user ON download_history(user_id);
CREATE INDEX idx_download_history_book ON download_history(book_id);

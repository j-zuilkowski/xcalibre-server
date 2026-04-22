CREATE TABLE book_user_state (
    user_id     CHAR(36) NOT NULL,
    book_id     CHAR(36) NOT NULL,
    is_read     TINYINT(1) NOT NULL DEFAULT 0,
    is_archived TINYINT(1) NOT NULL DEFAULT 0,
    updated_at  DATETIME NOT NULL,
    PRIMARY KEY (user_id, book_id),
    CONSTRAINT fk_book_user_state_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT fk_book_user_state_book FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

CREATE INDEX idx_book_user_state_user ON book_user_state(user_id);

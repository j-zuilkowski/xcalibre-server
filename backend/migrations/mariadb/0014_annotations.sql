CREATE TABLE book_annotations (
    id CHAR(36) PRIMARY KEY,
    user_id CHAR(36) NOT NULL,
    book_id CHAR(36) NOT NULL,
    type VARCHAR(16) NOT NULL CHECK(type IN ('highlight', 'note', 'bookmark')),
    cfi_range TEXT NOT NULL,
    highlighted_text TEXT NULL,
    note TEXT NULL,
    color VARCHAR(16) NOT NULL DEFAULT 'yellow'
          CHECK(color IN ('yellow', 'green', 'blue', 'pink')),
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

CREATE INDEX idx_annotations_user_book ON book_annotations(user_id, book_id);

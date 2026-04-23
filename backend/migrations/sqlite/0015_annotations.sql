CREATE TABLE book_annotations (
    id               TEXT PRIMARY KEY,
    user_id          TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id          TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    type             TEXT NOT NULL CHECK(type IN ('highlight', 'note', 'bookmark')),
    cfi_range        TEXT NOT NULL,
    highlighted_text TEXT,
    note             TEXT,
    color            TEXT NOT NULL DEFAULT 'yellow'
                       CHECK(color IN ('yellow', 'green', 'blue', 'pink')),
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE INDEX idx_annotations_user_book ON book_annotations(user_id, book_id);

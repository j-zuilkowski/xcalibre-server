CREATE TABLE book_chunks (
    id            TEXT PRIMARY KEY,
    book_id       TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chunk_index   INTEGER NOT NULL,
    chapter_index INTEGER NOT NULL,
    heading_path  TEXT,
    chunk_type    TEXT NOT NULL DEFAULT 'text'
                    CHECK(chunk_type IN ('text', 'procedure', 'reference',
                                         'concept', 'example', 'image')),
    text          TEXT NOT NULL,
    word_count    INTEGER NOT NULL,
    has_image     INTEGER NOT NULL DEFAULT 0,
    embedding     BLOB,
    created_at    TEXT NOT NULL
);

CREATE INDEX idx_book_chunks_book ON book_chunks(book_id, chunk_index);
CREATE INDEX idx_book_chunks_type ON book_chunks(book_id, chunk_type);

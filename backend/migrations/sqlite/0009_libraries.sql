CREATE TABLE libraries (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    calibre_db_path TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

INSERT INTO libraries (id, name, calibre_db_path, created_at, updated_at)
VALUES ('default', 'Default Library', '', datetime('now'), datetime('now'));

ALTER TABLE books ADD COLUMN library_id TEXT NOT NULL DEFAULT 'default'
    REFERENCES libraries(id);
CREATE INDEX idx_books_library_id ON books(library_id);

ALTER TABLE users ADD COLUMN default_library_id TEXT NOT NULL DEFAULT 'default'
    REFERENCES libraries(id);

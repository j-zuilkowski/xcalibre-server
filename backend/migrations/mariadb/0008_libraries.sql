CREATE TABLE libraries (
    id CHAR(36) PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    calibre_db_path TEXT NOT NULL,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

INSERT INTO libraries (id, name, calibre_db_path, created_at, updated_at)
VALUES ('default', 'Default Library', '', NOW(), NOW());

ALTER TABLE books
    ADD COLUMN library_id CHAR(36) NOT NULL DEFAULT 'default';
CREATE INDEX idx_books_library_id ON books(library_id);
ALTER TABLE books
    ADD CONSTRAINT fk_books_library_id FOREIGN KEY (library_id) REFERENCES libraries(id);

ALTER TABLE users
    ADD COLUMN default_library_id CHAR(36) NOT NULL DEFAULT 'default';
ALTER TABLE users
    ADD CONSTRAINT fk_users_default_library_id FOREIGN KEY (default_library_id) REFERENCES libraries(id);

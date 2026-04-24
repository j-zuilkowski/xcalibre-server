CREATE TABLE collections (
    id          VARCHAR(36) PRIMARY KEY,
    name        VARCHAR(255) NOT NULL,
    description TEXT,
    domain      VARCHAR(32) NOT NULL DEFAULT 'technical',
    owner_id    VARCHAR(36) NOT NULL,
    is_public   TINYINT(1) NOT NULL DEFAULT 0,
    created_at  DATETIME(6) NOT NULL,
    updated_at  DATETIME(6) NOT NULL,
    CONSTRAINT fk_collections_owner FOREIGN KEY (owner_id) REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT chk_collections_domain CHECK (domain IN ('technical','electronics','culinary','legal','academic','narrative'))
);

CREATE TABLE collection_books (
    collection_id VARCHAR(36) NOT NULL,
    book_id       VARCHAR(36) NOT NULL,
    added_at      DATETIME(6) NOT NULL,
    PRIMARY KEY (collection_id, book_id),
    CONSTRAINT fk_collection_books_collection FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
    CONSTRAINT fk_collection_books_book FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);
CREATE INDEX idx_collection_books_collection ON collection_books(collection_id);
CREATE INDEX idx_collection_books_book ON collection_books(book_id);

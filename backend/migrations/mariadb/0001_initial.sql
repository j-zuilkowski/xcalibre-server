CREATE TABLE roles (
    id CHAR(36) PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    can_upload TINYINT(1) NOT NULL DEFAULT 0,
    can_bulk TINYINT(1) NOT NULL DEFAULT 0,
    can_edit TINYINT(1) NOT NULL DEFAULT 1,
    can_download TINYINT(1) NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL,
    last_modified DATETIME NOT NULL
);

CREATE TABLE users (
    id CHAR(36) PRIMARY KEY,
    username VARCHAR(255) NOT NULL UNIQUE,
    email VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    role_id CHAR(36) NOT NULL,
    is_active TINYINT(1) NOT NULL DEFAULT 1,
    force_pw_reset TINYINT(1) NOT NULL DEFAULT 0,
    login_attempts INT NOT NULL DEFAULT 0,
    locked_until DATETIME NULL,
    created_at DATETIME NOT NULL,
    last_modified DATETIME NOT NULL,
    FOREIGN KEY (role_id) REFERENCES roles(id)
);

CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);

CREATE TABLE refresh_tokens (
    id CHAR(36) PRIMARY KEY,
    user_id CHAR(36) NOT NULL,
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL,
    revoked_at DATETIME NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX idx_refresh_tokens_hash ON refresh_tokens(token_hash);

CREATE TABLE authors (
    id CHAR(36) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    sort_name VARCHAR(255) NOT NULL,
    last_modified DATETIME NOT NULL
);

CREATE INDEX idx_authors_sort ON authors(sort_name);

CREATE TABLE series (
    id CHAR(36) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    sort_name VARCHAR(255) NOT NULL,
    last_modified DATETIME NOT NULL
);

CREATE TABLE tags (
    id CHAR(36) PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    source VARCHAR(32) NOT NULL DEFAULT 'manual',
    last_modified DATETIME NOT NULL,
    CHECK (source IN ('manual', 'llm', 'calibre_import'))
);

CREATE TABLE books (
    id CHAR(36) PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    sort_title VARCHAR(255) NOT NULL,
    description TEXT NULL,
    pubdate DATE NULL,
    language VARCHAR(32) NULL,
    rating INTEGER NULL,
    series_id CHAR(36) NULL,
    series_index DOUBLE NULL,
    has_cover TINYINT(1) NOT NULL DEFAULT 0,
    cover_path TEXT NULL,
    flags JSON NULL,
    indexed_at DATETIME NULL,
    created_at DATETIME NOT NULL,
    last_modified DATETIME NOT NULL,
    FOREIGN KEY (series_id) REFERENCES series(id) ON DELETE SET NULL,
    CHECK (rating IS NULL OR rating BETWEEN 0 AND 10)
);

CREATE INDEX idx_books_sort_title ON books(sort_title);
CREATE INDEX idx_books_series ON books(series_id);
CREATE INDEX idx_books_pubdate ON books(pubdate);
CREATE INDEX idx_books_language ON books(language);
CREATE INDEX idx_books_indexed_at ON books(indexed_at);

CREATE TABLE book_authors (
    book_id CHAR(36) NOT NULL,
    author_id CHAR(36) NOT NULL,
    display_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (book_id, author_id),
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE,
    FOREIGN KEY (author_id) REFERENCES authors(id) ON DELETE CASCADE
);

CREATE INDEX idx_book_authors_author ON book_authors(author_id);

CREATE TABLE book_tags (
    book_id CHAR(36) NOT NULL,
    tag_id CHAR(36) NOT NULL,
    confirmed TINYINT(1) NOT NULL DEFAULT 1,
    PRIMARY KEY (book_id, tag_id),
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE INDEX idx_book_tags_tag ON book_tags(tag_id);
CREATE INDEX idx_book_tags_pending ON book_tags(confirmed);

CREATE TABLE formats (
    id CHAR(36) PRIMARY KEY,
    book_id CHAR(36) NOT NULL,
    format VARCHAR(32) NOT NULL,
    path TEXT NOT NULL,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL,
    last_modified DATETIME NOT NULL,
    UNIQUE KEY uq_formats_book_format (book_id, format),
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

CREATE INDEX idx_formats_book ON formats(book_id);
CREATE INDEX idx_formats_format ON formats(format);

CREATE TABLE identifiers (
    id CHAR(36) PRIMARY KEY,
    book_id CHAR(36) NOT NULL,
    id_type VARCHAR(64) NOT NULL,
    value VARCHAR(255) NOT NULL,
    last_modified DATETIME NOT NULL,
    UNIQUE KEY uq_identifiers_book_type (book_id, id_type),
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

CREATE INDEX idx_identifiers_book ON identifiers(book_id);
CREATE INDEX idx_identifiers_value ON identifiers(value);

CREATE TABLE shelves (
    id CHAR(36) PRIMARY KEY,
    user_id CHAR(36) NOT NULL,
    name VARCHAR(255) NOT NULL,
    is_public TINYINT(1) NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL,
    last_modified DATETIME NOT NULL,
    UNIQUE KEY uq_shelves_user_name (user_id, name),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_shelves_user ON shelves(user_id);

CREATE TABLE shelf_books (
    shelf_id CHAR(36) NOT NULL,
    book_id CHAR(36) NOT NULL,
    display_order INTEGER NOT NULL DEFAULT 0,
    added_at DATETIME NOT NULL,
    PRIMARY KEY (shelf_id, book_id),
    FOREIGN KEY (shelf_id) REFERENCES shelves(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

CREATE INDEX idx_shelf_books_book ON shelf_books(book_id);

CREATE TABLE reading_progress (
    id CHAR(36) PRIMARY KEY,
    user_id CHAR(36) NOT NULL,
    book_id CHAR(36) NOT NULL,
    format_id CHAR(36) NOT NULL,
    cfi TEXT NULL,
    page INTEGER NULL,
    percentage DOUBLE NOT NULL DEFAULT 0.0,
    updated_at DATETIME NOT NULL,
    last_modified DATETIME NOT NULL,
    UNIQUE KEY uq_progress_user_book (user_id, book_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE,
    FOREIGN KEY (format_id) REFERENCES formats(id) ON DELETE CASCADE
);

CREATE INDEX idx_progress_user ON reading_progress(user_id);
CREATE INDEX idx_progress_book ON reading_progress(book_id);

CREATE TABLE custom_columns (
    id CHAR(36) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    label VARCHAR(255) NOT NULL UNIQUE,
    column_type VARCHAR(32) NOT NULL,
    is_multiple TINYINT(1) NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL
);

CREATE TABLE book_custom_values (
    id CHAR(36) PRIMARY KEY,
    book_id CHAR(36) NOT NULL,
    column_id CHAR(36) NOT NULL,
    value_text TEXT NULL,
    value_int BIGINT NULL,
    value_float DOUBLE NULL,
    value_bool TINYINT(1) NULL,
    UNIQUE KEY uq_book_custom_values_book_column (book_id, column_id),
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE,
    FOREIGN KEY (column_id) REFERENCES custom_columns(id) ON DELETE CASCADE
);

CREATE INDEX idx_custom_values_book ON book_custom_values(book_id);

CREATE TABLE llm_jobs (
    id CHAR(36) PRIMARY KEY,
    job_type VARCHAR(64) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    book_id CHAR(36) NULL,
    payload_json JSON NULL,
    result_json JSON NULL,
    error_text TEXT NULL,
    created_at DATETIME NOT NULL,
    started_at DATETIME NULL,
    completed_at DATETIME NULL,
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE,
    CHECK (job_type IN ('classify', 'semantic_index', 'quality_check', 'validate_metadata', 'organize', 'derive', 'backup')),
    CHECK (status IN ('pending', 'running', 'completed', 'failed'))
);

CREATE INDEX idx_llm_jobs_status ON llm_jobs(status);
CREATE INDEX idx_llm_jobs_book ON llm_jobs(book_id);
CREATE INDEX idx_llm_jobs_type ON llm_jobs(job_type);

CREATE TABLE llm_eval_results (
    id CHAR(36) PRIMARY KEY,
    fixture_name VARCHAR(255) NOT NULL,
    model_id VARCHAR(255) NOT NULL,
    prompt_hash VARCHAR(255) NOT NULL,
    role VARCHAR(64) NOT NULL,
    passed TINYINT(1) NOT NULL,
    results_json JSON NOT NULL,
    latency_ms INTEGER NOT NULL,
    run_at DATETIME NOT NULL
);

CREATE INDEX idx_eval_fixture ON llm_eval_results(fixture_name);
CREATE INDEX idx_eval_model ON llm_eval_results(model_id);
CREATE INDEX idx_eval_hash ON llm_eval_results(prompt_hash);

CREATE TABLE migration_log (
    id CHAR(36) PRIMARY KEY,
    source_path TEXT NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    records_total INTEGER NOT NULL DEFAULT 0,
    records_imported INTEGER NOT NULL DEFAULT 0,
    records_failed INTEGER NOT NULL DEFAULT 0,
    records_skipped INTEGER NOT NULL DEFAULT 0,
    started_at DATETIME NOT NULL,
    completed_at DATETIME NULL,
    log_json JSON NULL,
    CHECK (status IN ('pending', 'running', 'completed', 'failed'))
);

CREATE TABLE audit_log (
    id CHAR(36) PRIMARY KEY,
    user_id CHAR(36) NULL,
    action VARCHAR(32) NOT NULL,
    entity VARCHAR(32) NOT NULL,
    entity_id CHAR(36) NOT NULL,
    diff_json JSON NULL,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL,
    CHECK (action IN ('create', 'update', 'delete', 'tag_confirm', 'tag_reject')),
    CHECK (entity IN ('book', 'author', 'tag', 'series', 'shelf', 'user'))
);

CREATE INDEX idx_audit_user ON audit_log(user_id);
CREATE INDEX idx_audit_entity ON audit_log(entity, entity_id);
CREATE INDEX idx_audit_created ON audit_log(created_at);

CREATE TABLE book_embeddings (
    book_id CHAR(36) PRIMARY KEY,
    model_id VARCHAR(255) NOT NULL,
    embedding BLOB NOT NULL,
    created_at DATETIME NOT NULL,
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

INSERT INTO roles (id, name, can_upload, can_bulk, can_edit, can_download, created_at, last_modified)
VALUES
    ('admin', 'admin', 1, 1, 1, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP),
    ('user', 'user', 0, 0, 1, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

PRAGMA foreign_keys = ON;

CREATE TABLE roles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    can_upload INTEGER NOT NULL DEFAULT 0,
    can_bulk INTEGER NOT NULL DEFAULT 0,
    can_edit INTEGER NOT NULL DEFAULT 1,
    can_download INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    last_modified TEXT NOT NULL
);

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role_id TEXT NOT NULL REFERENCES roles(id),
    is_active INTEGER NOT NULL DEFAULT 1,
    force_pw_reset INTEGER NOT NULL DEFAULT 0,
    login_attempts INTEGER NOT NULL DEFAULT 0,
    locked_until TEXT,
    created_at TEXT NOT NULL,
    last_modified TEXT NOT NULL
);

CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);

CREATE TABLE refresh_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    revoked_at TEXT
);

CREATE INDEX idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX idx_refresh_tokens_hash ON refresh_tokens(token_hash);

CREATE TABLE authors (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    sort_name TEXT NOT NULL,
    last_modified TEXT NOT NULL
);

CREATE INDEX idx_authors_sort ON authors(sort_name);

CREATE TABLE series (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    sort_name TEXT NOT NULL,
    last_modified TEXT NOT NULL
);

CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL DEFAULT 'manual' CHECK(source IN ('manual', 'llm', 'calibre_import')),
    last_modified TEXT NOT NULL
);

CREATE TABLE books (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    sort_title TEXT NOT NULL,
    description TEXT,
    pubdate TEXT,
    language TEXT,
    rating INTEGER CHECK(rating BETWEEN 0 AND 10),
    series_id TEXT REFERENCES series(id) ON DELETE SET NULL,
    series_index REAL,
    has_cover INTEGER NOT NULL DEFAULT 0,
    cover_path TEXT,
    flags TEXT,
    indexed_at TEXT,
    created_at TEXT NOT NULL,
    last_modified TEXT NOT NULL
);

CREATE INDEX idx_books_sort_title ON books(sort_title);
CREATE INDEX idx_books_series ON books(series_id);
CREATE INDEX idx_books_pubdate ON books(pubdate);
CREATE INDEX idx_books_language ON books(language);
CREATE INDEX idx_books_indexed_at ON books(indexed_at);

CREATE TABLE book_authors (
    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    author_id TEXT NOT NULL REFERENCES authors(id) ON DELETE CASCADE,
    display_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (book_id, author_id)
);

CREATE INDEX idx_book_authors_author ON book_authors(author_id);

CREATE TABLE book_tags (
    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    confirmed INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (book_id, tag_id)
);

CREATE INDEX idx_book_tags_tag ON book_tags(tag_id);
CREATE INDEX idx_book_tags_pending ON book_tags(confirmed) WHERE confirmed = 0;

CREATE TABLE formats (
    id TEXT PRIMARY KEY,
    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    format TEXT NOT NULL,
    path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    last_modified TEXT NOT NULL,
    UNIQUE (book_id, format)
);

CREATE INDEX idx_formats_book ON formats(book_id);
CREATE INDEX idx_formats_format ON formats(format);

CREATE TABLE identifiers (
    id TEXT PRIMARY KEY,
    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    id_type TEXT NOT NULL,
    value TEXT NOT NULL,
    last_modified TEXT NOT NULL,
    UNIQUE (book_id, id_type)
);

CREATE INDEX idx_identifiers_book ON identifiers(book_id);
CREATE INDEX idx_identifiers_value ON identifiers(value);

CREATE TABLE shelves (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    is_public INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    last_modified TEXT NOT NULL,
    UNIQUE (user_id, name)
);

CREATE INDEX idx_shelves_user ON shelves(user_id);

CREATE TABLE shelf_books (
    shelf_id TEXT NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    display_order INTEGER NOT NULL DEFAULT 0,
    added_at TEXT NOT NULL,
    PRIMARY KEY (shelf_id, book_id)
);

CREATE INDEX idx_shelf_books_book ON shelf_books(book_id);

CREATE TABLE reading_progress (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    format_id TEXT NOT NULL REFERENCES formats(id) ON DELETE CASCADE,
    cfi TEXT,
    page INTEGER,
    percentage REAL NOT NULL DEFAULT 0.0,
    updated_at TEXT NOT NULL,
    last_modified TEXT NOT NULL,
    UNIQUE (user_id, book_id)
);

CREATE INDEX idx_progress_user ON reading_progress(user_id);
CREATE INDEX idx_progress_book ON reading_progress(book_id);

CREATE TABLE custom_columns (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    label TEXT NOT NULL UNIQUE,
    column_type TEXT NOT NULL,
    is_multiple INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE TABLE book_custom_values (
    id TEXT PRIMARY KEY,
    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    column_id TEXT NOT NULL REFERENCES custom_columns(id) ON DELETE CASCADE,
    value_text TEXT,
    value_int INTEGER,
    value_float REAL,
    value_bool INTEGER,
    UNIQUE (book_id, column_id)
);

CREATE INDEX idx_custom_values_book ON book_custom_values(book_id);

CREATE TABLE llm_jobs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL CHECK(job_type IN ('classify', 'semantic_index', 'quality_check', 'validate_metadata', 'organize', 'derive')),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'running', 'completed', 'failed')),
    book_id TEXT REFERENCES books(id) ON DELETE CASCADE,
    payload_json TEXT,
    result_json TEXT,
    error_text TEXT,
    created_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT
);

CREATE INDEX idx_llm_jobs_status ON llm_jobs(status);
CREATE INDEX idx_llm_jobs_book ON llm_jobs(book_id);
CREATE INDEX idx_llm_jobs_type ON llm_jobs(job_type);

CREATE TABLE llm_eval_results (
    id TEXT PRIMARY KEY,
    fixture_name TEXT NOT NULL,
    model_id TEXT NOT NULL,
    prompt_hash TEXT NOT NULL,
    role TEXT NOT NULL,
    passed INTEGER NOT NULL,
    results_json TEXT NOT NULL,
    latency_ms INTEGER NOT NULL,
    run_at TEXT NOT NULL
);

CREATE INDEX idx_eval_fixture ON llm_eval_results(fixture_name);
CREATE INDEX idx_eval_model ON llm_eval_results(model_id);
CREATE INDEX idx_eval_hash ON llm_eval_results(prompt_hash);

CREATE TABLE migration_log (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'running', 'completed', 'failed')),
    records_total INTEGER NOT NULL DEFAULT 0,
    records_imported INTEGER NOT NULL DEFAULT 0,
    records_failed INTEGER NOT NULL DEFAULT 0,
    records_skipped INTEGER NOT NULL DEFAULT 0,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    log_json TEXT
);

CREATE TABLE audit_log (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    action TEXT NOT NULL CHECK(action IN ('create', 'update', 'delete', 'tag_confirm', 'tag_reject')),
    entity TEXT NOT NULL CHECK(entity IN ('book', 'author', 'tag', 'series', 'shelf', 'user')),
    entity_id TEXT NOT NULL,
    diff_json TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_audit_user ON audit_log(user_id);
CREATE INDEX idx_audit_entity ON audit_log(entity, entity_id);
CREATE INDEX idx_audit_created ON audit_log(created_at);

CREATE TABLE book_embeddings (
    book_id TEXT PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
    model_id TEXT NOT NULL,
    embedding BLOB NOT NULL,
    created_at TEXT NOT NULL
);

INSERT INTO roles (id, name, can_upload, can_bulk, can_edit, can_download, created_at, last_modified)
VALUES
    ('admin', 'admin', 1, 1, 1, 1, datetime('now'), datetime('now')),
    ('user', 'user', 0, 0, 1, 1, datetime('now'), datetime('now'));

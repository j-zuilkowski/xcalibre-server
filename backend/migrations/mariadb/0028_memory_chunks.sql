CREATE TABLE IF NOT EXISTS memory_chunks (
    id           VARCHAR(32)  NOT NULL DEFAULT '',
    session_id   VARCHAR(255),
    project_path VARCHAR(1024),
    chunk_type   VARCHAR(32)  NOT NULL DEFAULT 'episodic',
    text         LONGTEXT     NOT NULL,
    tags         TEXT,
    model_id     VARCHAR(255) NOT NULL DEFAULT '',
    embedding    LONGBLOB,
    created_at   BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
    PRIMARY KEY (id),
    INDEX idx_memory_chunks_session_id (session_id),
    INDEX idx_memory_chunks_project_path (project_path(255)),
    INDEX idx_memory_chunks_created_at (created_at),
    FULLTEXT INDEX memory_chunks_fts (text)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE goodreads_import_log (
    id           CHAR(36) PRIMARY KEY,
    user_id      CHAR(36) NOT NULL,
    filename     VARCHAR(255) NOT NULL,
    source       VARCHAR(32) NOT NULL DEFAULT 'goodreads'
                 CHECK(source IN ('goodreads', 'storygraph')),
    status       VARCHAR(32) NOT NULL DEFAULT 'pending'
                 CHECK(status IN ('pending', 'running', 'complete', 'failed')),
    total_rows   INT NULL,
    matched      INT NOT NULL DEFAULT 0,
    unmatched    INT NOT NULL DEFAULT 0,
    errors       TEXT NULL,
    created_at   DATETIME NOT NULL,
    completed_at DATETIME NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

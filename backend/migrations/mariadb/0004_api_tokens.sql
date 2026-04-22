CREATE TABLE api_tokens (
    id CHAR(36) PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    token_hash CHAR(64) NOT NULL UNIQUE,
    created_by CHAR(36) NOT NULL,
    created_at DATETIME NOT NULL,
    last_used_at DATETIME NULL,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_api_tokens_created_by ON api_tokens(created_by);
CREATE INDEX idx_api_tokens_hash ON api_tokens(token_hash);
